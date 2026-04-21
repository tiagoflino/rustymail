use super::accounts::get_active_account;
use tauri::Manager;

pub(crate) fn validate_external_url(url: &str) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Only http and https URLs are allowed".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    validate_external_url(&url)?;
    tauri_plugin_opener::open_url(&url, None::<&str>)
        .map_err(|e| format!("Failed to open URL: {}", e))
}

#[tauri::command]
pub async fn get_upcoming_events(
    app_handle: tauri::AppHandle,
) -> Result<Vec<crate::calendar_api::CalendarEvent>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::calendar_api::get_upcoming_events(&account.access_token).await
}

#[tauri::command]
pub fn get_file_size(path: String) -> Result<u64, String> {
    std::fs::metadata(&path)
        .map(|m| m.len())
        .map_err(|e| format!("Cannot read file: {}", e))
}

#[tauri::command]
pub async fn get_log_path(app_handle: tauri::AppHandle) -> Result<String, String> {
    let dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("logs").to_string_lossy().to_string())
}

#[tauri::command]
pub async fn get_recent_logs(app_handle: tauri::AppHandle, lines: Option<usize>) -> Result<String, String> {
    let dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let log_dir = dir.join("logs");

    let mut entries: Vec<_> = std::fs::read_dir(&log_dir)
        .map_err(|e| format!("Cannot read log directory: {}", e))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("rustymail.log"))
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.metadata().ok().and_then(|m| m.modified().ok())));

    let path = entries.first().ok_or("No log files found")?;
    let content = std::fs::read_to_string(path.path())
        .map_err(|e| format!("Cannot read log file: {}", e))?;

    let max_lines = lines.unwrap_or(200);
    let result: String = content.lines().rev().take(max_lines).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
    Ok(result)
}

#[tauri::command]
pub async fn open_log_directory(app_handle: tauri::AppHandle) -> Result<(), String> {
    let dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let log_dir = dir.join("logs");
    tauri_plugin_opener::open_url(format!("file://{}", log_dir.to_string_lossy()), None::<&str>)
        .map_err(|e| format!("Failed to open directory: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_url_javascript_rejected() {
        let result = validate_external_url("javascript:alert(1)");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Only http and https URLs are allowed"));
    }

    #[test]
    fn test_validate_url_ftp_rejected() {
        let result = validate_external_url("ftp://example.com");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Only http and https URLs are allowed"));
    }

    #[test]
    fn test_validate_url_empty_rejected() {
        let result = validate_external_url("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Only http and https URLs are allowed"));
    }

    #[test]
    fn test_validate_url_data_rejected() {
        assert!(validate_external_url("data:text/html,<h1>hi</h1>").is_err());
    }

    #[test]
    fn test_validate_url_http_accepted() {
        assert!(validate_external_url("http://example.com").is_ok());
    }

    #[test]
    fn test_validate_url_https_accepted() {
        assert!(validate_external_url("https://example.com").is_ok());
    }
}
