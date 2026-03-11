use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use std::str::FromStr;

pub async fn setup_test_db() -> SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
    let pool = SqlitePool::connect_with(options).await.unwrap();
    crate::db::apply_schema(&pool).await.unwrap();
    pool
}

pub async fn insert_account(pool: &SqlitePool, id: &str, email: &str, display_name: &str, is_active: i32, created_at: i64) {
    sqlx::query("INSERT INTO accounts (id, email, display_name, avatar_url, token_expiry, is_active, created_at) VALUES (?, ?, ?, '', 9999999999, ?, ?)")
        .bind(id).bind(email).bind(display_name).bind(is_active).bind(created_at)
        .execute(pool).await.unwrap();
}

pub async fn insert_message(pool: &SqlitePool, id: &str, thread_id: &str, account_id: &str, sender: &str, recipients: &str, subject: &str, internal_date: i64) {
    sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, ?, ?, ?, '', ?, '', '', 0)")
        .bind(id).bind(thread_id).bind(account_id).bind(sender).bind(recipients).bind(subject).bind(internal_date)
        .execute(pool).await.unwrap();
}

pub async fn insert_thread(pool: &SqlitePool, id: &str, account_id: &str) {
    sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0)")
        .bind(id).bind(account_id)
        .execute(pool).await.unwrap();
}
