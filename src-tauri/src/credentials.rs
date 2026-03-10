use keyring::Entry;

const SERVICE_ACCESS: &str = "rustymail-access";
const SERVICE_REFRESH: &str = "rustymail-refresh";

fn map_keyring_error(e: keyring::Error) -> String {
    if cfg!(target_os = "linux") {
        format!(
            "Secure credential storage unavailable. Please install gnome-keyring \
             (sudo apt install gnome-keyring) or another Secret Service provider. \
             Details: {e}"
        )
    } else {
        format!("Keyring error: {e}")
    }
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

pub fn delete_tokens(account_id: &str) -> Result<(), String> {
    // Ignore errors on delete — token may not exist
    let _ = entry(SERVICE_ACCESS, account_id).and_then(|e| e.delete_password().map_err(map_keyring_error));
    let _ = entry(SERVICE_REFRESH, account_id).and_then(|e| e.delete_password().map_err(map_keyring_error));
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
        // Verifies Entry::new doesn't panic for valid inputs
        let access = entry(SERVICE_ACCESS, "test@example.com");
        let refresh = entry(SERVICE_REFRESH, "test@example.com");
        assert!(access.is_ok());
        assert!(refresh.is_ok());
    }

    #[test]
    fn test_linux_error_message_contains_install_hint() {
        let err = map_keyring_error(keyring::Error::NoEntry);
        if cfg!(target_os = "linux") {
            assert!(err.contains("gnome-keyring"), "Linux error should guide user to install gnome-keyring");
        } else {
            assert!(err.contains("Keyring error"));
        }
    }

    #[test]
    #[ignore] // Requires OS keyring — run locally, skipped in headless CI
    fn test_delete_tokens_does_not_error_on_missing() {
        // delete_tokens should succeed even if no tokens exist
        let result = delete_tokens("nonexistent-account-that-never-had-tokens@test.local");
        assert!(result.is_ok());
    }
}
