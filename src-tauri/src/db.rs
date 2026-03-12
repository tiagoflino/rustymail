use anyhow::Result;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;
use tauri::Manager;

const CURRENT_SCHEMA_VERSION: &str = "2";

pub async fn apply_schema(pool: &SqlitePool) -> Result<()> {
    let schema = r#"
    CREATE TABLE IF NOT EXISTS accounts (
        id TEXT PRIMARY KEY,
        email TEXT,
        display_name TEXT,
        avatar_url TEXT,
        token_expiry INTEGER,
        is_active INTEGER DEFAULT 1,
        created_at INTEGER
    );

    CREATE TABLE IF NOT EXISTS labels (
        id TEXT PRIMARY KEY,
        account_id TEXT,
        name TEXT,
        type TEXT,
        unread_count INTEGER
    );

    CREATE TABLE IF NOT EXISTS threads (
        id TEXT PRIMARY KEY,
        account_id TEXT,
        snippet TEXT,
        history_id TEXT,
        synced_history_id TEXT,
        last_message_internal_date INTEGER,
        unread INTEGER
    );

    CREATE TABLE IF NOT EXISTS messages (
        id TEXT PRIMARY KEY,
        thread_id TEXT,
        account_id TEXT,
        sender TEXT,
        recipients TEXT,
        subject TEXT,
        snippet TEXT,
        internal_date INTEGER,
        body_plain TEXT,
        body_html TEXT,
        has_attachments INTEGER
    );

    CREATE TABLE IF NOT EXISTS attachments (
        id TEXT PRIMARY KEY,
        message_id TEXT,
        filename TEXT,
        mime_type TEXT,
        size INTEGER,
        local_path TEXT,
        downloaded INTEGER
    );

    CREATE TABLE IF NOT EXISTS drafts (
        id TEXT PRIMARY KEY,
        account_id TEXT,
        to_field TEXT,
        cc_field TEXT,
        subject TEXT,
        body_html TEXT,
        created_at INTEGER
    );

    CREATE TABLE IF NOT EXISTS history_state (
        account_id TEXT PRIMARY KEY,
        last_history_id TEXT
    );

    CREATE TABLE IF NOT EXISTS thread_labels (
        thread_id TEXT NOT NULL,
        label_id TEXT NOT NULL,
        PRIMARY KEY (thread_id, label_id)
    );

    CREATE TABLE IF NOT EXISTS message_labels (
        message_id TEXT NOT NULL,
        label_id TEXT NOT NULL,
        PRIMARY KEY (message_id, label_id)
    );

    CREATE TABLE IF NOT EXISTS settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_thread_labels_thread ON thread_labels(thread_id);
    CREATE INDEX IF NOT EXISTS idx_thread_labels_label ON thread_labels(label_id);
    CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_id);
    CREATE INDEX IF NOT EXISTS idx_messages_account ON messages(account_id);
    CREATE INDEX IF NOT EXISTS idx_messages_internal_date ON messages(internal_date);
    CREATE INDEX IF NOT EXISTS idx_message_labels_label ON message_labels(label_id);
    CREATE INDEX IF NOT EXISTS idx_threads_account ON threads(account_id);
    CREATE INDEX IF NOT EXISTS idx_labels_account ON labels(account_id);

    PRAGMA journal_mode=WAL;
    "#;

    sqlx::query(schema).execute(pool).await?;

    sqlx::query("INSERT OR IGNORE INTO settings (key, value) VALUES ('schema_version', ?)")
        .bind(CURRENT_SCHEMA_VERSION)
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(sender, subject, body_plain, content=messages, content_rowid=rowid)"
    ).execute(pool).await.ok();

    let defaults = [
        ("theme", "system"),
        ("density", "default"),
        ("default_mailbox", "INBOX"),
        ("mark_read_delay", "2"),
        ("reading_pane", "right"),
        ("signature", ""),
        ("reply_position", "above"),
        ("notifications_enabled", "true"),
        ("notifications_sound", "true"),
        ("sync_frequency", "30"),
        ("max_threads_sync", "100"),
        ("max_cache_mb", "500"),
        ("download_folder", ""),
        ("attachment_action", "open"),
    ];
    for (key, value) in defaults {
        let _ = sqlx::query("INSERT OR IGNORE INTO settings (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(pool)
            .await;
    }

    Ok(())
}

