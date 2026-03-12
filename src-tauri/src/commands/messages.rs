use super::accounts::get_active_account;
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

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
    pub has_attachments: bool,
}

#[derive(serde::Serialize)]
pub struct Attachment {
    pub id: String,
    pub message_id: String,
    pub filename: String,
    pub mime_type: String,
    pub size: i32,
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
        has_attachments: bool,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT m.id, m.thread_id, m.sender, m.recipients, m.subject, m.snippet, m.internal_date, m.body_html, m.body_plain,
         EXISTS(SELECT 1 FROM message_labels ml WHERE ml.message_id = m.id AND ml.label_id = 'DRAFT') as is_draft,
         COALESCE(m.has_attachments, 0) as has_attachments
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
            has_attachments: r.has_attachments,
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

#[tauri::command]
pub async fn get_attachments(
    app_handle: tauri::AppHandle,
    message_id: String,
) -> Result<Vec<Attachment>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();

    #[derive(sqlx::FromRow)]
    struct AttachmentRow {
        id: String,
        message_id: String,
        filename: Option<String>,
        mime_type: Option<String>,
        size: Option<i32>,
    }

    let rows: Vec<AttachmentRow> = sqlx::query_as(
        "SELECT id, message_id, filename, mime_type, size FROM attachments WHERE message_id = ?",
    )
    .bind(&message_id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| Attachment {
            id: r.id,
            message_id: r.message_id,
            filename: r.filename.unwrap_or_default(),
            mime_type: r.mime_type.unwrap_or_default(),
            size: r.size.unwrap_or(0),
        })
        .collect())
}

#[tauri::command]
pub async fn download_attachment(
    app_handle: tauri::AppHandle,
    message_id: String,
    attachment_id: String,
    filename: String,
) -> Result<String, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    let bytes = crate::gmail_api::download_attachment(
        &account.access_token,
        &message_id,
        &attachment_id,
    )
    .await?;

    let custom_folder: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'download_folder'")
        .fetch_optional(pool.inner())
        .await
        .unwrap_or(None);

    let downloads = match custom_folder {
        Some(ref path) if !path.is_empty() => {
            let p = std::path::PathBuf::from(path);
            if p.is_dir() { p } else {
                dirs::download_dir()
                    .or_else(|| dirs::home_dir().map(|d| d.join("Downloads")))
                    .ok_or("Could not find downloads directory")?
            }
        }
        _ => dirs::download_dir()
            .or_else(|| dirs::home_dir().map(|d| d.join("Downloads")))
            .ok_or("Could not find downloads directory")?,
    };

    let mut save_path = downloads.join(&filename);
    if save_path.exists() {
        let stem = std::path::Path::new(&filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&filename);
        let ext = std::path::Path::new(&filename)
            .extension()
            .and_then(|s| s.to_str());
        let mut counter = 1u32;
        loop {
            let new_name = match ext {
                Some(e) => format!("{} ({}).{}", stem, counter, e),
                None => format!("{} ({})", stem, counter),
            };
            save_path = downloads.join(&new_name);
            if !save_path.exists() {
                break;
            }
            counter += 1;
        }
    }
    std::fs::write(&save_path, &bytes).map_err(|e| format!("Failed to save file: {}", e))?;

    Ok(save_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn open_attachment(
    app_handle: tauri::AppHandle,
    message_id: String,
    attachment_id: String,
    filename: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    let bytes = crate::gmail_api::download_attachment(
        &account.access_token,
        &message_id,
        &attachment_id,
    )
    .await?;

    let temp_dir = std::env::temp_dir().join("rustymail-attachments");
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temp directory: {}", e))?;

    let save_path = temp_dir.join(&filename);
    std::fs::write(&save_path, &bytes)
        .map_err(|e| format!("Failed to save file: {}", e))?;

    app_handle
        .opener()
        .open_path(save_path.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("Failed to open file: {}", e))
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

    async fn get_attachments_inner(pool: &sqlx::SqlitePool, message_id: &str) -> Result<Vec<Attachment>, String> {
        #[derive(sqlx::FromRow)]
        struct AttachmentRow {
            id: String,
            message_id: String,
            filename: Option<String>,
            mime_type: Option<String>,
            size: Option<i32>,
        }

        let rows: Vec<AttachmentRow> = sqlx::query_as(
            "SELECT id, message_id, filename, mime_type, size FROM attachments WHERE message_id = ?",
        )
        .bind(message_id)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(rows
            .into_iter()
            .map(|r| Attachment {
                id: r.id,
                message_id: r.message_id,
                filename: r.filename.unwrap_or_default(),
                mime_type: r.mime_type.unwrap_or_default(),
                size: r.size.unwrap_or(0),
            })
            .collect())
    }

    #[tokio::test]
    async fn test_get_messages_inner_has_attachments_flag() {
        let pool = setup_test_db().await;
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, ?, ?, ?, '', ?, '', '', 1)")
            .bind("m1").bind("t1").bind("acc1").bind("alice@test.com").bind("bob@test.com").bind("With Attachment").bind(1000i64)
            .execute(&pool).await.unwrap();

        let messages = get_messages_inner(&pool, "t1").await.unwrap();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].has_attachments);
    }

    #[tokio::test]
    async fn test_get_messages_inner_has_attachments_default_false() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@test.com", "bob@test.com", "No Attachment", 1000).await;

        let messages = get_messages_inner(&pool, "t1").await.unwrap();
        assert_eq!(messages.len(), 1);
        assert!(!messages[0].has_attachments);
    }

    #[tokio::test]
    async fn test_get_attachments_returns_stored_attachments() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@test.com", "bob@test.com", "Msg", 1000).await;
        sqlx::query("INSERT INTO attachments (id, message_id, filename, mime_type, size, downloaded) VALUES (?, ?, ?, ?, ?, 0)")
            .bind("att1").bind("m1").bind("report.pdf").bind("application/pdf").bind(1024)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO attachments (id, message_id, filename, mime_type, size, downloaded) VALUES (?, ?, ?, ?, ?, 0)")
            .bind("att2").bind("m1").bind("image.png").bind("image/png").bind(2048)
            .execute(&pool).await.unwrap();

        let attachments = get_attachments_inner(&pool, "m1").await.unwrap();
        assert_eq!(attachments.len(), 2);
        assert_eq!(attachments[0].id, "att1");
        assert_eq!(attachments[0].filename, "report.pdf");
        assert_eq!(attachments[0].mime_type, "application/pdf");
        assert_eq!(attachments[0].size, 1024);
        assert_eq!(attachments[1].id, "att2");
        assert_eq!(attachments[1].filename, "image.png");
        assert_eq!(attachments[1].mime_type, "image/png");
        assert_eq!(attachments[1].size, 2048);
    }

    #[tokio::test]
    async fn test_get_attachments_empty_for_no_attachments() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@test.com", "bob@test.com", "Msg", 1000).await;

        let attachments = get_attachments_inner(&pool, "m1").await.unwrap();
        assert!(attachments.is_empty());
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
