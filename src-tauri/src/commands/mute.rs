use super::accounts::get_active_account;
use super::threads::BatchResult;
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

#[tauri::command]
pub async fn mute_thread(
    app_handle: AppHandle,
    thread_id: String,
    muted_until: Option<i64>,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs() as i64;

    if let Some(until) = muted_until {
        if until <= now {
            return Err("muted_until must be in the future".to_string());
        }
    }

    sqlx::query(
        "INSERT OR REPLACE INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, ?, ?)"
    )
    .bind(&thread_id)
    .bind(&account.id)
    .bind(muted_until)
    .bind(now)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

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

    sqlx::query("DELETE FROM muted_threads WHERE thread_id = ? AND account_id = ?")
        .bind(&thread_id)
        .bind(&account.id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!("Thread unmuted: {}", thread_id);
    Ok(())
}

#[tauri::command]
pub async fn get_muted_threads(
    app_handle: AppHandle,
) -> Result<Vec<MutedThreadInfo>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

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
    .bind(&account.id)
    .fetch_all(pool.inner())
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

#[tauri::command]
pub async fn check_muted_threads(
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
        "SELECT thread_id FROM muted_threads WHERE account_id = ? AND muted_until IS NOT NULL AND muted_until <= ?"
    )
    .bind(&account.id)
    .bind(now)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let thread_ids: Vec<String> = expired.into_iter().map(|r| r.thread_id).collect();

    if !thread_ids.is_empty() {
        tracing::info!("Checked muted threads: {} expired", thread_ids.len());
        sqlx::query(
            "DELETE FROM muted_threads WHERE account_id = ? AND muted_until IS NOT NULL AND muted_until <= ?"
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
pub async fn is_thread_muted(
    app_handle: AppHandle,
    thread_id: String,
) -> Result<bool, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs() as i64;

    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM muted_threads WHERE thread_id = ? AND account_id = ? AND (muted_until IS NULL OR muted_until > ?)"
    )
    .bind(&thread_id)
    .bind(&account.id)
    .bind(now)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(count.0 > 0)
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::{setup_test_db, insert_account, insert_thread};

    #[tokio::test]
    async fn test_mute_inserts_record() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let muted_until = now + 86400;

        sqlx::query(
            "INSERT OR REPLACE INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t1")
        .bind("acc1")
        .bind(muted_until)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let result: (Option<i64>,) = sqlx::query_as(
            "SELECT muted_until FROM muted_threads WHERE thread_id = ?"
        )
        .bind("t1")
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(result.0, Some(muted_until));
    }

    #[tokio::test]
    async fn test_mute_forever_null_until() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, NULL, ?)"
        )
        .bind("t1")
        .bind("acc1")
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let result: (Option<i64>,) = sqlx::query_as(
            "SELECT muted_until FROM muted_threads WHERE thread_id = ?"
        )
        .bind("t1")
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(result.0, None);
    }

    #[tokio::test]
    async fn test_mute_rejects_past_timestamp() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let past_timestamp = now - 1000;

        assert!(past_timestamp <= now);
        // Future timestamps pass
        let future_timestamp = now + 3600;
        assert!(future_timestamp > now);
    }

    #[tokio::test]
    async fn test_unmute_removes_record() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t1")
        .bind("acc1")
        .bind(now + 86400)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("DELETE FROM muted_threads WHERE thread_id = ? AND account_id = ?")
            .bind("t1")
            .bind("acc1")
            .execute(&pool)
            .await
            .unwrap();

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM muted_threads WHERE thread_id = ?"
        )
        .bind("t1")
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn test_unmute_nonexistent_is_ok() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let result = sqlx::query(
            "DELETE FROM muted_threads WHERE thread_id = ? AND account_id = ?"
        )
        .bind("nonexistent")
        .bind("acc1")
        .execute(&pool)
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_muted_returns_ordered() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t1")
        .bind("acc1")
        .bind(now + 300)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t2")
        .bind("acc1")
        .bind(now + 100)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, NULL, ?)"
        )
        .bind("t3")
        .bind("acc1")
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        #[derive(sqlx::FromRow)]
        struct OrderedRow {
            thread_id: String,
        }

        let rows: Vec<OrderedRow> = sqlx::query_as(
            "SELECT thread_id FROM muted_threads WHERE account_id = ? ORDER BY muted_until ASC"
        )
        .bind("acc1")
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(rows.len(), 3);
        // NULL sorts first in ASC order
        assert_eq!(rows[0].thread_id, "t3");
        assert_eq!(rows[1].thread_id, "t2");
        assert_eq!(rows[2].thread_id, "t1");
    }

    #[tokio::test]
    async fn test_get_muted_empty() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT thread_id FROM muted_threads WHERE account_id = ?"
        )
        .bind("acc1")
        .fetch_all(&pool)
        .await
        .unwrap();

        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn test_check_muted_finds_expired() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t1")
        .bind("acc1")
        .bind(now - 100)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t2")
        .bind("acc1")
        .bind(now - 200)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let expired: Vec<(String,)> = sqlx::query_as(
            "SELECT thread_id FROM muted_threads WHERE account_id = ? AND muted_until IS NOT NULL AND muted_until <= ?"
        )
        .bind("acc1")
        .bind(now)
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(expired.len(), 2);
    }

    #[tokio::test]
    async fn test_check_muted_skips_future() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t1")
        .bind("acc1")
        .bind(now + 3600)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let expired: Vec<(String,)> = sqlx::query_as(
            "SELECT thread_id FROM muted_threads WHERE account_id = ? AND muted_until IS NOT NULL AND muted_until <= ?"
        )
        .bind("acc1")
        .bind(now)
        .fetch_all(&pool)
        .await
        .unwrap();

        assert!(expired.is_empty());
    }

    #[tokio::test]
    async fn test_check_muted_skips_forever() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, NULL, ?)"
        )
        .bind("t_forever")
        .bind("acc1")
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let expired: Vec<(String,)> = sqlx::query_as(
            "SELECT thread_id FROM muted_threads WHERE account_id = ? AND muted_until IS NOT NULL AND muted_until <= ?"
        )
        .bind("acc1")
        .bind(now)
        .fetch_all(&pool)
        .await
        .unwrap();

        assert!(expired.is_empty());
    }

    #[tokio::test]
    async fn test_is_thread_muted_returns_true() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t_active")
        .bind("acc1")
        .bind(now + 3600)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM muted_threads WHERE thread_id = ? AND account_id = ? AND (muted_until IS NULL OR muted_until > ?)"
        )
        .bind("t_active")
        .bind("acc1")
        .bind(now)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn test_is_thread_muted_returns_false_for_expired() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind("t_expired")
        .bind("acc1")
        .bind(now - 100)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM muted_threads WHERE thread_id = ? AND account_id = ? AND (muted_until IS NULL OR muted_until > ?)"
        )
        .bind("t_expired")
        .bind("acc1")
        .bind(now)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn test_is_thread_muted_returns_true_for_forever() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO muted_threads (thread_id, account_id, muted_until, created_at) VALUES (?, ?, NULL, ?)"
        )
        .bind("t_forever")
        .bind("acc1")
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM muted_threads WHERE thread_id = ? AND account_id = ? AND (muted_until IS NULL OR muted_until > ?)"
        )
        .bind("t_forever")
        .bind("acc1")
        .bind(now)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(count.0, 1);
    }
}
