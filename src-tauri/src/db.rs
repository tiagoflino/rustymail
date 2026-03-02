use anyhow::Result;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;
use tauri::Manager;

pub async fn init_db(app_handle: &tauri::AppHandle) -> Result<SqlitePool> {
    let app_dir = app_handle.path().app_data_dir().expect("Failed to get app data dir");
    std::fs::create_dir_all(&app_dir)?;

    let db_path = app_dir.join("rustymail.db");
    
    let options = SqliteConnectOptions::from_str(
        &format!("sqlite://{}", db_path.to_string_lossy())
    )?.create_if_missing(true);

    let pool = SqlitePool::connect_with(options).await?;

    let schema = r#"
    CREATE TABLE IF NOT EXISTS accounts (
        id TEXT PRIMARY KEY,
        email TEXT,
        display_name TEXT,
        avatar_url TEXT,
        access_token TEXT,
        refresh_token TEXT,
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
    "#;

    sqlx::query(schema).execute(&pool).await?;

    let _ = sqlx::query("ALTER TABLE accounts ADD COLUMN display_name TEXT").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE accounts ADD COLUMN avatar_url TEXT").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE accounts ADD COLUMN is_active INTEGER DEFAULT 1").execute(&pool).await;

    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(sender, subject, body_plain, content=messages, content_rowid=rowid)"
    ).execute(&pool).await.ok();

    let defaults = [
        ("theme", "system"),
        ("density", "default"),
        ("default_mailbox", "INBOX"),
        ("mark_read_delay", "instant"),
        ("reading_pane", "right"),
        ("signature", ""),
        ("reply_position", "above"),
        ("notifications_enabled", "true"),
        ("notifications_sound", "true"),
        ("sync_frequency", "30"),
        ("max_threads_sync", "100"),
        ("max_cache_mb", "500"),
        ("mark_read_delay", "2"),
        ("default_mailbox", "INBOX"),
    ];
    for (key, value) in defaults {
        let _ = sqlx::query("INSERT OR IGNORE INTO settings (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(&pool)
            .await;
    }

    Ok(pool)
}
