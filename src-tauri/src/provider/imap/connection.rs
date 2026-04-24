use async_imap::Session;
use tokio::net::TcpStream;
use tokio_native_tls::TlsStream;

pub type ImapSession = Session<TlsStream<TcpStream>>;

#[derive(Debug, Clone)]
pub struct ImapConfig {
    pub account_id: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub username: String,
    pub auth_method: String,
    pub use_tls: bool,
}

impl ImapConfig {
    pub async fn from_db(pool: &sqlx::SqlitePool, account_id: &str) -> Result<Self, String> {
        #[derive(sqlx::FromRow)]
        struct Row {
            imap_host: String,
            imap_port: i32,
            smtp_host: String,
            smtp_port: i32,
            auth_method: String,
            use_tls: i32,
        }

        let row = sqlx::query_as::<_, Row>(
            "SELECT imap_host, imap_port, smtp_host, smtp_port, auth_method, use_tls FROM imap_config WHERE account_id = ?",
        )
        .bind(account_id)
        .fetch_one(pool)
        .await
        .map_err(|e| format!("IMAP config not found for {}: {}", account_id, e))?;

        #[derive(sqlx::FromRow)]
        struct EmailRow {
            email: String,
        }
        let email_row = sqlx::query_as::<_, EmailRow>(
            "SELECT COALESCE(email, id) as email FROM accounts WHERE id = ?",
        )
        .bind(account_id)
        .fetch_one(pool)
        .await
        .map_err(|e| format!("Account not found: {}", e))?;

        Ok(Self {
            account_id: account_id.to_string(),
            imap_host: row.imap_host,
            imap_port: row.imap_port as u16,
            smtp_host: row.smtp_host,
            smtp_port: row.smtp_port as u16,
            username: email_row.email,
            auth_method: row.auth_method,
            use_tls: row.use_tls != 0,
        })
    }
}

pub async fn connect(config: &ImapConfig) -> Result<ImapSession, String> {
    let addr = (config.imap_host.as_str(), config.imap_port);
    let tcp = TcpStream::connect(addr)
        .await
        .map_err(|e| format!("Failed to connect to {}:{}: {}", config.imap_host, config.imap_port, e))?;

    let native_connector = native_tls::TlsConnector::new()
        .map_err(|e| format!("TLS connector creation failed: {}", e))?;
    let tls_connector = tokio_native_tls::TlsConnector::from(native_connector);
    let tls_stream = tls_connector
        .connect(&config.imap_host, tcp)
        .await
        .map_err(|e| format!("TLS handshake failed: {}", e))?;

    let client = async_imap::Client::new(tls_stream);

    let session = if config.auth_method == "oauth2" {
        let access_token = crate::credentials::get_access_token(&config.account_id)
            .map_err(|e| format!("Failed to get OAuth2 token: {}", e))?;
        let auth_string = format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            config.username, access_token
        );
        client
            .authenticate("XOAUTH2", XOAuth2Authenticator(auth_string))
            .await
            .map_err(|e| format!("XOAUTH2 auth failed: {}", e.0))?
    } else {
        let password = crate::credentials::get_imap_password(&config.account_id)
            .map_err(|e| format!("Failed to get IMAP password: {}", e))?;
        client
            .login(&config.username, &password)
            .await
            .map_err(|e| format!("IMAP login failed: {}", e.0))?
    };

    tracing::info!("IMAP connected to {} as {} ({})", config.imap_host, config.username, config.auth_method);
    Ok(session)
}

pub struct XOAuth2Authenticator(pub String);

impl async_imap::Authenticator for XOAuth2Authenticator {
    type Response = String;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        self.0.clone()
    }
}

pub async fn test_connection(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<String, String> {
    let addr = (host, port);
    let tcp = TcpStream::connect(addr)
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    let native_connector = native_tls::TlsConnector::new()
        .map_err(|e| format!("TLS connector creation failed: {}", e))?;
    let tls_connector = tokio_native_tls::TlsConnector::from(native_connector);
    let tls_stream = tls_connector
        .connect(host, tcp)
        .await
        .map_err(|e| format!("TLS failed: {}", e))?;

    let client = async_imap::Client::new(tls_stream);
    let mut session = client
        .login(username, password)
        .await
        .map_err(|e| format!("Login failed: {}", e.0))?;

    let caps = session.capabilities().await.map_err(|e| e.to_string())?;
    let cap_list: Vec<String> = caps.iter().map(|c| format!("{:?}", c)).collect();

    let _ = session.logout().await;
    Ok(format!("Connected successfully. Capabilities: {}", cap_list.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imap_config_fields() {
        let config = ImapConfig {
            account_id: "user@outlook.com".to_string(),
            imap_host: "outlook.office365.com".to_string(),
            imap_port: 993,
            smtp_host: "smtp.office365.com".to_string(),
            smtp_port: 587,
            username: "user@outlook.com".to_string(),
            auth_method: "password".to_string(),
            use_tls: true,
        };
        assert_eq!(config.imap_port, 993);
        assert_eq!(config.smtp_port, 587);
        assert!(config.use_tls);
    }

    #[tokio::test]
    async fn test_imap_config_from_db() {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        crate::db::apply_schema(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO accounts (id, email, display_name, is_active, created_at, provider_type) VALUES (?, ?, ?, 1, 0, 'imap')",
        )
        .bind("acc1")
        .bind("user@fastmail.com")
        .bind("Test User")
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO imap_config (account_id, imap_host, imap_port, smtp_host, smtp_port, auth_method, use_tls) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("acc1")
        .bind("imap.fastmail.com")
        .bind(993)
        .bind("smtp.fastmail.com")
        .bind(587)
        .bind("password")
        .bind(1)
        .execute(&pool)
        .await
        .unwrap();

        let config = ImapConfig::from_db(&pool, "acc1").await.unwrap();
        assert_eq!(config.imap_host, "imap.fastmail.com");
        assert_eq!(config.imap_port, 993);
        assert_eq!(config.smtp_host, "smtp.fastmail.com");
        assert_eq!(config.smtp_port, 587);
        assert_eq!(config.username, "user@fastmail.com");
        assert!(config.use_tls);
    }

    #[tokio::test]
    async fn test_imap_config_from_db_missing() {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        crate::db::apply_schema(&pool).await.unwrap();

        let result = ImapConfig::from_db(&pool, "nonexistent").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    use std::str::FromStr;
}
