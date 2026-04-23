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
        let uids = get_thread_message_uids(pool, thread_id).await?;
        if uids.is_empty() {
            return Ok(());
        }

        let folder = get_thread_folder(pool, thread_id).await?;
        let trash_folder = find_special_folder(pool, &self.config.account_id, "TRASH")
            .await
            .unwrap_or_else(|| "Trash".to_string());

        let mut session = connection::connect(&self.config).await?;
        operations::trash_messages(&mut session, &folder, &trash_folder, &uids).await?;
        let _ = session.logout().await;

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
        let uids = get_thread_message_uids(pool, thread_id).await?;
        if uids.is_empty() {
            return Ok(());
        }

        let folder = get_thread_folder(pool, thread_id).await?;
        let archive_folder = find_special_folder(pool, &self.config.account_id, "imap:Archive")
            .await
            .unwrap_or_else(|| "Archive".to_string());

        let mut session = connection::connect(&self.config).await?;
        operations::archive_messages(&mut session, &folder, &archive_folder, &uids).await?;
        let _ = session.logout().await;

        sqlx::query("DELETE FROM thread_labels WHERE thread_id = ? AND label_id = 'INBOX'")
            .bind(thread_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
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
