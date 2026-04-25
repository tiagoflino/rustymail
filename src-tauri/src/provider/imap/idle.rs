use super::connection::ImapConfig;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_native_tls::TlsStream;

const IDLE_TIMEOUT_SECS: u64 = 25 * 60;
const RECONNECT_BASE_DELAY_SECS: u64 = 5;
const RECONNECT_MAX_DELAY_SECS: u64 = 300;

pub struct IdleHandle {
    shutdown: Arc<AtomicBool>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl IdleHandle {
    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

impl Drop for IdleHandle {
    fn drop(&mut self) {
        self.stop();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

pub fn start_idle(
    config: ImapConfig,
    app_handle: tauri::AppHandle,
) -> IdleHandle {
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    let task = tokio::spawn(async move {
        idle_loop(config, app_handle, shutdown_clone).await;
    });

    IdleHandle {
        shutdown,
        task: Some(task),
    }
}

async fn idle_loop(
    config: ImapConfig,
    app_handle: tauri::AppHandle,
    shutdown: Arc<AtomicBool>,
) {
    let mut consecutive_failures: u32 = 0;

    loop {
        if shutdown.load(Ordering::Relaxed) {
            tracing::info!("IDLE loop shutting down for {}", config.account_id);
            break;
        }

        match run_idle_session(&config, &app_handle, &shutdown).await {
            Ok(()) => {
                consecutive_failures = 0;
            }
            Err(e) => {
                consecutive_failures += 1;
                let delay = std::cmp::min(
                    RECONNECT_BASE_DELAY_SECS * 2u64.pow(consecutive_failures.min(6)),
                    RECONNECT_MAX_DELAY_SECS,
                );
                tracing::warn!(
                    "IDLE session failed for {} (attempt {}): {}. Reconnecting in {}s",
                    config.account_id, consecutive_failures, e, delay
                );

                use tauri::Emitter;
                let _ = app_handle.emit("imap-connection-state", serde_json::json!({
                    "account_id": &config.account_id,
                    "state": "reconnecting",
                    "retry_in_secs": delay,
                }));

                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(delay)) => {},
                    _ = async {
                        loop {
                            if shutdown.load(Ordering::Relaxed) { break; }
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                    } => { break; }
                }
            }
        }
    }
}

async fn run_idle_session(
    config: &ImapConfig,
    app_handle: &tauri::AppHandle,
    shutdown: &Arc<AtomicBool>,
) -> Result<(), String> {
    let addr = (config.imap_host.as_str(), config.imap_port);
    let tcp = TcpStream::connect(addr)
        .await
        .map_err(|e| format!("Connect failed: {}", e))?;

    let native_connector = native_tls::TlsConnector::new()
        .map_err(|e| format!("TLS failed: {}", e))?;
    let tls_connector = tokio_native_tls::TlsConnector::from(native_connector);
    let tls_stream: TlsStream<TcpStream> = tls_connector
        .connect(&config.imap_host, tcp)
        .await
        .map_err(|e| format!("TLS handshake failed: {}", e))?;

    let client = async_imap::Client::new(tls_stream);
    let mut session = if config.auth_method == "oauth2" {
        let access_token = crate::credentials::get_access_token(&config.account_id)
            .map_err(|e| format!("Failed to get OAuth2 token: {}", e))?;
        let auth_string = format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            config.username, access_token
        );
        client
            .authenticate("XOAUTH2", super::connection::XOAuth2Authenticator(auth_string))
            .await
            .map_err(|e| format!("XOAUTH2 auth failed: {}", e.0))?
    } else {
        let password = crate::credentials::get_imap_password(&config.account_id)
            .map_err(|e| format!("Failed to get IMAP password: {}", e))?;
        client
            .login(&config.username, &password)
            .await
            .map_err(|e| format!("Login failed: {}", e.0))?
    };

    session
        .select("INBOX")
        .await
        .map_err(|e| format!("SELECT INBOX failed: {}", e))?;

    use tauri::Emitter;
    let _ = app_handle.emit("imap-connection-state", serde_json::json!({
        "account_id": &config.account_id,
        "state": "connected",
    }));

    tracing::info!("IDLE session started for {}", config.account_id);

    loop {
        if shutdown.load(Ordering::Relaxed) {
            let _ = session.logout().await;
            return Ok(());
        }

        let mut idle_handle = session.idle();

        idle_handle.init()
            .await
            .map_err(|e| format!("IDLE init failed: {}", e))?;

        let (wait_future, _stop_source) = idle_handle.wait_with_timeout(
            Duration::from_secs(IDLE_TIMEOUT_SECS),
        );

        let idle_result = tokio::select! {
            result = wait_future => { result }
            _ = async {
                loop {
                    if shutdown.load(Ordering::Relaxed) { break; }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            } => {
                let _ = idle_handle.done().await;
                return Ok(());
            }
        };

        session = match idle_handle.done().await {
            Ok(s) => s,
            Err(e) => return Err(format!("IDLE done failed: {}", e)),
        };

        match idle_result {
            Ok(reason) => {
                tracing::info!("IDLE event for {}: {:?}", config.account_id, reason);
                let _ = app_handle.emit("imap-new-mail", serde_json::json!({
                    "account_id": &config.account_id,
                }));
            }
            Err(e) => {
                return Err(format!("IDLE wait failed: {}", e));
            }
        }
    }
}

pub struct IdleManager {
    handles: Mutex<HashMap<String, IdleHandle>>,
}

impl Default for IdleManager {
    fn default() -> Self {
        Self::new()
    }
}

impl IdleManager {
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(HashMap::new()),
        }
    }

    pub async fn start_for_account(
        &self,
        config: ImapConfig,
        app_handle: tauri::AppHandle,
    ) {
        let account_id = config.account_id.clone();
        let mut handles = self.handles.lock().await;

        if handles.contains_key(&account_id) {
            return;
        }

        let handle = start_idle(config, app_handle);
        handles.insert(account_id.clone(), handle);
        tracing::info!("IDLE started for {}", account_id);
    }

    pub async fn stop_for_account(&self, account_id: &str) {
        let mut handles = self.handles.lock().await;
        if let Some(handle) = handles.remove(account_id) {
            handle.stop();
            tracing::info!("IDLE stopped for {}", account_id);
        }
    }

    pub async fn stop_all(&self) {
        let mut handles = self.handles.lock().await;
        for (id, handle) in handles.drain() {
            handle.stop();
            tracing::info!("IDLE stopped for {}", id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_idle_manager_new() {
        let manager = IdleManager::new();
        let handles = manager.handles.lock().await;
        assert!(handles.is_empty());
    }

    #[tokio::test]
    async fn test_idle_manager_stop_nonexistent() {
        let manager = IdleManager::new();
        manager.stop_for_account("nonexistent-account").await;
        let handles = manager.handles.lock().await;
        assert!(handles.is_empty());
    }

    #[tokio::test]
    async fn test_idle_manager_stop_all_empty() {
        let manager = IdleManager::new();
        manager.stop_all().await;
        let handles = manager.handles.lock().await;
        assert!(handles.is_empty());
    }

    #[test]
    fn test_idle_manager_default() {
        let manager = IdleManager::default();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let handles = manager.handles.lock().await;
            assert!(handles.is_empty());
        });
    }
}