pub async fn init_db(app_handle: &tauri::AppHandle) -> Result<SqlitePool> {
    let app_dir = app_handle.path().app_data_dir().expect("Failed to get app data dir");
    std::fs::create_dir_all(&app_dir)?;

    let db_path = app_dir.join("rustymail.db");

    let options = SqliteConnectOptions::from_str(
        &format!("sqlite://{}", db_path.to_string_lossy())
    )?.create_if_missing(true);

    let pool = SqlitePool::connect_with(options).await?;
    apply_schema(&pool).await?;
    run_migrations(&pool).await?;

    Ok(pool)
}

async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    // Migration: add synced_history_id column to threads (v3)
    let has_col: bool = sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(*) FROM pragma_table_info('threads') WHERE name = 'synced_history_id'"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0) > 0;

    if !has_col {
        sqlx::query("ALTER TABLE threads ADD COLUMN synced_history_id TEXT")
            .execute(pool)
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        SqlitePool::connect_with(options).await.unwrap()
    }

    #[tokio::test]
    async fn test_apply_schema_creates_all_tables() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();

        let tables: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
        ).fetch_all(&pool).await.unwrap();

        let names: Vec<&str> = tables.iter().map(|r| r.0.as_str()).collect();
        for expected in &[
            "accounts", "attachments", "drafts", "history_state", "labels",
            "message_labels", "messages", "messages_fts", "settings",
            "thread_labels", "threads",
        ] {
            assert!(names.contains(expected), "Missing table: {expected}");
        }
    }

    #[tokio::test]
    async fn test_apply_schema_creates_indexes() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();

        let indexes: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%' ORDER BY name"
        ).fetch_all(&pool).await.unwrap();

        let names: Vec<&str> = indexes.iter().map(|r| r.0.as_str()).collect();
        for expected in &[
            "idx_labels_account", "idx_message_labels_label",
            "idx_messages_account", "idx_messages_internal_date", "idx_messages_thread",
            "idx_thread_labels_label", "idx_thread_labels_thread", "idx_threads_account",
        ] {
            assert!(names.contains(expected), "Missing index: {expected}");
        }
    }

    #[tokio::test]
    async fn test_apply_schema_seeds_defaults() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM settings")
            .fetch_one(&pool).await.unwrap();
        assert!(count.0 >= 12, "Expected at least 12 default settings, got {}", count.0);

        let theme: (String,) = sqlx::query_as("SELECT value FROM settings WHERE key = 'theme'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(theme.0, "system");
    }

    #[tokio::test]
    async fn test_apply_schema_sets_version() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();

        let version: (String,) = sqlx::query_as("SELECT value FROM settings WHERE key = 'schema_version'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(version.0, CURRENT_SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn test_apply_schema_idempotent() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();
        apply_schema(&pool).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM settings WHERE key = 'theme'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 1, "Schema should be idempotent");
    }

    #[tokio::test]
    async fn test_settings_upsert() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();

        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
            .bind("test_key").bind("test_value").execute(&pool).await.unwrap();

        let val: (String,) = sqlx::query_as("SELECT value FROM settings WHERE key = 'test_key'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(val.0, "test_value");

        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
            .bind("test_key").bind("new_value").execute(&pool).await.unwrap();

        let new_val: (String,) = sqlx::query_as("SELECT value FROM settings WHERE key = 'test_key'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(new_val.0, "new_value");
    }

    #[tokio::test]
    async fn test_fts_search() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();

        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, subject, body_plain) VALUES (?, ?, ?, ?, ?, ?)")
            .bind("m1").bind("t1").bind("acc1").bind("alice@example.com").bind("Meeting tomorrow").bind("Let's meet at 3pm")
            .execute(&pool).await.unwrap();

        sqlx::query("INSERT INTO messages_fts (rowid, sender, subject, body_plain) VALUES ((SELECT rowid FROM messages WHERE id = 'm1'), ?, ?, ?)")
            .bind("alice@example.com").bind("Meeting tomorrow").bind("Let's meet at 3pm")
            .execute(&pool).await.unwrap();

        let results: Vec<(String,)> = sqlx::query_as(
            "SELECT m.thread_id FROM messages m JOIN messages_fts f ON m.rowid = f.rowid WHERE messages_fts MATCH 'meeting'"
        ).fetch_all(&pool).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "t1");
    }
}
