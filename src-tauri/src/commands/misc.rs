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
    pub trend: Vec<TrendPoint>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TrackerSender {
    pub sender_email: String,
    pub tracker_count: i64,
    pub tracker_types: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TrendPoint {
    pub day: String,
    pub count: i64,
}

#[allow(dead_code)] // public writer API exercised in tests; sync path inlines into its tx
pub(crate) async fn record_tracking_events(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    message_id: Option<&str>,
    sender_email: &str,
    tracker_types: &[&str],
    blocked: bool,
) -> Result<(), String> {
    if tracker_types.is_empty() {
        return Ok(());
    }
    if let Some(mid) = message_id {
        let existing: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM tracking_events WHERE account_id = ? AND message_id = ?",
        )
        .bind(account_id)
        .bind(mid)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
        if existing.0 > 0 {
            return Ok(());
        }
    }
    let now = chrono::Utc::now().timestamp();
    let blocked_val = if blocked { 1 } else { 0 };
    for tracker_type in tracker_types {
        sqlx::query(
            "INSERT INTO tracking_events (account_id, message_id, sender_email, tracker_type, blocked, detected_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(account_id)
        .bind(message_id)
        .bind(sender_email)
        .bind(tracker_type)
        .bind(blocked_val)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_privacy_report(app_handle: tauri::AppHandle) -> Result<PrivacyReport, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    build_privacy_report(pool.inner(), &account.id).await
}

pub(crate) async fn build_privacy_report(
    pool: &sqlx::SqlitePool,
    account_id: &str,
) -> Result<PrivacyReport, String> {
    let now = chrono::Utc::now().timestamp();
    let week_ago = now - 7 * 86400;
    let month_ago = now - 30 * 86400;

    let total_blocked: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM tracking_events WHERE account_id = ? AND blocked = 1"
    ).bind(account_id).fetch_one(pool).await.map_err(|e| e.to_string())?;

    let unique_senders: (i64,) = sqlx::query_as(
        "SELECT COUNT(DISTINCT sender_email) FROM tracking_events WHERE account_id = ? AND blocked = 1"
    ).bind(account_id).fetch_one(pool).await.map_err(|e| e.to_string())?;

    let blocked_this_week: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM tracking_events WHERE account_id = ? AND blocked = 1 AND detected_at > ?"
    ).bind(account_id).bind(week_ago).fetch_one(pool).await.map_err(|e| e.to_string())?;

    #[derive(sqlx::FromRow)]
    struct TopTrackerRow {
        sender_email: String,
        tracker_count: i64,
        tracker_types: Option<String>,
    }

    let top_trackers: Vec<TopTrackerRow> = sqlx::query_as(
        "SELECT sender_email, COUNT(*) as tracker_count, GROUP_CONCAT(DISTINCT tracker_type) as tracker_types
         FROM tracking_events WHERE account_id = ? AND blocked = 1 GROUP BY sender_email
         ORDER BY tracker_count DESC LIMIT 5"
    ).bind(account_id).fetch_all(pool).await.map_err(|e| e.to_string())?;

    let trend: Vec<TrendPoint> = sqlx::query_as::<_, (String, i64)>(
        "SELECT strftime('%Y-%m-%d', detected_at, 'unixepoch') as day, COUNT(*) as count
         FROM tracking_events WHERE account_id = ? AND blocked = 1 AND detected_at > ?
         GROUP BY day ORDER BY day ASC"
    )
    .bind(account_id)
    .bind(month_ago)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|(day, count)| TrendPoint { day, count })
    .collect();

    Ok(PrivacyReport {
        total_blocked: total_blocked.0,
        unique_senders_tracked: unique_senders.0,
        blocked_this_week: blocked_this_week.0,
        top_trackers: top_trackers.into_iter().map(|t| TrackerSender {
            sender_email: t.sender_email,
            tracker_count: t.tracker_count,
            tracker_types: t.tracker_types.unwrap_or_default(),
        }).collect(),
        trend,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::SqlitePool;
    use std::str::FromStr;

    async fn report_test_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        crate::db::apply_schema(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn test_privacy_report_empty_is_all_zeros() {
        let pool = report_test_pool().await;
        let report = build_privacy_report(&pool, "acc1").await.unwrap();
        assert_eq!(report.total_blocked, 0);
        assert_eq!(report.unique_senders_tracked, 0);
        assert_eq!(report.blocked_this_week, 0);
        assert!(report.top_trackers.is_empty());
        assert!(report.trend.is_empty());
    }

    #[tokio::test]
    async fn test_record_then_report_is_nonzero() {
        let pool = report_test_pool().await;
        record_tracking_events(&pool, "acc1", Some("m1"), "spy@ads.com", &["tracking_pixel"], true)
            .await
            .unwrap();
        let report = build_privacy_report(&pool, "acc1").await.unwrap();
        assert_eq!(report.total_blocked, 1);
        assert_eq!(report.unique_senders_tracked, 1);
        assert_eq!(report.blocked_this_week, 1);
        assert_eq!(report.top_trackers.len(), 1);
        assert_eq!(report.top_trackers[0].sender_email, "spy@ads.com");
        assert_eq!(report.top_trackers[0].tracker_count, 1);
        assert_eq!(report.trend.iter().map(|t| t.count).sum::<i64>(), 1);
    }

    #[tokio::test]
    async fn test_record_double_count_guard_by_message_id() {
        let pool = report_test_pool().await;
        record_tracking_events(&pool, "acc1", Some("m1"), "spy@ads.com", &["tracking_pixel"], true)
            .await
            .unwrap();
        record_tracking_events(&pool, "acc1", Some("m1"), "spy@ads.com", &["tracking_pixel", "remote_image"], true)
            .await
            .unwrap();
        let report = build_privacy_report(&pool, "acc1").await.unwrap();
        assert_eq!(report.total_blocked, 1, "same message must not be counted twice");
    }

    #[tokio::test]
    async fn test_top_trackers_excludes_unblocked_events() {
        let pool = report_test_pool().await;
        record_tracking_events(&pool, "acc1", Some("m1"), "spy@ads.com", &["tracking_pixel"], true)
            .await
            .unwrap();
        record_tracking_events(&pool, "acc1", Some("m2"), "spy@ads.com", &["remote_image"], false)
            .await
            .unwrap();
        let report = build_privacy_report(&pool, "acc1").await.unwrap();
        assert_eq!(report.total_blocked, 1, "only blocked events count in headline");
        assert_eq!(report.top_trackers.len(), 1);
        assert_eq!(
            report.top_trackers[0].tracker_count, 1,
            "top_trackers count must match blocked semantics, not include unblocked rows"
        );
    }

    #[tokio::test]
    async fn test_record_empty_types_is_noop() {
        let pool = report_test_pool().await;
        record_tracking_events(&pool, "acc1", Some("m1"), "x@y.com", &[], true)
            .await
            .unwrap();
        let report = build_privacy_report(&pool, "acc1").await.unwrap();
        assert_eq!(report.total_blocked, 0);
    }

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
