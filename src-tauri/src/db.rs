use anyhow::Result;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;
use tauri::Manager;

const CURRENT_SCHEMA_VERSION: &str = "3";

pub async fn apply_schema(pool: &SqlitePool) -> Result<()> {
    let schema = r#"
    CREATE TABLE IF NOT EXISTS accounts (
        id TEXT PRIMARY KEY,
        email TEXT,
        display_name TEXT,
        avatar_url TEXT,
        token_expiry INTEGER,
        is_active INTEGER DEFAULT 1,
        created_at INTEGER,
        credential_source TEXT DEFAULT 'builtin',
        provider_type TEXT DEFAULT 'gmail'
    );

    CREATE TABLE IF NOT EXISTS labels (
        id TEXT PRIMARY KEY,
        account_id TEXT,
        name TEXT,
        type TEXT,
        unread_count INTEGER,
        threads_total INTEGER DEFAULT 0,
        threads_unread INTEGER DEFAULT 0,
        bg_color TEXT,
        text_color TEXT
    );

    CREATE TABLE IF NOT EXISTS threads (
        id TEXT PRIMARY KEY,
        account_id TEXT,
        snippet TEXT,
        history_id TEXT,
        synced_history_id TEXT,
        last_message_internal_date INTEGER,
        unread INTEGER,
        sender TEXT,
        subject TEXT,
        latest_date INTEGER,
        metadata_synced INTEGER DEFAULT 0
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
        has_attachments INTEGER,
        rfc_message_id TEXT
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

    CREATE TABLE IF NOT EXISTS subscriptions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        account_id TEXT NOT NULL,
        sender_email TEXT NOT NULL,
        sender_name TEXT,
        detection_method TEXT NOT NULL,
        detection_details TEXT,
        unsubscribe_url TEXT,
        unsubscribe_mailto TEXT,
        supports_one_click INTEGER DEFAULT 0,
        status TEXT DEFAULT 'active',
        message_count INTEGER DEFAULT 1,
        read_count INTEGER DEFAULT 0,
        avg_frequency_days REAL,
        first_seen INTEGER NOT NULL,
        last_seen INTEGER NOT NULL,
        user_corrected INTEGER DEFAULT 0,
        UNIQUE(account_id, sender_email)
    );

    CREATE TABLE IF NOT EXISTS schema_migrations (
        version INTEGER PRIMARY KEY,
        applied_at TEXT DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS snoozed_threads (
        thread_id TEXT NOT NULL,
        account_id TEXT NOT NULL,
        snoozed_until INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (thread_id, account_id)
    );

    CREATE TABLE IF NOT EXISTS scheduled_sends (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        account_id TEXT NOT NULL,
        draft_id TEXT NOT NULL,
        thread_id TEXT,
        to_recipients TEXT NOT NULL,
        subject TEXT NOT NULL,
        send_at INTEGER NOT NULL,
        created_at INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS templates (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        subject TEXT DEFAULT '',
        body_html TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS imap_config (
        account_id TEXT PRIMARY KEY,
        imap_host TEXT NOT NULL,
        imap_port INTEGER NOT NULL DEFAULT 993,
        smtp_host TEXT NOT NULL,
        smtp_port INTEGER NOT NULL DEFAULT 587,
        auth_method TEXT NOT NULL DEFAULT 'password',
        use_tls INTEGER NOT NULL DEFAULT 1,
        caldav_url TEXT
    );

    CREATE TABLE IF NOT EXISTS imap_sync_state (
        account_id TEXT NOT NULL,
        folder TEXT NOT NULL,
        uid_validity INTEGER,
        highest_uid INTEGER,
        highest_modseq INTEGER,
        PRIMARY KEY (account_id, folder)
    );

    CREATE TABLE IF NOT EXISTS outlook_sync_state (
        account_id TEXT NOT NULL,
        folder_id TEXT NOT NULL,
        delta_link TEXT,
        PRIMARY KEY (account_id, folder_id)
    );

    CREATE INDEX IF NOT EXISTS idx_thread_labels_thread ON thread_labels(thread_id);
    CREATE INDEX IF NOT EXISTS idx_thread_labels_label ON thread_labels(label_id);
    CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_id);
    CREATE INDEX IF NOT EXISTS idx_messages_account ON messages(account_id);
    CREATE INDEX IF NOT EXISTS idx_messages_internal_date ON messages(internal_date);
    CREATE INDEX IF NOT EXISTS idx_message_labels_label ON message_labels(label_id);
    CREATE INDEX IF NOT EXISTS idx_threads_account ON threads(account_id);
    CREATE INDEX IF NOT EXISTS idx_labels_account ON labels(account_id);
    CREATE INDEX IF NOT EXISTS idx_snoozed_account_until ON snoozed_threads(account_id, snoozed_until);
    CREATE INDEX IF NOT EXISTS idx_subscriptions_account ON subscriptions(account_id);
    CREATE INDEX IF NOT EXISTS idx_subscriptions_sender ON subscriptions(sender_email);
    CREATE INDEX IF NOT EXISTS idx_subscriptions_status ON subscriptions(account_id, status);
    CREATE INDEX IF NOT EXISTS idx_scheduled_sends_time ON scheduled_sends(send_at);

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
        ("notifications_preview", "false"),
        ("sync_frequency", "30"),
        ("max_threads_sync", "100"),
        ("max_cache_mb", "500"),
        ("download_folder", ""),
        ("attachment_action", "open"),
        ("undo_send_delay", "5"),
        ("threads_per_page", "100"),
        ("shortcut_palette", "Meta+k"),
        ("shortcut_compose", "c"),
        ("shortcut_sync", "Meta+r"),
        ("shortcut_settings", "Meta+,"),
        ("shortcut_search", "/"),
        ("unified_indicator", "avatar"),
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
    )?.create_if_missing(true)
      .busy_timeout(std::time::Duration::from_secs(10));

    let pool = SqlitePool::connect_with(options).await?;
    apply_schema(&pool).await?;
    run_migrations(&pool).await?;

    Ok(pool)
}

async fn has_column(pool: &SqlitePool, table: &str, column: &str) -> bool {
    sqlx::query_scalar::<_, i32>(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = '{}'", table, column)
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0) > 0
}

async fn m001_add_synced_history_id(pool: &SqlitePool) -> Result<()> {
    if !has_column(pool, "threads", "synced_history_id").await {
        sqlx::query("ALTER TABLE threads ADD COLUMN synced_history_id TEXT")
            .execute(pool).await?;
    }
    Ok(())
}

async fn m002_add_thread_metadata_columns(pool: &SqlitePool) -> Result<()> {
    for (col, typ) in [("sender", "TEXT"), ("subject", "TEXT"), ("latest_date", "INTEGER"), ("metadata_synced", "INTEGER DEFAULT 0")] {
        if !has_column(pool, "threads", col).await {
            sqlx::query(&format!("ALTER TABLE threads ADD COLUMN {} {}", col, typ))
                .execute(pool).await?;
        }
    }
    Ok(())
}

async fn m003_add_label_stats_columns(pool: &SqlitePool) -> Result<()> {
    for (col, typ) in [("threads_total", "INTEGER DEFAULT 0"), ("threads_unread", "INTEGER DEFAULT 0")] {
        if !has_column(pool, "labels", col).await {
            sqlx::query(&format!("ALTER TABLE labels ADD COLUMN {} {}", col, typ))
                .execute(pool).await?;
        }
    }
    Ok(())
}

async fn m004_backfill_thread_metadata(pool: &SqlitePool) -> Result<()> {
    let needs_backfill: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM threads WHERE (metadata_synced IS NULL OR metadata_synced = 0) AND EXISTS (SELECT 1 FROM messages WHERE messages.thread_id = threads.id)"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0) > 0;

    if needs_backfill {
        println!("[Migration] Backfilling thread metadata from existing messages...");
        sqlx::query(
            "UPDATE threads SET
                sender = (SELECT m.sender FROM messages m WHERE m.thread_id = threads.id ORDER BY m.internal_date DESC LIMIT 1),
                subject = (SELECT m.subject FROM messages m WHERE m.thread_id = threads.id ORDER BY m.internal_date DESC LIMIT 1),
                latest_date = (SELECT MAX(m.internal_date) FROM messages m WHERE m.thread_id = threads.id),
                metadata_synced = 1
            WHERE (metadata_synced IS NULL OR metadata_synced = 0) AND EXISTS (SELECT 1 FROM messages WHERE messages.thread_id = threads.id)"
        )
        .execute(pool)
        .await
        .ok();
        println!("[Migration] Backfill complete");
    }
    Ok(())
}

async fn m005_add_label_colors(pool: &SqlitePool) -> Result<()> {
    for col in ["bg_color", "text_color"] {
        if !has_column(pool, "labels", col).await {
            sqlx::query(&format!("ALTER TABLE labels ADD COLUMN {} TEXT", col))
                .execute(pool).await?;
        }
    }
    Ok(())
}

async fn has_table(pool: &SqlitePool, table: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?"
    )
    .bind(table)
    .fetch_one(pool)
    .await
    .unwrap_or(0) > 0
}

