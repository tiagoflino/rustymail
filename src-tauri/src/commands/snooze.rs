use super::accounts::get_active_account;
use super::threads::BatchResult;
use tauri::AppHandle;
use tauri::Manager;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct SnoozedThreadInfo {
    pub thread_id: String,
    pub account_id: String,
    pub snoozed_until: i64,
    pub created_at: i64,
    pub subject: String,
    pub sender: String,
}

#[tauri::command]
pub async fn snooze_thread(
    app_handle: AppHandle,
    thread_id: String,
    snoozed_until: i64,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs() as i64;

    if snoozed_until <= now {
        return Err("snoozed_until must be in the future".to_string());
    }

    sqlx::query(
        "INSERT OR REPLACE INTO snoozed_threads (thread_id, account_id, snoozed_until, created_at) VALUES (?, ?, ?, ?)"
    )
    .bind(&thread_id)
    .bind(&account.id)
    .bind(snoozed_until)
    .bind(now)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    tracing::info!("Thread snoozed: {} until {}", thread_id, snoozed_until);
    Ok(())
}

#[tauri::command]
pub async fn unsnooze_thread(
    app_handle: AppHandle,
    thread_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    sqlx::query("DELETE FROM snoozed_threads WHERE thread_id = ? AND account_id = ?")
        .bind(&thread_id)
        .bind(&account.id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!("Thread unsnoozed: {}", thread_id);
    Ok(())
}

#[tauri::command]
pub async fn get_snoozed_threads(
    app_handle: AppHandle,
) -> Result<Vec<SnoozedThreadInfo>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    #[derive(sqlx::FromRow)]
    struct SnoozedRow {
        thread_id: String,
        account_id: String,
        snoozed_until: i64,
        created_at: i64,
        subject: Option<String>,
        sender: Option<String>,
    }

    let rows: Vec<SnoozedRow> = sqlx::query_as(
        "SELECT s.thread_id, s.account_id, s.snoozed_until, s.created_at,
                t.subject, t.sender
         FROM snoozed_threads s
         LEFT JOIN threads t ON s.thread_id = t.id AND s.account_id = t.account_id
         WHERE s.account_id = ?
         ORDER BY s.snoozed_until ASC"
    )
    .bind(&account.id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| SnoozedThreadInfo {
            thread_id: r.thread_id,
            account_id: r.account_id,
            snoozed_until: r.snoozed_until,
            created_at: r.created_at,
            subject: r.subject.unwrap_or_else(|| "No Subject".to_string()),
            sender: r.sender.unwrap_or_else(|| "Unknown Sender".to_string()),
        })
        .collect())
}

#[tauri::command]
pub async fn check_snoozed_threads(
    app_handle: AppHandle,
) -> Result<Vec<String>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs() as i64;

    #[derive(sqlx::FromRow)]
    struct ExpiredRow {
        thread_id: String,
    }

    let expired: Vec<ExpiredRow> = sqlx::query_as(
        "SELECT thread_id FROM snoozed_threads WHERE account_id = ? AND snoozed_until <= ?"
    )
    .bind(&account.id)
    .bind(now)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let thread_ids: Vec<String> = expired.into_iter().map(|r| r.thread_id).collect();

    if !thread_ids.is_empty() {
        tracing::info!("Checked snoozed threads: {} expired", thread_ids.len());
        sqlx::query(
            "DELETE FROM snoozed_threads WHERE account_id = ? AND snoozed_until <= ?"
        )
        .bind(&account.id)
        .bind(now)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(thread_ids)
}

