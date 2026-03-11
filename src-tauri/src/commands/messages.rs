use super::accounts::get_active_account;
use tauri::Manager;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct LocalMessage {
    pub id: String,
    pub thread_id: String,
    pub sender: String,
    pub recipients: String,
    pub subject: String,
    pub snippet: String,
    pub internal_date: i64,
    pub body_html: String,
    pub body_plain: String,
    pub is_draft: bool,
}

pub(crate) async fn get_messages_inner(
    pool: &sqlx::SqlitePool,
    thread_id: &str,
) -> Result<Vec<LocalMessage>, String> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        thread_id: Option<String>,
        sender: Option<String>,
        recipients: Option<String>,
        subject: Option<String>,
        snippet: Option<String>,
        internal_date: Option<i64>,
        body_html: Option<String>,
        body_plain: Option<String>,
        is_draft: bool,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT m.id, m.thread_id, m.sender, m.recipients, m.subject, m.snippet, m.internal_date, m.body_html, m.body_plain,
         EXISTS(SELECT 1 FROM message_labels ml WHERE ml.message_id = m.id AND ml.label_id = 'DRAFT') as is_draft
         FROM messages m WHERE m.thread_id = ? ORDER BY m.internal_date ASC"
    ).bind(thread_id).fetch_all(pool).await.map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| LocalMessage {
            id: r.id,
            thread_id: r.thread_id.unwrap_or_default(),
            sender: r.sender.unwrap_or_default(),
            recipients: r.recipients.unwrap_or_default(),
            subject: r.subject.unwrap_or_default(),
            snippet: r.snippet.unwrap_or_default(),
            internal_date: r.internal_date.unwrap_or(0),
            body_plain: r.body_plain.unwrap_or_default(),
            body_html: r.body_html.unwrap_or_default(),
            is_draft: r.is_draft,
        })
        .collect())
}

#[tauri::command]
pub async fn get_messages(
    app_handle: tauri::AppHandle,
    thread_id: String,
) -> Result<Vec<LocalMessage>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    get_messages_inner(pool.inner(), &thread_id).await
}

#[tauri::command]
pub async fn sync_thread_messages(
    app_handle: tauri::AppHandle,
    thread_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::gmail_api::fetch_messages_for_thread(
        pool.inner(),
        &account.id,
        &account.access_token,
        &thread_id,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_helpers::*;

    #[tokio::test]
    async fn test_get_messages_inner_empty() {
        let pool = setup_test_db().await;
        let messages = get_messages_inner(&pool, "nonexistent").await.unwrap();
        assert!(messages.is_empty());
    }

    #[tokio::test]
    async fn test_get_messages_inner_ordered_by_date() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m2", "t1", "acc1", "bob@test.com", "alice@test.com", "Reply", 2000).await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@test.com", "bob@test.com", "Original", 1000).await;

        let messages = get_messages_inner(&pool, "t1").await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, "m1");
        assert_eq!(messages[0].subject, "Original");
        assert_eq!(messages[1].id, "m2");
        assert_eq!(messages[1].subject, "Reply");
    }

    #[tokio::test]
    async fn test_get_messages_inner_draft_flag() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@test.com", "", "Draft msg", 1000).await;
        sqlx::query("INSERT INTO message_labels (message_id, label_id) VALUES ('m1', 'DRAFT')")
            .execute(&pool).await.unwrap();

        let messages = get_messages_inner(&pool, "t1").await.unwrap();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is_draft);
    }

    #[tokio::test]
    async fn test_get_messages_inner_non_draft() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@test.com", "", "Regular", 1000).await;

        let messages = get_messages_inner(&pool, "t1").await.unwrap();
        assert_eq!(messages.len(), 1);
        assert!(!messages[0].is_draft);
    }

    #[tokio::test]
    async fn test_get_messages_inner_with_html_body() {
        let pool = setup_test_db().await;
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, subject, internal_date, body_html, body_plain) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind("m1").bind("t1").bind("acc1").bind("alice@test.com").bind("HTML Test").bind(1000i64)
            .bind("<p>Hello <b>World</b></p>").bind("Hello World")
            .execute(&pool).await.unwrap();

        let msgs = get_messages_inner(&pool, "t1").await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].body_html, "<p>Hello <b>World</b></p>");
        assert_eq!(msgs[0].body_plain, "Hello World");
    }
}