async fn m006_create_subscriptions_table(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS subscriptions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id TEXT NOT NULL,
            sender_email TEXT NOT NULL,
            sender_name TEXT,
            detection_method TEXT NOT NULL,
            detection_details TEXT,
            unsubscribe_url TEXT,
            unsubscribe_mailto TEXT,
            supports_one_click INTEGER DEFAULT 0,
            status TEXT DEFAULT 'active',
            message_count INTEGER DEFAULT 1,
            read_count INTEGER DEFAULT 0,
            avg_frequency_days REAL,
            first_seen INTEGER NOT NULL,
            last_seen INTEGER NOT NULL,
            user_corrected INTEGER DEFAULT 0,
            UNIQUE(account_id, sender_email)
        )"
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_subscriptions_account ON subscriptions(account_id)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_subscriptions_sender ON subscriptions(sender_email)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_subscriptions_status ON subscriptions(account_id, status)")
        .execute(pool)
        .await?;

    Ok(())
}

async fn m007_create_snoozed_threads(pool: &SqlitePool) -> Result<()> {
    if !has_table(pool, "snoozed_threads").await {
        sqlx::query(
            "CREATE TABLE snoozed_threads (
                thread_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                snoozed_until INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (thread_id, account_id)
            )"
        )
        .execute(pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_snoozed_account_until ON snoozed_threads(account_id, snoozed_until)")
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn m008_add_credential_source(pool: &SqlitePool) -> Result<()> {
    if !has_column(pool, "accounts", "credential_source").await {
        sqlx::query("ALTER TABLE accounts ADD COLUMN credential_source TEXT DEFAULT 'builtin'")
            .execute(pool).await?;
    }
    Ok(())
}

async fn m009_create_ai_summary_cache(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS ai_summary_cache (
            thread_id TEXT PRIMARY KEY,
            summary TEXT NOT NULL,
            message_count INTEGER NOT NULL,
            latest_message_date INTEGER NOT NULL,
            created_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;
    Ok(())
}

async fn m010_create_scheduled_sends(pool: &SqlitePool) -> Result<()> {
    if !has_table(pool, "scheduled_sends").await {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS scheduled_sends (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_id TEXT NOT NULL,
                draft_id TEXT NOT NULL,
                thread_id TEXT,
                to_recipients TEXT NOT NULL,
                subject TEXT NOT NULL,
                send_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            )"
        )
        .execute(pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_scheduled_sends_time ON scheduled_sends(send_at)")
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn m011_create_templates(pool: &SqlitePool) -> Result<()> {
    if !has_table(pool, "templates").await {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS templates (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                subject TEXT DEFAULT '',
                body_html TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )"
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn m012_add_provider_type(pool: &SqlitePool) -> Result<()> {
    if !has_column(pool, "accounts", "provider_type").await {
        sqlx::query("ALTER TABLE accounts ADD COLUMN provider_type TEXT DEFAULT 'gmail'")
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn m013_create_imap_config(pool: &SqlitePool) -> Result<()> {
    if !has_table(pool, "imap_config").await {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS imap_config (
                account_id TEXT PRIMARY KEY,
                imap_host TEXT NOT NULL,
                imap_port INTEGER NOT NULL DEFAULT 993,
                smtp_host TEXT NOT NULL,
                smtp_port INTEGER NOT NULL DEFAULT 587,
                auth_method TEXT NOT NULL DEFAULT 'password',
                use_tls INTEGER NOT NULL DEFAULT 1
            )"
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn m014_create_imap_sync_state(pool: &SqlitePool) -> Result<()> {
    if !has_table(pool, "imap_sync_state").await {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS imap_sync_state (
                account_id TEXT NOT NULL,
                folder TEXT NOT NULL,
                uid_validity INTEGER,
                highest_uid INTEGER,
                highest_modseq INTEGER,
                PRIMARY KEY (account_id, folder)
            )"
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn m015_create_outlook_sync_state(pool: &SqlitePool) -> Result<()> {
    if !has_table(pool, "outlook_sync_state").await {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS outlook_sync_state (
                account_id TEXT NOT NULL,
                folder_id TEXT NOT NULL,
                delta_link TEXT,
                PRIMARY KEY (account_id, folder_id)
            )"
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn m016_add_rfc_message_id(pool: &SqlitePool) -> Result<()> {
    if !has_column(pool, "messages", "rfc_message_id").await {
        sqlx::query("ALTER TABLE messages ADD COLUMN rfc_message_id TEXT")
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn m017_add_caldav_url(pool: &SqlitePool) -> Result<()> {
    if !has_column(pool, "imap_config", "caldav_url").await {
        sqlx::query("ALTER TABLE imap_config ADD COLUMN caldav_url TEXT")
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub(crate) async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    let applied: Vec<i64> = sqlx::query_scalar("SELECT version FROM schema_migrations")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    if applied.is_empty() {
        let bootstrap_checks: Vec<(i64, &str, &str)> = vec![
            (1, "threads", "synced_history_id"),
            (2, "threads", "metadata_synced"),
            (3, "labels", "threads_total"),
            (5, "labels", "bg_color"),
            (8, "accounts", "credential_source"),
        ];
        for (version, table, column) in &bootstrap_checks {
            if has_column(pool, table, column).await {
                let _ = sqlx::query("INSERT OR IGNORE INTO schema_migrations (version) VALUES (?)")
                    .bind(version)
                    .execute(pool)
                    .await;
            }
        }
        // Bootstrap: mark table-based migrations if tables already exist
        if has_table(pool, "snoozed_threads").await {
            let _ = sqlx::query("INSERT OR IGNORE INTO schema_migrations (version) VALUES (?)")
                .bind(6i64)
                .execute(pool)
                .await;
        }
        if has_table(pool, "ai_summary_cache").await {
            let _ = sqlx::query("INSERT OR IGNORE INTO schema_migrations (version) VALUES (?)")
                .bind(9i64)
                .execute(pool)
                .await;
        }
        if has_table(pool, "scheduled_sends").await {
            let _ = sqlx::query("INSERT OR IGNORE INTO schema_migrations (version) VALUES (?)")
                .bind(10i64)
                .execute(pool)
                .await;
        }
        if has_table(pool, "templates").await {
            let _ = sqlx::query("INSERT OR IGNORE INTO schema_migrations (version) VALUES (?)")
                .bind(11i64)
                .execute(pool)
                .await;
        }
        if has_column(pool, "accounts", "provider_type").await {
            let _ = sqlx::query("INSERT OR IGNORE INTO schema_migrations (version) VALUES (?)")
                .bind(12i64)
                .execute(pool)
                .await;
        }
        if has_table(pool, "imap_config").await {
            let _ = sqlx::query("INSERT OR IGNORE INTO schema_migrations (version) VALUES (?)")
                .bind(13i64)
                .execute(pool)
                .await;
        }
        if has_table(pool, "imap_sync_state").await {
            let _ = sqlx::query("INSERT OR IGNORE INTO schema_migrations (version) VALUES (?)")
                .bind(14i64)
                .execute(pool)
                .await;
        }
        if has_table(pool, "outlook_sync_state").await {
            let _ = sqlx::query("INSERT OR IGNORE INTO schema_migrations (version) VALUES (?)")
                .bind(15i64)
                .execute(pool)
                .await;
        }
        if has_column(pool, "messages", "rfc_message_id").await {
            let _ = sqlx::query("INSERT OR IGNORE INTO schema_migrations (version) VALUES (?)")
                .bind(16i64)
                .execute(pool)
                .await;
        }
        if has_column(pool, "imap_config", "caldav_url").await {
            let _ = sqlx::query("INSERT OR IGNORE INTO schema_migrations (version) VALUES (?)")
                .bind(17i64)
                .execute(pool)
                .await;
        }
        let applied_after: Vec<i64> = sqlx::query_scalar("SELECT version FROM schema_migrations")
            .fetch_all(pool)
            .await
            .unwrap_or_default();
        return run_pending_migrations(pool, &applied_after).await;
    }

    run_pending_migrations(pool, &applied).await
}

async fn run_pending_migrations(pool: &SqlitePool, applied: &[i64]) -> Result<()> {
    for version in 1..=17i64 {
        if !applied.contains(&version) {
            println!("[Migration] Running v{}...", version);
            match version {
                1 => m001_add_synced_history_id(pool).await?,
                2 => m002_add_thread_metadata_columns(pool).await?,
                3 => m003_add_label_stats_columns(pool).await?,
                4 => m004_backfill_thread_metadata(pool).await?,
                5 => m005_add_label_colors(pool).await?,
                6 => m006_create_subscriptions_table(pool).await?,
                7 => m007_create_snoozed_threads(pool).await?,
                8 => m008_add_credential_source(pool).await?,
                9 => m009_create_ai_summary_cache(pool).await?,
                10 => m010_create_scheduled_sends(pool).await?,
                11 => m011_create_templates(pool).await?,
                12 => m012_add_provider_type(pool).await?,
                13 => m013_create_imap_config(pool).await?,
                14 => m014_create_imap_sync_state(pool).await?,
                15 => m015_create_outlook_sync_state(pool).await?,
                16 => m016_add_rfc_message_id(pool).await?,
                17 => m017_add_caldav_url(pool).await?,
                _ => {}
            }
            sqlx::query("INSERT INTO schema_migrations (version) VALUES (?)")
                .bind(version)
                .execute(pool)
                .await?;
            println!("[Migration] v{} complete", version);
        }
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
            "message_labels", "messages", "messages_fts", "schema_migrations",
            "settings", "snoozed_threads", "subscriptions", "templates", "thread_labels", "threads",
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
            "idx_snoozed_account_until",
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
        assert!(count.0 >= 14, "Expected at least 14 default settings, got {}", count.0);

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

    #[tokio::test]
    async fn test_threads_has_metadata_columns() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();
        #[derive(sqlx::FromRow)]
        struct ColInfo { name: String }
        let cols: Vec<ColInfo> = sqlx::query_as("SELECT name FROM pragma_table_info('threads')")
            .fetch_all(&pool).await.unwrap();
        let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"sender"), "threads missing sender column");
        assert!(names.contains(&"subject"), "threads missing subject column");
        assert!(names.contains(&"latest_date"), "threads missing latest_date column");
        assert!(names.contains(&"metadata_synced"), "threads missing metadata_synced column");
    }

    #[tokio::test]
    async fn test_labels_has_thread_count_columns() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();
        #[derive(sqlx::FromRow)]
        struct ColInfo { name: String }
        let cols: Vec<ColInfo> = sqlx::query_as("SELECT name FROM pragma_table_info('labels')")
            .fetch_all(&pool).await.unwrap();
        let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"threads_total"), "labels missing threads_total column");
        assert!(names.contains(&"threads_unread"), "labels missing threads_unread column");
        assert!(names.contains(&"bg_color"), "labels missing bg_color column");
        assert!(names.contains(&"text_color"), "labels missing text_color column");
    }

    #[tokio::test]
    async fn test_migrations_are_tracked() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();
        run_migrations(&pool).await.unwrap();

        let versions: Vec<i64> = sqlx::query_scalar("SELECT version FROM schema_migrations ORDER BY version")
            .fetch_all(&pool).await.unwrap();
        assert!(versions.contains(&1));
        assert!(versions.contains(&2));
        assert!(versions.contains(&3));
        assert!(versions.contains(&4));
        assert!(versions.contains(&5));
        assert!(versions.contains(&6));
    }

    #[tokio::test]
    async fn test_migrations_idempotent() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();
        run_migrations(&pool).await.unwrap();
        run_migrations(&pool).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_migrations")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 17);
    }

    #[tokio::test]
    async fn test_subscriptions_table_exists() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();

        let tables: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name = 'subscriptions'"
        ).fetch_all(&pool).await.unwrap();

        assert!(!tables.is_empty(), "subscriptions table should exist");
    }

    #[tokio::test]
    async fn test_migrations_include_m006() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();
        run_migrations(&pool).await.unwrap();

        let versions: Vec<i64> = sqlx::query_scalar("SELECT version FROM schema_migrations WHERE version = 6")
            .fetch_all(&pool).await.unwrap();
        assert!(!versions.is_empty(), "migration 6 should be applied");
    }

    #[tokio::test]
    async fn test_scheduled_sends_table_exists() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();
        let result: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='scheduled_sends'"
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(result.0, 1);
    }

    #[tokio::test]
    async fn test_scheduled_sends_insert_and_query() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO scheduled_sends (account_id, draft_id, thread_id, to_recipients, subject, send_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind("acc1").bind("draft123").bind("thread456")
        .bind("test@test.com").bind("Test Subject")
        .bind(now + 3600).bind(now)
        .execute(&pool).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scheduled_sends")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn test_scheduled_sends_query_overdue() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp();

        // Insert one overdue and one future
        sqlx::query(
            "INSERT INTO scheduled_sends (account_id, draft_id, to_recipients, subject, send_at, created_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("acc1").bind("d1").bind("a@b.com").bind("Overdue").bind(now - 60).bind(now).execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO scheduled_sends (account_id, draft_id, to_recipients, subject, send_at, created_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("acc1").bind("d2").bind("c@d.com").bind("Future").bind(now + 3600).bind(now).execute(&pool).await.unwrap();

        let overdue: Vec<(String,)> = sqlx::query_as(
            "SELECT draft_id FROM scheduled_sends WHERE send_at <= ?"
        ).bind(now).fetch_all(&pool).await.unwrap();
        assert_eq!(overdue.len(), 1);
        assert_eq!(overdue[0].0, "d1");
    }

    #[tokio::test]
    async fn test_templates_table_exists() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();
        let result: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='templates'"
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(result.0, 1);
    }

    #[tokio::test]
    async fn test_templates_insert_and_query() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO templates (id, name, subject, body_html, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind("t1").bind("Meeting Follow-up").bind("Re: Our Meeting")
        .bind("<p>Thanks for the meeting</p>").bind(now).bind(now)
        .execute(&pool).await.unwrap();

        let row: (String, String) = sqlx::query_as("SELECT name, subject FROM templates WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(row.0, "Meeting Follow-up");
        assert_eq!(row.1, "Re: Our Meeting");
    }

    #[tokio::test]
    async fn test_provider_type_column_exists() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO accounts (id, email, display_name, is_active, created_at, provider_type) VALUES (?, ?, ?, 1, 0, 'imap')"
        ).bind("acc1").bind("user@outlook.com").bind("Test User")
        .execute(&pool).await.unwrap();

        let pt: String = sqlx::query_scalar("SELECT provider_type FROM accounts WHERE id = 'acc1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(pt, "imap");
    }

    #[tokio::test]
    async fn test_provider_type_defaults_to_gmail() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO accounts (id, email, display_name, is_active, created_at) VALUES (?, ?, ?, 1, 0)"
        ).bind("acc1").bind("user@gmail.com").bind("Gmail User")
        .execute(&pool).await.unwrap();

        let pt: String = sqlx::query_scalar(
            "SELECT COALESCE(provider_type, 'gmail') FROM accounts WHERE id = 'acc1'"
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(pt, "gmail");
    }

    #[tokio::test]
    async fn test_imap_config_table_exists() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();

        let result: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='imap_config'"
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(result.0, 1);
    }

    #[tokio::test]
    async fn test_imap_config_insert_and_query() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO accounts (id, email, display_name, is_active, created_at, provider_type) VALUES (?, ?, ?, 1, 0, 'imap')"
        ).bind("acc1").bind("user@outlook.com").bind("Test")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO imap_config (account_id, imap_host, imap_port, smtp_host, smtp_port, auth_method, use_tls) VALUES (?, ?, ?, ?, ?, ?, ?)"
        ).bind("acc1").bind("outlook.office365.com").bind(993)
        .bind("smtp.office365.com").bind(587)
        .bind("password").bind(1)
        .execute(&pool).await.unwrap();

        let row: (String, i32, String, i32) = sqlx::query_as(
            "SELECT imap_host, imap_port, smtp_host, smtp_port FROM imap_config WHERE account_id = 'acc1'"
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(row.0, "outlook.office365.com");
        assert_eq!(row.1, 993);
        assert_eq!(row.2, "smtp.office365.com");
        assert_eq!(row.3, 587);
    }

    #[tokio::test]
    async fn test_imap_sync_state_table_exists() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();

        let result: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='imap_sync_state'"
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(result.0, 1);
    }

    #[tokio::test]
    async fn test_imap_sync_state_insert_and_query() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO imap_sync_state (account_id, folder, uid_validity, highest_uid, highest_modseq) VALUES (?, ?, ?, ?, ?)"
        ).bind("acc1").bind("INBOX").bind(12345i64).bind(500i64).bind(100i64)
        .execute(&pool).await.unwrap();

        let row: (i64, i64, i64) = sqlx::query_as(
            "SELECT uid_validity, highest_uid, highest_modseq FROM imap_sync_state WHERE account_id = 'acc1' AND folder = 'INBOX'"
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(row.0, 12345);
        assert_eq!(row.1, 500);
        assert_eq!(row.2, 100);
    }

    #[tokio::test]
    async fn test_imap_sync_state_multi_folder() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();

        for folder in &["INBOX", "Sent", "Drafts"] {
            sqlx::query(
                "INSERT INTO imap_sync_state (account_id, folder, uid_validity, highest_uid) VALUES (?, ?, ?, ?)"
            ).bind("acc1").bind(folder).bind(1i64).bind(10i64)
            .execute(&pool).await.unwrap();
        }

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM imap_sync_state WHERE account_id = 'acc1'"
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 3);
    }

    #[tokio::test]
    async fn test_rfc_message_id_column_exists() {
        let pool = test_pool().await;
        apply_schema(&pool).await.unwrap();
        run_migrations(&pool).await.unwrap();

        // Verify column exists via pragma
        let has_col = has_column(&pool, "messages", "rfc_message_id").await;
        assert!(has_col, "messages table should have rfc_message_id column");

        // Verify we can insert and read the column
        sqlx::query(
            "INSERT INTO messages (id, thread_id, account_id, sender, subject, internal_date, rfc_message_id) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind("m1").bind("t1").bind("acc1").bind("test@test.com").bind("Test").bind(1000i64).bind("<abc@example.com>")
        .execute(&pool).await.unwrap();

        let rfc_id: Option<String> = sqlx::query_scalar("SELECT rfc_message_id FROM messages WHERE id = 'm1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(rfc_id, Some("<abc@example.com>".to_string()));
    }
}
