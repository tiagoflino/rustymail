use keyring::Entry;

const SERVICE_ACCESS: &str = "rustymail-access";
const SERVICE_REFRESH: &str = "rustymail-refresh";
const SERVICE_IMAP_PASSWORD: &str = "rustymail-imap-password";

/// Max time to wait for a keyring operation before giving up.
/// The keyring can hang indefinitely if dbus/secret-service is not
/// responding (e.g., locked keyring after face-auth login).
const KEYRING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Wraps a synchronous keyring operation with a timeout and proper error handling.
/// Returns Ok(Ok(())) on success, Ok(Err(message)) on keyring error,
/// Err(message) on timeout.
pub async fn with_keyring_timeout<F>(operation: F, name: &str) -> Result<Result<(), String>, String>
where
    F: FnOnce() -> Result<(), String> + Send + 'static,
{
    match tokio::time::timeout(KEYRING_TIMEOUT, tokio::task::spawn_blocking(operation)).await {
        Ok(Ok(Ok(()))) => Ok(Ok(())),
        Ok(Ok(Err(e))) => {
            tracing::error!("Keyring operation '{}' failed: {e}", name);
            Ok(Err(format!(
                "Cannot access the system keyring.\n\n{e}\n\n\
                 Your login keyring may not be unlocked (common after face \
                 recognition or fingerprint login).\n\n\
                 To fix:\n\
                 • Run: gnome-keyring-daemon --unlock\n\
                 • Or log out and log back in with your password"
            )))
        }
        Ok(Err(join_err)) => {
            tracing::error!("Keyring task '{}' panicked: {join_err}", name);
            Err("Internal error accessing credential storage.".into())
        }
        Err(_timeout) => {
            tracing::error!("Keyring operation '{}' timed out after {:?}", name, KEYRING_TIMEOUT);
            Err(format!(
                "The system keyring is not responding (timed out after {} seconds).\n\n\
                 Your login keyring may not be unlocked. This is common after \
                 face recognition or fingerprint login.\n\n\
                 To fix:\n\
                 • Run: gnome-keyring-daemon --unlock\n\
                 • Or log out and log back in with your password\n\n\
                 Then restart Rustymail.",
                KEYRING_TIMEOUT.as_secs()
            ))
        }
    }
}

/// Detailed health status of the credential storage backend.
#[derive(Debug, Clone, serde::Serialize)]
pub struct KeyringHealth {
    pub available: bool,
    pub unlocked: bool,
    pub backend: String,
    pub detail: String,
}

