use super::accounts::get_active_account;
use super::threads::BatchResult;
use sqlx::SqlitePool;
use tauri::AppHandle;
use tauri::Manager;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct MutedThreadInfo {
    pub thread_id: String,
    pub account_id: String,
    pub muted_until: Option<i64>,
    pub created_at: i64,
    pub subject: String,
    pub sender: String,
}

fn now_secs() -> Result<i64, String> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs() as i64)
}

async fn mute_thread_impl(
    pool: &SqlitePool,
    account_id: &str,
    thread_id: &str,
    muted_until: Option<i64>,
    now: i64,
) -> Result<(), String> {
    if let Some(until) = muted_until {
        if until <= now {
            return Err("muted_until must be in the future".to_string());
        }
    }

    sqlx::query(
        "INSERT OR REPLACE INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, ?, ?)"
    )
    .bind(thread_id)
    .bind(account_id)
    .bind(muted_until)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

async fn unmute_thread_impl(
    pool: &SqlitePool,
    account_id: &str,
    thread_id: &str,
) -> Result<(), String> {
    sqlx::query("DELETE FROM muted_threads WHERE thread_id = ? AND account_id = ?")
        .bind(thread_id)
        .bind(account_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

async fn get_muted_threads_impl(
    pool: &SqlitePool,
    account_id: &str,
) -> Result<Vec<MutedThreadInfo>, String> {
    #[derive(sqlx::FromRow)]
    struct MutedRow {
        thread_id: String,
        account_id: String,
        muted_until: Option<i64>,
        created_at: i64,
        subject: Option<String>,
        sender: Option<String>,
    }

    let rows: Vec<MutedRow> = sqlx::query_as(
        "SELECT m.thread_id, m.account_id, m.muted_until, m.created_at,
                t.subject, t.sender
         FROM muted_threads m
         LEFT JOIN threads t ON m.thread_id = t.id AND m.account_id = t.account_id
         WHERE m.account_id = ?
         ORDER BY m.muted_until ASC"
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| MutedThreadInfo {
            thread_id: r.thread_id,
            account_id: r.account_id,
            muted_until: r.muted_until,
            created_at: r.created_at,
            subject: r.subject.unwrap_or_else(|| "No Subject".to_string()),
            sender: r.sender.unwrap_or_else(|| "Unknown Sender".to_string()),
        })
        .collect())
}

async fn check_muted_threads_impl(
    pool: &SqlitePool,
    account_id: &str,
    now: i64,
) -> Result<Vec<String>, String> {
    #[derive(sqlx::FromRow)]
    struct ExpiredRow {
        thread_id: String,
    }

    let expired: Vec<ExpiredRow> = sqlx::query_as(
        "SELECT thread_id FROM muted_threads WHERE account_id = ? AND muted_until IS NOT NULL AND muted_until <= ?"
    )
    .bind(account_id)
    .bind(now)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let thread_ids: Vec<String> = expired.into_iter().map(|r| r.thread_id).collect();

    if !thread_ids.is_empty() {
        tracing::info!("Checked muted threads: {} expired", thread_ids.len());
        sqlx::query(
            "DELETE FROM muted_threads WHERE account_id = ? AND muted_until IS NOT NULL AND muted_until <= ?"
        )
        .bind(account_id)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(thread_ids)
}

async fn is_thread_muted_impl(
    pool: &SqlitePool,
    account_id: &str,
    thread_id: &str,
    now: i64,
) -> Result<bool, String> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM muted_threads WHERE thread_id = ? AND account_id = ? AND (muted_until IS NULL OR muted_until > ?)"
    )
    .bind(thread_id)
    .bind(account_id)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(count.0 > 0)
}

#[tauri::command]
pub async fn mute_thread(
    app_handle: AppHandle,
    thread_id: String,
    muted_until: Option<i64>,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let now = now_secs()?;

    mute_thread_impl(pool.inner(), &account.id, &thread_id, muted_until, now).await?;

    tracing::info!("Thread muted: {} until {:?}", thread_id, muted_until);
    Ok(())
}

#[tauri::command]
pub async fn unmute_thread(
    app_handle: AppHandle,
    thread_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    unmute_thread_impl(pool.inner(), &account.id, &thread_id).await?;

    tracing::info!("Thread unmuted: {}", thread_id);
    Ok(())
}

#[tauri::command]
pub async fn batch_mute_threads(
    app_handle: AppHandle,
    thread_ids: Vec<String>,
    muted_until: Option<i64>,
) -> Result<BatchResult, String> {
    tracing::info!("Batch mute: {} threads", thread_ids.len());
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let now = now_secs()?;

    if let Some(until) = muted_until {
        if until <= now {
            return Err("muted_until must be in the future".to_string());
        }
    }

    let mut succeeded = 0usize;
    let mut failed_ids = Vec::new();
    for tid in &thread_ids {
        match mute_thread_impl(pool.inner(), &account.id, tid, muted_until, now).await {
            Ok(()) => succeeded += 1,
            Err(_) => failed_ids.push(tid.clone()),
        }
    }
    if !failed_ids.is_empty() {
        tracing::error!("Batch mute: {} succeeded, {} failed", succeeded, failed_ids.len());
    }
    Ok(BatchResult {
        succeeded,
        failed_ids,
    })
}

#[tauri::command]
pub async fn get_muted_threads(
    app_handle: AppHandle,
) -> Result<Vec<MutedThreadInfo>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    get_muted_threads_impl(pool.inner(), &account.id).await
}