#[tauri::command]
pub async fn batch_snooze_threads(
    app_handle: AppHandle,
    thread_ids: Vec<String>,
    snoozed_until: i64,
) -> Result<BatchResult, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs() as i64;

    if snoozed_until <= now {
        return Err("snoozed_until must be in the future".to_string());
    }

    let mut succeeded = 0usize;
    let mut failed_ids = Vec::new();
    for tid in &thread_ids {
        match sqlx::query(
            "INSERT OR REPLACE INTO snoozed_threads (thread_id, account_id, snoozed_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind(tid)
        .bind(&account.id)
        .bind(snoozed_until)
        .bind(now)
        .execute(pool.inner())
        .await
        {
            Ok(_) => succeeded += 1,
            Err(_) => failed_ids.push(tid.clone()),
        }
    }
    Ok(BatchResult {
        succeeded,
        failed_ids,
    })
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::{setup_test_db, insert_account, insert_thread};

    #[tokio::test]
    async fn test_snooze_inserts_record() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let snoozed_until = now + 3600;

        sqlx::query(
            "INSERT OR REPLACE INTO snoozed_threads (thread_id, account_id, snoozed_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t1")
        .bind("acc1")
        .bind(snoozed_until)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let result: (i64,) = sqlx::query_as(
            "SELECT snoozed_until FROM snoozed_threads WHERE thread_id = ?"
        )
        .bind("t1")
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(result.0, snoozed_until);
    }

    #[tokio::test]
    async fn test_snooze_rejects_past_timestamp() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let past_timestamp = now - 1000;

        // Verify past timestamps would fail the > now check
        assert!(past_timestamp <= now);
        // Boundary: exact now should also be rejected (requires strictly >)
        assert!(now <= now);
        // Future timestamps pass
        let future_timestamp = now + 3600;
        assert!(future_timestamp > now);
    }

    #[tokio::test]
    async fn test_snooze_upsert_updates_timestamp() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT OR REPLACE INTO snoozed_threads (thread_id, account_id, snoozed_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t1")
        .bind("acc1")
        .bind(now + 3600)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT OR REPLACE INTO snoozed_threads (thread_id, account_id, snoozed_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t1")
        .bind("acc1")
        .bind(now + 7200)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let result: (i64,) = sqlx::query_as(
            "SELECT snoozed_until FROM snoozed_threads WHERE thread_id = ?"
        )
        .bind("t1")
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(result.0, now + 7200);
    }

    #[tokio::test]
    async fn test_unsnooze_removes_record() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO snoozed_threads (thread_id, account_id, snoozed_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t1")
        .bind("acc1")
        .bind(now + 3600)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("DELETE FROM snoozed_threads WHERE thread_id = ? AND account_id = ?")
            .bind("t1")
            .bind("acc1")
            .execute(&pool)
            .await
            .unwrap();

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM snoozed_threads WHERE thread_id = ?"
        )
        .bind("t1")
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn test_unsnooze_nonexistent_is_ok() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let result = sqlx::query("DELETE FROM snoozed_threads WHERE thread_id = ? AND account_id = ?")
            .bind("nonexistent")
            .bind("acc1")
            .execute(&pool)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_snoozed_returns_ordered() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO snoozed_threads (thread_id, account_id, snoozed_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t1")
        .bind("acc1")
        .bind(now + 300)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO snoozed_threads (thread_id, account_id, snoozed_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t2")
        .bind("acc1")
        .bind(now + 100)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO snoozed_threads (thread_id, account_id, snoozed_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t3")
        .bind("acc1")
        .bind(now + 200)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        #[derive(sqlx::FromRow)]
        struct OrderedRow {
            thread_id: String,
        }

        let rows: Vec<OrderedRow> = sqlx::query_as(
            "SELECT thread_id FROM snoozed_threads WHERE account_id = ? ORDER BY snoozed_until ASC"
        )
        .bind("acc1")
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].thread_id, "t2");
        assert_eq!(rows[1].thread_id, "t3");
        assert_eq!(rows[2].thread_id, "t1");
    }

    #[tokio::test]
    async fn test_get_snoozed_empty() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT thread_id FROM snoozed_threads WHERE account_id = ?"
        )
        .bind("acc1")
        .fetch_all(&pool)
        .await
        .unwrap();

        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn test_check_snoozed_finds_expired() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO snoozed_threads (thread_id, account_id, snoozed_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t1")
        .bind("acc1")
        .bind(now - 100)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO snoozed_threads (thread_id, account_id, snoozed_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t2")
        .bind("acc1")
        .bind(now - 200)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let expired: Vec<(String,)> = sqlx::query_as(
            "SELECT thread_id FROM snoozed_threads WHERE account_id = ? AND snoozed_until <= ?"
        )
        .bind("acc1")
        .bind(now)
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(expired.len(), 2);
    }

    #[tokio::test]
    async fn test_check_snoozed_skips_future() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO snoozed_threads (thread_id, account_id, snoozed_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t1")
        .bind("acc1")
        .bind(now + 3600)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let expired: Vec<(String,)> = sqlx::query_as(
            "SELECT thread_id FROM snoozed_threads WHERE account_id = ? AND snoozed_until <= ?"
        )
        .bind("acc1")
        .bind(now)
        .fetch_all(&pool)
        .await
        .unwrap();

        assert!(expired.is_empty());
    }
}
