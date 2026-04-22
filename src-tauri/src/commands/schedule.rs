use super::accounts::{get_active_account, get_account_by_id};
use tauri::Manager;

#[derive(serde::Serialize, serde::Deserialize, Debug, sqlx::FromRow)]
pub struct ScheduledSendInfo {
    pub id: i64,
    pub account_id: String,
    pub draft_id: String,
    pub thread_id: Option<String>,
    pub to_recipients: String,
    pub subject: String,
    pub send_at: i64,
    pub created_at: i64,
}

#[tauri::command]
pub async fn schedule_send(
    app_handle: tauri::AppHandle,
    draft_id: String,
    thread_id: Option<String>,
    to_recipients: String,
    subject: String,
    send_at: i64,
) -> Result<i64, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let now = chrono::Utc::now().timestamp();

    if send_at <= now {
        return Err("Scheduled time must be in the future".into());
    }

    let result = sqlx::query(
        "INSERT INTO scheduled_sends (account_id, draft_id, thread_id, to_recipients, subject, send_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&account.id)
    .bind(&draft_id)
    .bind(&thread_id)
    .bind(&to_recipients)
    .bind(&subject)
    .bind(send_at)
    .bind(now)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    tracing::info!("Scheduled send: '{}' to {} at {}", subject, to_recipients, send_at);
    Ok(result.last_insert_rowid())
}

#[tauri::command]
pub async fn cancel_scheduled_send(
    app_handle: tauri::AppHandle,
    id: i64,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    sqlx::query("DELETE FROM scheduled_sends WHERE id = ?")
        .bind(id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!("Cancelled scheduled send: id={}", id);
    Ok(())
}

#[tauri::command]
pub async fn get_scheduled_sends(
    app_handle: tauri::AppHandle,
) -> Result<Vec<ScheduledSendInfo>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    let rows: Vec<ScheduledSendInfo> = sqlx::query_as(
        "SELECT id, account_id, draft_id, thread_id, to_recipients, subject, send_at, created_at FROM scheduled_sends WHERE account_id = ? ORDER BY send_at ASC"
    )
    .bind(&account.id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows)
}

#[tauri::command]
pub async fn check_scheduled_sends(
    app_handle: tauri::AppHandle,
) -> Result<Vec<String>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let now = chrono::Utc::now().timestamp();

    // Get distinct accounts with overdue sends
    let account_ids: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT account_id FROM scheduled_sends WHERE send_at <= ?"
    ).bind(now).fetch_all(pool.inner()).await.map_err(|e| e.to_string())?;

    let mut sent_subjects = Vec::new();

    for (account_id,) in &account_ids {
        let account = match get_account_by_id(pool.inner(), account_id).await {
            Ok(a) => a,
            Err(e) => {
                tracing::error!("Failed to get account {} for scheduled send: {}", account_id, e);
                continue;
            }
        };

        let sends: Vec<(i64, String, String)> = sqlx::query_as(
            "SELECT id, draft_id, subject FROM scheduled_sends WHERE account_id = ? AND send_at <= ?"
        ).bind(account_id).bind(now).fetch_all(pool.inner()).await.map_err(|e| e.to_string())?;

        for (id, draft_id, subject) in &sends {
            match crate::gmail_api::send_draft(&account.access_token, draft_id).await {
                Ok(()) => {
                    tracing::info!("Scheduled send fired: '{}' (draft {})", subject, draft_id);
                    sent_subjects.push(subject.clone());
                }
                Err(e) => {
                    tracing::error!("Failed to send scheduled draft {}: {}", draft_id, e);
                }
            }
            // Remove from table regardless (draft gone or sent)
            let _ = sqlx::query("DELETE FROM scheduled_sends WHERE id = ?")
                .bind(id).execute(pool.inner()).await;
        }
    }

    Ok(sent_subjects)
}

#[tauri::command]
pub async fn get_scheduled_count(
    app_handle: tauri::AppHandle,
) -> Result<u32, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scheduled_sends")
        .fetch_one(pool.inner()).await.map_err(|e| e.to_string())?;
    Ok(count.0 as u32)
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::setup_test_db;

    #[tokio::test]
    async fn test_schedule_send_inserts_row() {
        let pool = setup_test_db().await;
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO scheduled_sends (account_id, draft_id, to_recipients, subject, send_at, created_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("acc1").bind("d1").bind("a@b.com").bind("Test").bind(now + 3600).bind(now)
        .execute(&pool).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scheduled_sends WHERE account_id = 'acc1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn test_cancel_scheduled_send() {
        let pool = setup_test_db().await;
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query(
            "INSERT INTO scheduled_sends (account_id, draft_id, to_recipients, subject, send_at, created_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("acc1").bind("d1").bind("a@b.com").bind("Test").bind(now + 3600).bind(now)
        .execute(&pool).await.unwrap();

        let id = result.last_insert_rowid();
        sqlx::query("DELETE FROM scheduled_sends WHERE id = ?").bind(id).execute(&pool).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scheduled_sends")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn test_get_scheduled_count() {
        let pool = setup_test_db().await;
        let now = chrono::Utc::now().timestamp();

        for i in 0..3 {
            sqlx::query(
                "INSERT INTO scheduled_sends (account_id, draft_id, to_recipients, subject, send_at, created_at) VALUES (?, ?, ?, ?, ?, ?)"
            ).bind("acc1").bind(format!("d{}", i)).bind("a@b.com").bind(format!("Test {}", i)).bind(now + 3600).bind(now)
            .execute(&pool).await.unwrap();
        }

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scheduled_sends")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 3);
    }

    #[tokio::test]
    async fn test_check_overdue_scheduled_sends() {
        let pool = setup_test_db().await;
        let now = chrono::Utc::now().timestamp();

        // Overdue
        sqlx::query(
            "INSERT INTO scheduled_sends (account_id, draft_id, to_recipients, subject, send_at, created_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("acc1").bind("d1").bind("a@b.com").bind("Overdue").bind(now - 60).bind(now)
        .execute(&pool).await.unwrap();

        // Future
        sqlx::query(
            "INSERT INTO scheduled_sends (account_id, draft_id, to_recipients, subject, send_at, created_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("acc1").bind("d2").bind("c@d.com").bind("Future").bind(now + 3600).bind(now)
        .execute(&pool).await.unwrap();

        let overdue: Vec<(String, String)> = sqlx::query_as(
            "SELECT account_id, draft_id FROM scheduled_sends WHERE send_at <= ?"
        ).bind(now).fetch_all(&pool).await.unwrap();

        assert_eq!(overdue.len(), 1);
        assert_eq!(overdue[0].1, "d1");
    }

    #[tokio::test]
    async fn test_multi_account_scheduled_sends() {
        let pool = setup_test_db().await;
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO scheduled_sends (account_id, draft_id, to_recipients, subject, send_at, created_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("acc1").bind("d1").bind("a@b.com").bind("From acc1").bind(now - 60).bind(now)
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO scheduled_sends (account_id, draft_id, to_recipients, subject, send_at, created_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("acc2").bind("d2").bind("c@d.com").bind("From acc2").bind(now - 30).bind(now)
        .execute(&pool).await.unwrap();

        let accounts: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT account_id FROM scheduled_sends WHERE send_at <= ?"
        ).bind(now).fetch_all(&pool).await.unwrap();

        assert_eq!(accounts.len(), 2);
    }
}