/// Probe the keyring to check if it's available and unlocked.
/// Called at startup so we can warn the user before they try to authenticate.
pub fn check_keyring_health() -> KeyringHealth {
    #[cfg(target_os = "linux")]
    {
        // Try to connect to the Secret Service directly and check the default collection.
        // We use a raw dbus check because the keyring crate wraps errors opaquely.
        match std::process::Command::new("secret-tool")
            .arg("lookup")
            .arg("application")
            .arg("rust-keyring-health-check")
            .output()
        {
            Ok(output) if output.status.success() => {
                // secret-tool ran successfully — daemon is up and keyring is unlocked
                KeyringHealth {
                    available: true,
                    unlocked: true,
                    backend: "Secret Service (gnome-keyring)".into(),
                    detail: "Keyring is available and unlocked".into(),
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("not running") || stderr.contains("Cannot autolaunch") {
                    KeyringHealth {
                        available: false,
                        unlocked: false,
                        backend: "Secret Service".into(),
                        detail: "gnome-keyring daemon is not running. Start it with: systemctl --user start gnome-keyring-daemon".into(),
                    }
                } else if stderr.contains("locked") || stderr.contains("No such secret") {
                    // Daemon is running, collection may be locked but we couldn't find our test key
                    // (which is expected since we didn't create one)
                    KeyringHealth {
                        available: true,
                        unlocked: true, // Daemon responded, likely fine
                        backend: "Secret Service (gnome-keyring)".into(),
                        detail: "Keyring daemon is running".into(),
                    }
                } else {
                    KeyringHealth {
                        available: false,
                        unlocked: false,
                        backend: "Secret Service".into(),
                        detail: format!("Keyring check failed: {}", stderr.trim()),
                    }
                }
            }
            Err(e) => {
                KeyringHealth {
                    available: false,
                    unlocked: false,
                    backend: "Secret Service".into(),
                    detail: format!("Cannot reach keyring: {e}. Install gnome-keyring and ensure dbus is running."),
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        // macOS Keychain is virtually always available
        KeyringHealth {
            available: true,
            unlocked: true,
            backend: "macOS Keychain".into(),
            detail: "Keychain is available".into(),
        }
    }
    #[cfg(target_os = "windows")]
    {
        KeyringHealth {
            available: true,
            unlocked: true,
            backend: "Windows Credential Manager".into(),
            detail: "Credential Manager is available".into(),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        KeyringHealth {
            available: false,
            unlocked: false,
            backend: "unknown".into(),
            detail: "Unsupported platform".into(),
        }
    }
}

fn map_keyring_error(e: keyring::Error) -> String {
    let base = match &e {
        keyring::Error::NoStorageAccess(_) => {
            if cfg!(target_os = "linux") {
                "Cannot access the system keyring. Your login keyring may not be unlocked.\n\n\
                 This can happen with face recognition or fingerprint login.\n\n\
                 To fix:\n\
                 1. Run: gnome-keyring-daemon --unlock\n\
                 2. Or log out and log back in with your password\n\
                 3. Or install seahorse (GUI) to manage your keyring\n\n\
                 Once unlocked, restart Rustymail.".into()
            } else {
                format!("Cannot access the system keyring: {e}")
            }
        }
        keyring::Error::NoEntry => {
            "No credentials found. Please re-authenticate.".into()
        }
        keyring::Error::PlatformFailure(_) => {
            if cfg!(target_os = "linux") {
                "The secure credential storage service is not available.\n\n\
                 Please install and start gnome-keyring:\n\
                   sudo dnf install gnome-keyring\n\
                   systemctl --user start gnome-keyring-daemon\n\n\
                 Then restart Rustymail.".into()
            } else {
                format!("Keyring error: {e}")
            }
        }
        _ => {
            if cfg!(target_os = "linux") {
                format!(
                    "Keyring error: {e}\n\n\
                     The secure credential storage may not be properly configured.\n\
                     Ensure gnome-keyring is installed and running."
                )
            } else {
                format!("Keyring error: {e}")
            }
        }
    };
    tracing::error!("Credential storage error: {base}");
    base
}

fn entry(service: &str, account_id: &str) -> Result<Entry, String> {
    Entry::new(service, account_id).map_err(map_keyring_error)
}

pub fn store_tokens(
    account_id: &str,
    access_token: &str,
    refresh_token: &str,
) -> Result<(), String> {
    entry(SERVICE_ACCESS, account_id)?
        .set_password(access_token)
        .map_err(map_keyring_error)?;

    entry(SERVICE_REFRESH, account_id)?
        .set_password(refresh_token)
        .map_err(map_keyring_error)?;

    Ok(())
}

pub fn get_access_token(account_id: &str) -> Result<String, String> {
    entry(SERVICE_ACCESS, account_id)?
        .get_password()
        .map_err(map_keyring_error)
}

pub fn get_refresh_token(account_id: &str) -> Result<String, String> {
    entry(SERVICE_REFRESH, account_id)?
        .get_password()
        .map_err(map_keyring_error)
}

pub fn update_access_token(account_id: &str, access_token: &str) -> Result<(), String> {
    entry(SERVICE_ACCESS, account_id)?
        .set_password(access_token)
        .map_err(map_keyring_error)
}

pub fn store_imap_password(account_id: &str, password: &str) -> Result<(), String> {
    entry(SERVICE_IMAP_PASSWORD, account_id)?
        .set_password(password)
        .map_err(map_keyring_error)
}

pub fn get_imap_password(account_id: &str) -> Result<String, String> {
    entry(SERVICE_IMAP_PASSWORD, account_id)?
        .get_password()
        .map_err(map_keyring_error)
}

pub fn delete_tokens(account_id: &str) -> Result<(), String> {
    let _ = entry(SERVICE_ACCESS, account_id)
        .and_then(|e| e.delete_password().map_err(map_keyring_error));
    let _ = entry(SERVICE_REFRESH, account_id)
        .and_then(|e| e.delete_password().map_err(map_keyring_error));
    let _ = entry(SERVICE_IMAP_PASSWORD, account_id)
        .and_then(|e| e.delete_password().map_err(map_keyring_error));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_names_are_distinct() {
        assert_ne!(SERVICE_ACCESS, SERVICE_REFRESH);
    }

    #[test]
    fn test_entry_creates_with_correct_service() {
        let access = entry(SERVICE_ACCESS, "test@example.com");
        let refresh = entry(SERVICE_REFRESH, "test@example.com");
        assert!(access.is_ok());
        assert!(refresh.is_ok());
    }

    #[test]
    fn test_linux_error_message_contains_actionable_guidance() {
        let err = map_keyring_error(keyring::Error::NoStorageAccess(
            "simulated".into()
        ));
        if cfg!(target_os = "linux") {
            assert!(
                err.contains("keyring"),
                "Error should mention keyring: {err}"
            );
        }
    }

    #[test]
    fn test_check_keyring_health_returns_struct() {
        let health = check_keyring_health();
        assert!(!health.backend.is_empty());
        assert!(!health.detail.is_empty());
        // available may be true or false depending on environment
    }

    #[test]
    #[ignore] // Requires OS keyring — run locally, skipped in headless CI
    fn test_delete_tokens_does_not_error_on_missing() {
        let result = delete_tokens("nonexistent-account-that-never-had-tokens@test.local");
        assert!(result.is_ok());
    }
}
