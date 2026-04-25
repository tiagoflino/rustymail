use super::connection::{self, ImapConfig};
use super::folders;
use super::sync;
use super::smtp;
use super::operations;
use crate::provider::types::{Folder, ProviderCapabilities};
use futures::StreamExt;

pub struct ImapProvider {
    pub config: ImapConfig,
}

impl ImapProvider {
    pub fn new(config: ImapConfig) -> Self {
        Self { config }
    }

    pub fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::imap()
    }

    pub async fn list_folders(&self, pool: &sqlx::SqlitePool) -> Result<Vec<Folder>, String> {
        let mut session = connection::connect(&self.config).await?;
        let discovered = folders::discover_folders(&mut session).await?;
        folders::sync_folders_to_labels(pool, &self.config.account_id, &discovered).await?;
        let _ = session.logout().await;
        Ok(discovered)
    }

    pub async fn sync_folder(
        &self,
        pool: &sqlx::SqlitePool,
        folder: &str,
    ) -> Result<sync::SyncResult, String> {
        let mut session = connection::connect(&self.config).await?;
        let result = sync::sync_folder(&mut session, pool, &self.config.account_id, folder).await?;
        let _ = session.logout().await;
        Ok(result)
    }

    pub async fn send_message(&self, message: &lettre::Message) -> Result<(), String> {
        let password = crate::credentials::get_imap_password(&self.config.account_id)?;
        smtp::send_via_smtp(
            &self.config.smtp_host,
            self.config.smtp_port,
            &self.config.username,
            &password,
            message,
        )
        .await
    }

    pub async fn mark_read(
        &self,
        pool: &sqlx::SqlitePool,
        thread_id: &str,
        read: bool,
    ) -> Result<(), String> {
        let uids = get_thread_message_uids(pool, thread_id).await?;
        if uids.is_empty() {
            return Ok(());
        }

        let folder = get_thread_folder(pool, thread_id).await?;
        let mut session = connection::connect(&self.config).await?;
        operations::mark_read(&mut session, &folder, &uids, read).await?;
        let _ = session.logout().await;

        let unread_val = if read { 0 } else { 1 };
        sqlx::query("UPDATE threads SET unread = ? WHERE id = ?")
            .bind(unread_val)
            .bind(thread_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub async fn set_star(
        &self,
        pool: &sqlx::SqlitePool,
        thread_id: &str,
        starred: bool,
    ) -> Result<(), String> {
        let uids = get_thread_message_uids(pool, thread_id).await?;
        if uids.is_empty() {
            return Ok(());
        }

        let folder = get_thread_folder(pool, thread_id).await?;
        let mut session = connection::connect(&self.config).await?;
        operations::set_starred(&mut session, &folder, &uids, starred).await?;
        let _ = session.logout().await;

        if starred {
            sqlx::query("INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, 'STARRED')")
                .bind(thread_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
        } else {
            sqlx::query("DELETE FROM thread_labels WHERE thread_id = ? AND label_id = 'STARRED'")
                .bind(thread_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    pub async fn trash_thread(
        &self,
        pool: &sqlx::SqlitePool,
        thread_id: &str,
    ) -> Result<(), String> {
        let messages = get_thread_messages_with_rfc_ids(pool, thread_id).await?;
        if messages.is_empty() {
            return Ok(());
        }

        let folder = get_thread_folder(pool, thread_id).await?;
        let trash_folder = find_special_folder(pool, &self.config.account_id, "TRASH")
            .await
            .unwrap_or_else(|| "Trash".to_string());

        let uids: Vec<u32> = messages.iter().map(|(uid, _, _)| *uid).collect();
        let rfc_ids: Vec<Option<String>> = messages.iter().map(|(_, _, rfc)| rfc.clone()).collect();

        let mut session = connection::connect(&self.config).await?;
        let new_mappings = operations::trash_messages(&mut session, &folder, &trash_folder, &uids, &rfc_ids).await?;
        let _ = session.logout().await;

        update_message_ids_after_move(pool, &self.config.account_id, &trash_folder, &messages, &new_mappings).await?;

        sqlx::query("DELETE FROM thread_labels WHERE thread_id = ? AND label_id = 'INBOX'")
            .bind(thread_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, 'TRASH')")
            .bind(thread_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub async fn archive_thread(
        &self,
        pool: &sqlx::SqlitePool,
        thread_id: &str,
    ) -> Result<(), String> {
        let messages = get_thread_messages_with_rfc_ids(pool, thread_id).await?;
        if messages.is_empty() {
            return Ok(());
        }

        let folder = get_thread_folder(pool, thread_id).await?;
        let archive_folder = find_special_folder(pool, &self.config.account_id, "imap:Archive")
            .await
            .unwrap_or_else(|| "Archive".to_string());

        let uids: Vec<u32> = messages.iter().map(|(uid, _, _)| *uid).collect();
        let rfc_ids: Vec<Option<String>> = messages.iter().map(|(_, _, rfc)| rfc.clone()).collect();

        let mut session = connection::connect(&self.config).await?;
        let new_mappings = operations::archive_messages(&mut session, &folder, &archive_folder, &uids, &rfc_ids).await?;
        let _ = session.logout().await;

        update_message_ids_after_move(pool, &self.config.account_id, &archive_folder, &messages, &new_mappings).await?;

        sqlx::query("DELETE FROM thread_labels WHERE thread_id = ? AND label_id = 'INBOX'")
            .bind(thread_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub async fn untrash_thread(
        &self,
        pool: &sqlx::SqlitePool,
        thread_id: &str,
    ) -> Result<(), String> {
        let messages = get_thread_messages_with_rfc_ids(pool, thread_id).await?;
        if messages.is_empty() {
            return Ok(());
        }

        let trash_folder = find_special_folder(pool, &self.config.account_id, "TRASH")
            .await
            .unwrap_or_else(|| "Trash".to_string());
        let inbox_folder = "INBOX".to_string();

        let uids: Vec<u32> = messages.iter().map(|(uid, _, _)| *uid).collect();
        let rfc_ids: Vec<Option<String>> = messages.iter().map(|(_, _, rfc)| rfc.clone()).collect();

        let mut session = connection::connect(&self.config).await?;
        let new_mappings = operations::move_messages(&mut session, &trash_folder, &inbox_folder, &uids, &rfc_ids).await?;
        let _ = session.logout().await;

        update_message_ids_after_move(pool, &self.config.account_id, &inbox_folder, &messages, &new_mappings).await?;

        sqlx::query("DELETE FROM thread_labels WHERE thread_id = ? AND label_id = 'TRASH'")
            .bind(thread_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, 'INBOX')")
            .bind(thread_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub async fn fetch_attachment_data(
        &self,
        _pool: &sqlx::SqlitePool,
        message_id: &str,
        attachment_id: &str,
    ) -> Result<Vec<u8>, String> {
        let parts: Vec<&str> = message_id.splitn(4, ':').collect();
        if parts.len() < 4 || parts[0] != "imap" {
            return Err(format!("Invalid IMAP message ID: {}", message_id));
        }
        let folder = parts[2];
        let uid: u32 = parts[3].parse().map_err(|_| "Invalid UID")?;

        let mut session = connection::connect(&self.config).await?;
        session.select(folder).await.map_err(|e| e.to_string())?;

        let fetch_stream = session
            .uid_fetch(uid.to_string(), "(BODY[])")
            .await
            .map_err(|e| format!("FETCH BODY failed: {}", e))?;

        let fetched: Vec<_> = fetch_stream.filter_map(|r| async { r.ok() }).collect().await;
        let _ = session.logout().await;

        for msg in &fetched {
            if let Some(body) = msg.body() {
                let parsed = mail_parser::MessageParser::default().parse(body);
                if let Some(parsed) = parsed {
                    // attachment_id is the 0-based index of the attachment
                    let idx: usize = attachment_id.parse().unwrap_or(0);
                    for (att_index, att) in parsed.attachments().enumerate() {
                        if att_index == idx {
                            return Ok(att.contents().to_vec());
                        }
                    }
                    return Err("Attachment not found in message".to_string());
                }
            }
        }

        Err("Failed to fetch message body for attachment extraction".to_string())
    }

    pub async fn fetch_message_body(
        &self,
        pool: &sqlx::SqlitePool,
        message_id: &str,
    ) -> Result<(), String> {
        let parts: Vec<&str> = message_id.splitn(4, ':').collect();
        if parts.len() < 4 || parts[0] != "imap" {
            return Err(format!("Invalid IMAP message ID: {}", message_id));
        }
        let folder = parts[2];
        let uid: u32 = parts[3].parse().map_err(|_| "Invalid UID")?;

        let mut session = connection::connect(&self.config).await?;
        session.select(folder).await.map_err(|e| e.to_string())?;

        let fetch_stream = session
            .uid_fetch(uid.to_string(), "(BODY[])")
            .await
            .map_err(|e| format!("FETCH BODY failed: {}", e))?;

        let fetched: Vec<_> = fetch_stream.filter_map(|r| async { r.ok() }).collect().await;

        for msg in &fetched {
            if let Some(body) = msg.body() {
                let parsed = mail_parser::MessageParser::default().parse(body);
                if let Some(parsed) = parsed {
                    let body_html = parsed
                        .body_html(0)
                        .map(|h| crate::email_utils::sanitize_email_html(&h))
                        .unwrap_or_default();
                    let body_plain = parsed.body_text(0).map(|t| t.to_string()).unwrap_or_default();

                    sqlx::query(
                        "UPDATE messages SET body_html = ?, body_plain = ? WHERE id = ?",
                    )
                    .bind(&body_html)
                    .bind(&body_plain)
                    .bind(message_id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                }
            }
        }

        let _ = session.logout().await;
        Ok(())
    }
}

async fn get_thread_message_uids(pool: &sqlx::SqlitePool, thread_id: &str) -> Result<Vec<u32>, String> {
    let ids: Vec<(String,)> = sqlx::query_as(
        "SELECT id FROM messages WHERE thread_id = ?",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut uids = Vec::new();
    for (id,) in ids {
        let parts: Vec<&str> = id.splitn(4, ':').collect();
        if parts.len() >= 4 {
            if let Ok(uid) = parts[3].parse::<u32>() {
                uids.push(uid);
            }
        }
    }
    Ok(uids)
}

async fn get_thread_messages_with_rfc_ids(
    pool: &sqlx::SqlitePool,
    thread_id: &str,
) -> Result<Vec<(u32, String, Option<String>)>, String> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT id, rfc_message_id FROM messages WHERE thread_id = ?",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for (id, rfc_id) in rows {
        let parts: Vec<&str> = id.splitn(4, ':').collect();
        if parts.len() >= 4 {
            if let Ok(uid) = parts[3].parse::<u32>() {
                result.push((uid, id, rfc_id));
            }
        }
    }
    Ok(result)
}

async fn update_message_ids_after_move(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    new_folder: &str,
    old_messages: &[(u32, String, Option<String>)],
    new_mappings: &[(String, u32)],
) -> Result<(), String> {
    for (rfc_id, new_uid) in new_mappings {
        if let Some((_, old_id, _)) = old_messages
            .iter()
            .find(|(_, _, rfc)| rfc.as_deref() == Some(rfc_id.as_str()))
        {
            let new_id = format!("imap:{}:{}:{}", account_id, new_folder, new_uid);
            if *old_id != new_id {
                sqlx::query("UPDATE messages SET id = ? WHERE id = ?")
                    .bind(&new_id)
                    .bind(old_id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

async fn get_thread_folder(pool: &sqlx::SqlitePool, thread_id: &str) -> Result<String, String> {
    let id: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM messages WHERE thread_id = ? LIMIT 1",
    )
    .bind(thread_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    let id = id.ok_or_else(|| "No messages in thread".to_string())?.0;
    let parts: Vec<&str> = id.splitn(4, ':').collect();
    if parts.len() >= 3 {
        Ok(parts[2].to_string())
    } else {
        Err("Cannot determine folder from message ID".to_string())
    }
}

async fn find_special_folder(pool: &sqlx::SqlitePool, account_id: &str, label_id: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT name FROM labels WHERE id = ? AND account_id = ?",
    )
    .bind(label_id)
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_helpers::setup_test_db;

    #[tokio::test]
    async fn test_get_thread_message_uids_empty() {
        let pool = setup_test_db().await;
        let uids = get_thread_message_uids(&pool, "nonexistent-thread").await.unwrap();
        assert!(uids.is_empty());
    }

    #[tokio::test]
    async fn test_get_thread_message_uids_parses_imap_ids() {
        let pool = setup_test_db().await;
        sqlx::query(
            "INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0)",
        )
        .bind("t1").bind("acc1")
        .execute(&pool).await.unwrap();

        for uid in [10u32, 20, 30] {
            let msg_id = format!("imap:acc1:INBOX:{}", uid);
            sqlx::query(
                "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, '', '', '', '', 0, '', '', 0)",
            )
            .bind(&msg_id).bind("t1").bind("acc1")
            .execute(&pool).await.unwrap();
        }

        let mut uids = get_thread_message_uids(&pool, "t1").await.unwrap();
        uids.sort();
        assert_eq!(uids, vec![10, 20, 30]);
    }

    #[tokio::test]
    async fn test_get_thread_message_uids_ignores_non_imap_ids() {
        let pool = setup_test_db().await;
        sqlx::query(
            "INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0)",
        )
        .bind("t1").bind("acc1")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, '', '', '', '', 0, '', '', 0)",
        )
        .bind("18a3f5b2c1d0e9f8").bind("t1").bind("acc1")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, '', '', '', '', 0, '', '', 0)",
        )
        .bind("outlook:AAMkAG1").bind("t1").bind("acc1")
        .execute(&pool).await.unwrap();

        let uids = get_thread_message_uids(&pool, "t1").await.unwrap();
        assert!(uids.is_empty());
    }

    #[tokio::test]
    async fn test_get_thread_folder_extracts_folder() {
        let pool = setup_test_db().await;
        sqlx::query(
            "INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0)",
        )
        .bind("t1").bind("acc1")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, '', '', '', '', 0, '', '', 0)",
        )
        .bind("imap:acc1:INBOX:123").bind("t1").bind("acc1")
        .execute(&pool).await.unwrap();

        let folder = get_thread_folder(&pool, "t1").await.unwrap();
        assert_eq!(folder, "INBOX");
    }

    #[tokio::test]
    async fn test_get_thread_folder_no_messages() {
        let pool = setup_test_db().await;
        sqlx::query(
            "INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0)",
        )
        .bind("t1").bind("acc1")
        .execute(&pool).await.unwrap();

        let result = get_thread_folder(&pool, "t1").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No messages in thread"));
    }

    #[tokio::test]
    async fn test_find_special_folder_found() {
        let pool = setup_test_db().await;
        sqlx::query(
            "INSERT INTO labels (id, account_id, name, type) VALUES (?, ?, ?, 'system')",
        )
        .bind("TRASH").bind("acc1").bind("Deleted Items")
        .execute(&pool).await.unwrap();

        let result = find_special_folder(&pool, "acc1", "TRASH").await;
        assert_eq!(result, Some("Deleted Items".to_string()));
    }

    #[tokio::test]
    async fn test_find_special_folder_not_found() {
        let pool = setup_test_db().await;
        let result = find_special_folder(&pool, "acc1", "TRASH").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_thread_messages_with_rfc_ids() {
        let pool = setup_test_db().await;
        sqlx::query(
            "INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0)",
        )
        .bind("t1").bind("acc1")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments, rfc_message_id) VALUES (?, ?, ?, '', '', '', '', 0, '', '', 0, ?)",
        )
        .bind("imap:acc1:INBOX:100").bind("t1").bind("acc1").bind("<msg1@example.com>")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments, rfc_message_id) VALUES (?, ?, ?, '', '', '', '', 0, '', '', 0, ?)",
        )
        .bind("imap:acc1:INBOX:200").bind("t1").bind("acc1").bind("<msg2@example.com>")
        .execute(&pool).await.unwrap();

        let result = get_thread_messages_with_rfc_ids(&pool, "t1").await.unwrap();
        assert_eq!(result.len(), 2);

        let (uid1, id1, rfc1) = &result[0];
        assert_eq!(*uid1, 100);
        assert_eq!(id1, "imap:acc1:INBOX:100");
        assert_eq!(rfc1.as_deref(), Some("<msg1@example.com>"));

        let (uid2, id2, rfc2) = &result[1];
        assert_eq!(*uid2, 200);
        assert_eq!(id2, "imap:acc1:INBOX:200");
        assert_eq!(rfc2.as_deref(), Some("<msg2@example.com>"));
    }

    #[tokio::test]
    async fn test_update_message_ids_after_move() {
        let pool = setup_test_db().await;
        sqlx::query(
            "INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0)",
        )
        .bind("t1").bind("acc1")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments, rfc_message_id) VALUES (?, ?, ?, '', '', '', '', 0, '', '', 0, ?)",
        )
        .bind("imap:acc1:INBOX:100").bind("t1").bind("acc1").bind("<msg1@example.com>")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments, rfc_message_id) VALUES (?, ?, ?, '', '', '', '', 0, '', '', 0, ?)",
        )
        .bind("imap:acc1:INBOX:200").bind("t1").bind("acc1").bind("<msg2@example.com>")
        .execute(&pool).await.unwrap();

        let old_messages = vec![
            (100u32, "imap:acc1:INBOX:100".to_string(), Some("<msg1@example.com>".to_string())),
            (200u32, "imap:acc1:INBOX:200".to_string(), Some("<msg2@example.com>".to_string())),
        ];
        let new_mappings = vec![
            ("<msg1@example.com>".to_string(), 501u32),
            ("<msg2@example.com>".to_string(), 502u32),
        ];

        update_message_ids_after_move(&pool, "acc1", "Trash", &old_messages, &new_mappings)
            .await
            .unwrap();

        let ids: Vec<(String,)> = sqlx::query_as("SELECT id FROM messages WHERE thread_id = 't1' ORDER BY id")
            .fetch_all(&pool).await.unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.iter().any(|(id,)| id == "imap:acc1:Trash:501"));
        assert!(ids.iter().any(|(id,)| id == "imap:acc1:Trash:502"));
    }

    #[tokio::test]
    async fn test_update_message_ids_after_move_no_match() {
        let pool = setup_test_db().await;
        sqlx::query(
            "INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0)",
        )
        .bind("t1").bind("acc1")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments, rfc_message_id) VALUES (?, ?, ?, '', '', '', '', 0, '', '', 0, ?)",
        )
        .bind("imap:acc1:INBOX:100").bind("t1").bind("acc1").bind("<msg1@example.com>")
        .execute(&pool).await.unwrap();

        let old_messages = vec![
            (100u32, "imap:acc1:INBOX:100".to_string(), Some("<msg1@example.com>".to_string())),
        ];
        let new_mappings = vec![
            ("<nonexistent@example.com>".to_string(), 999u32),
        ];

        update_message_ids_after_move(&pool, "acc1", "Trash", &old_messages, &new_mappings)
            .await
            .unwrap();

        let ids: Vec<(String,)> = sqlx::query_as("SELECT id FROM messages WHERE thread_id = 't1'")
            .fetch_all(&pool).await.unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].0, "imap:acc1:INBOX:100");
    }
}
