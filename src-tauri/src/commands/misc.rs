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
pub struct TrackingScanResult {
    pub trackers_found: usize,
    pub trackers_blocked: usize,
    pub cleaned_html: String,
    pub tracker_details: Vec<crate::tracking_detector::DetectedTracker>,
}

async fn persist_tracking_events(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    message_id: Option<&str>,
    sender_email: &str,
    trackers: &[crate::tracking_detector::DetectedTracker],
) -> Result<(), sqlx::Error> {
    if trackers.is_empty() {
        return Ok(());
    }
    let detected_at = chrono::Utc::now().timestamp();
    let mut tx = pool.begin().await?;
    for t in trackers {
        sqlx::query(
            "INSERT INTO tracking_events (account_id, message_id, sender_email, tracker_type, details, url_snippet, blocked, detected_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(account_id)
        .bind(message_id)
        .bind(sender_email)
        .bind(t.tracker_type.as_str())
        .bind(&t.details)
        .bind(&t.url_snippet)
        .bind(t.blocked as i64)
        .bind(detected_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

#[tauri::command]
pub async fn scan_tracking_content(
    app_handle: tauri::AppHandle,
    html: String,
    sender: Option<String>,
    message_id: Option<String>,
) -> Result<TrackingScanResult, String> {
    let (trackers, cleaned_html, blocked) =
        crate::tracking_detector::scan_and_block(&html);
    let found = trackers.len();

    if found > 0 {
        let pool = app_handle.state::<sqlx::SqlitePool>();
        if let Ok(account) = get_active_account(pool.inner()).await {
            if let Err(e) = persist_tracking_events(
                pool.inner(),
                &account.id,
                message_id.as_deref(),
                sender.as_deref().unwrap_or("unknown"),
                &trackers,
            )
            .await
            {
                tracing::warn!("Failed to persist tracking events: {}", e);
            }
        }
    }

    Ok(TrackingScanResult {
        trackers_found: found,
        trackers_blocked: blocked,
        cleaned_html,
        tracker_details: trackers,
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

    async fn test_pool() -> sqlx::SqlitePool {
        use std::str::FromStr;
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        crate::db::apply_schema(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn test_persist_tracking_events_inserts_rows() {
        let pool = test_pool().await;
        let (trackers, _cleaned, _blocked) = crate::tracking_detector::scan_and_block(
            r#"<img src="https://track.com/pixel.gif" width="1" height="1">"#,
        );
        assert!(!trackers.is_empty());

        persist_tracking_events(&pool, "acct-1", Some("msg-1"), "spy@example.com", &trackers)
            .await
            .unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tracking_events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, trackers.len() as i64, "one row per detected tracker");

        let row: (String, String, String, i64) = sqlx::query_as(
            "SELECT account_id, sender_email, tracker_type, blocked FROM tracking_events LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, "acct-1");
        assert_eq!(row.1, "spy@example.com");
        assert_eq!(row.2, "tracking_pixel");
        assert_eq!(row.3, 1, "blocked 1x1 pixel persisted as blocked=1");
    }

    #[tokio::test]
    async fn test_persist_tracking_events_empty_noop() {
        let pool = test_pool().await;
        persist_tracking_events(&pool, "acct-1", None, "x@y.com", &[])
            .await
            .unwrap();
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tracking_events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0);
    }
}