#[tauri::command]
pub async fn check_muted_threads(
    app_handle: AppHandle,
) -> Result<Vec<String>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let now = now_secs()?;

    check_muted_threads_impl(pool.inner(), &account.id, now).await
}

#[tauri::command]
pub async fn is_thread_muted(
    app_handle: AppHandle,
    thread_id: String,
) -> Result<bool, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let now = now_secs()?;

    is_thread_muted_impl(pool.inner(), &account.id, &thread_id, now).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_helpers::{setup_test_db, insert_account, insert_thread};

    fn now() -> i64 {
        super::now_secs().unwrap()
    }

    async fn stored_until(pool: &SqlitePool, thread_id: &str) -> Option<i64> {
        let row: (Option<i64>,) = sqlx::query_as(
            "SELECT muted_until FROM muted_threads WHERE thread_id = ?"
        )
        .bind(thread_id)
        .fetch_one(pool)
        .await
        .unwrap();
        row.0
    }

    async fn count_for(pool: &SqlitePool, thread_id: &str) -> i64 {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM muted_threads WHERE thread_id = ?"
        )
        .bind(thread_id)
        .fetch_one(pool)
        .await
        .unwrap();
        row.0
    }

    #[tokio::test]
    async fn test_mute_inserts_record() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        let now = now();
        let muted_until = now + 86400;

        mute_thread_impl(&pool, "acc1", "t1", Some(muted_until), now)
            .await
            .unwrap();

        assert_eq!(stored_until(&pool, "t1").await, Some(muted_until));
    }

    #[tokio::test]
    async fn test_mute_forever_null_until() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        let now = now();

        mute_thread_impl(&pool, "acc1", "t1", None, now).await.unwrap();

        assert_eq!(stored_until(&pool, "t1").await, None);
    }

    #[tokio::test]
    async fn test_mute_rejects_past_timestamp() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        let now = now();

        let result = mute_thread_impl(&pool, "acc1", "t1", Some(now - 1000), now).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "muted_until must be in the future");
        assert_eq!(count_for(&pool, "t1").await, 0, "rejected mute must not be persisted");
    }

    #[tokio::test]
    async fn test_mute_rejects_equal_timestamp() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        let now = now();

        let result = mute_thread_impl(&pool, "acc1", "t1", Some(now), now).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mute_is_idempotent_upsert() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        let now = now();

        mute_thread_impl(&pool, "acc1", "t1", Some(now + 100), now).await.unwrap();
        mute_thread_impl(&pool, "acc1", "t1", Some(now + 200), now).await.unwrap();

        assert_eq!(count_for(&pool, "t1").await, 1, "re-mute must replace, not duplicate");
        assert_eq!(stored_until(&pool, "t1").await, Some(now + 200));
    }

    #[tokio::test]
    async fn test_mute_scopes_to_account() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "a@test.com", "A", 1, 1000).await;
        insert_account(&pool, "acc2", "b@test.com", "B", 0, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        let now = now();

        mute_thread_impl(&pool, "acc1", "t1", Some(now + 100), now).await.unwrap();

        assert_eq!(get_muted_threads_impl(&pool, "acc1").await.unwrap().len(), 1);
        assert_eq!(get_muted_threads_impl(&pool, "acc2").await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_unmute_removes_record() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        let now = now();

        mute_thread_impl(&pool, "acc1", "t1", Some(now + 86400), now).await.unwrap();
        unmute_thread_impl(&pool, "acc1", "t1").await.unwrap();

        assert_eq!(count_for(&pool, "t1").await, 0);
    }

    #[tokio::test]
    async fn test_unmute_nonexistent_is_ok() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let result = unmute_thread_impl(&pool, "acc1", "nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_batch_mute_writes_all_with_until() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_thread(&pool, "t3", "acc1").await;
        let now = now();
        let until = now + 86400;

        let mut succeeded = 0usize;
        for tid in ["t1", "t2", "t3"] {
            mute_thread_impl(&pool, "acc1", tid, Some(until), now).await.unwrap();
            succeeded += 1;
        }

        assert_eq!(succeeded, 3);
        for tid in ["t1", "t2", "t3"] {
            assert_eq!(stored_until(&pool, tid).await, Some(until), "{tid} muted_until");
        }
        let muted = get_muted_threads_impl(&pool, "acc1").await.unwrap();
        assert_eq!(muted.len(), 3);
    }

    #[tokio::test]
    async fn test_get_muted_returns_ordered() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        let now = now();

        mute_thread_impl(&pool, "acc1", "t1", Some(now + 300), now).await.unwrap();
        mute_thread_impl(&pool, "acc1", "t2", Some(now + 100), now).await.unwrap();
        mute_thread_impl(&pool, "acc1", "t3", None, now).await.unwrap();

        let rows = get_muted_threads_impl(&pool, "acc1").await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].thread_id, "t3");
        assert_eq!(rows[1].thread_id, "t2");
        assert_eq!(rows[2].thread_id, "t1");
    }

    #[tokio::test]
    async fn test_get_muted_empty() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let rows = get_muted_threads_impl(&pool, "acc1").await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn test_check_muted_finds_and_deletes_expired() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        let now = now();

        mute_thread_impl(&pool, "acc1", "t1", Some(now + 100), now - 200).await.unwrap();
        mute_thread_impl(&pool, "acc1", "t2", Some(now + 200), now - 300).await.unwrap();

        let expired = check_muted_threads_impl(&pool, "acc1", now + 1000).await.unwrap();
        assert_eq!(expired.len(), 2);
        assert_eq!(get_muted_threads_impl(&pool, "acc1").await.unwrap().len(), 0, "expired rows deleted");
    }

    #[tokio::test]
    async fn test_check_muted_skips_future() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        let now = now();

        mute_thread_impl(&pool, "acc1", "t1", Some(now + 3600), now).await.unwrap();

        let expired = check_muted_threads_impl(&pool, "acc1", now).await.unwrap();
        assert!(expired.is_empty());
        assert_eq!(get_muted_threads_impl(&pool, "acc1").await.unwrap().len(), 1, "future mute kept");
    }

    #[tokio::test]
    async fn test_check_muted_skips_forever() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        let now = now();

        mute_thread_impl(&pool, "acc1", "t_forever", None, now).await.unwrap();

        let expired = check_muted_threads_impl(&pool, "acc1", now + 999999).await.unwrap();
        assert!(expired.is_empty());
        assert_eq!(get_muted_threads_impl(&pool, "acc1").await.unwrap().len(), 1, "forever mute never expires");
    }

    #[tokio::test]
    async fn test_is_thread_muted_returns_true() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        let now = now();

        mute_thread_impl(&pool, "acc1", "t_active", Some(now + 3600), now).await.unwrap();

        assert!(is_thread_muted_impl(&pool, "acc1", "t_active", now).await.unwrap());
    }

    #[tokio::test]
    async fn test_is_thread_muted_returns_false_for_expired() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        let now = now();

        mute_thread_impl(&pool, "acc1", "t_expired", Some(now + 100), now - 200).await.unwrap();

        assert!(!is_thread_muted_impl(&pool, "acc1", "t_expired", now + 1000).await.unwrap());
    }

    #[tokio::test]
    async fn test_is_thread_muted_returns_true_for_forever() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        let now = now();

        mute_thread_impl(&pool, "acc1", "t_forever", None, now).await.unwrap();

        assert!(is_thread_muted_impl(&pool, "acc1", "t_forever", now + 999999).await.unwrap());
    }
}
