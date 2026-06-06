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

    let provider_type = super::accounts::get_provider_type(pool.inner(), &account.id).await;
    if provider_type == "imap" {
        let caldav_url = sqlx::query_scalar::<_, Option<String>>(
            "SELECT caldav_url FROM imap_config WHERE account_id = ?",
        )
        .bind(&account.id)
        .fetch_optional(pool.inner())
        .await
        .map_err(|e| e.to_string())?
        .flatten();
        if let Some(url) = caldav_url {
            let config = crate::provider::imap::connection::ImapConfig::from_db(pool.inner(), &account.id).await?;
            let password = crate::credentials::get_imap_password(&account.id)?;
            let now = chrono::Utc::now();
            let start = now.to_rfc3339();
            let end = (now + chrono::Duration::days(7)).to_rfc3339();
            return crate::caldav_api::caldav_get_events(&url, &config.username, &password, &start, &end).await;
        }
        return Ok(vec![]);
    }
    if provider_type == "outlook" {
        let now = chrono::Utc::now();
        let start = now.to_rfc3339();
        let end = (now + chrono::Duration::days(7)).to_rfc3339();
        return crate::outlook_api::outlook_get_events(&account.access_token, &start, &end).await;
    }

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

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct PrivacyReport {
    pub total_blocked: i64,
    pub unique_senders_tracked: i64,
    pub blocked_this_week: i64,
    pub top_trackers: Vec<TrackerSender>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TrackerSender {
    pub sender_email: String,
    pub tracker_count: i64,
    pub tracker_types: String,
}

#[tauri::command]
pub async fn get_privacy_report(app_handle: tauri::AppHandle) -> Result<PrivacyReport, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    let now = chrono::Utc::now().timestamp();
    let week_ago = now - 7 * 86400;

    let total_blocked: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM tracking_events WHERE account_id = ? AND blocked = 1"
    ).bind(&account.id).fetch_one(pool.inner()).await.map_err(|e| e.to_string())?;

    let unique_senders: (i64,) = sqlx::query_as(
        "SELECT COUNT(DISTINCT sender_email) FROM tracking_events WHERE account_id = ?"
    ).bind(&account.id).fetch_one(pool.inner()).await.map_err(|e| e.to_string())?;

    let blocked_this_week: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM tracking_events WHERE account_id = ? AND blocked = 1 AND detected_at > ?"
    ).bind(&account.id).bind(week_ago).fetch_one(pool.inner()).await.map_err(|e| e.to_string())?;

    #[derive(sqlx::FromRow)]
    struct TopTrackerRow {
        sender_email: String,
        tracker_count: i64,
        tracker_types: Option<String>,
    }

    let top_trackers: Vec<TopTrackerRow> = sqlx::query_as(
        "SELECT sender_email, COUNT(*) as tracker_count, GROUP_CONCAT(DISTINCT tracker_type) as tracker_types
         FROM tracking_events WHERE account_id = ? GROUP BY sender_email
         ORDER BY tracker_count DESC LIMIT 5"
    ).bind(&account.id).fetch_all(pool.inner()).await.map_err(|e| e.to_string())?;

    Ok(PrivacyReport {
        total_blocked: total_blocked.0,
        unique_senders_tracked: unique_senders.0,
        blocked_this_week: blocked_this_week.0,
        top_trackers: top_trackers.into_iter().map(|t| TrackerSender {
            sender_email: t.sender_email,
            tracker_count: t.tracker_count,
            tracker_types: t.tracker_types.unwrap_or_default(),
        }).collect(),
    })
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
