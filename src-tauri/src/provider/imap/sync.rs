use super::connection::ImapSession;
use super::threading::{group_into_threads, ParsedMessageHeaders};
use futures::StreamExt;

pub struct SyncResult {
    pub new_thread_count: u32,
    pub updated_thread_ids: Vec<String>,
}

pub async fn sync_folder(
    session: &mut ImapSession,
    pool: &sqlx::SqlitePool,
    account_id: &str,
    folder: &str,
) -> Result<SyncResult, String> {
    let mailbox = session
        .select(folder)
        .await
        .map_err(|e| format!("SELECT {} failed: {}", folder, e))?;

    let uid_validity = mailbox.uid_validity.unwrap_or(0);
    let exists = mailbox.exists;

    if exists == 0 {
        return Ok(SyncResult {
            new_thread_count: 0,
            updated_thread_ids: vec![],
        });
    }

    let state = get_sync_state(pool, account_id, folder).await;

    if state.is_none()
        || state.as_ref().map(|s| s.uid_validity) != Some(uid_validity as i64)
    {
        return full_folder_sync(session, pool, account_id, folder, uid_validity).await;
    }

    let stored = state.unwrap();
    incremental_sync(session, pool, account_id, folder, stored.highest_uid, uid_validity).await
}

async fn full_folder_sync(
    session: &mut ImapSession,
    pool: &sqlx::SqlitePool,
    account_id: &str,
    folder: &str,
    uid_validity: u32,
) -> Result<SyncResult, String> {
    tracing::info!("Full IMAP sync for {}/{}", account_id, folder);

    let fetch_range = "1:*";
    let fetch_stream = session
        .uid_fetch(fetch_range, "(UID FLAGS ENVELOPE RFC822.HEADER RFC822.SIZE)")
        .await
        .map_err(|e| format!("FETCH failed: {}", e))?;

    let fetched: Vec<_> = fetch_stream.filter_map(|r| async { r.ok() }).collect().await;

    let mut headers: Vec<ParsedMessageHeaders> = Vec::new();
    let mut highest_uid: u32 = 0;

    for msg in &fetched {
        let uid = msg.uid.unwrap_or(0);
        if uid > highest_uid {
            highest_uid = uid;
        }

        if let Some(header_bytes) = msg.header() {
            let parsed = mail_parser::MessageParser::default()
                .parse(header_bytes);

            if let Some(parsed) = parsed {
                let message_id = parsed.message_id().map(|s| s.to_string());
                let in_reply_to = parsed.in_reply_to().as_text().map(|s| s.to_string());
                let references = parsed.references().as_text_list()
                    .map(|list| list.join(" "));

                let sender = parsed.from().and_then(|a| {
                    a.first().map(|addr| {
                        match addr.name() {
                            Some(name) => format!("{} <{}>", name, addr.address().unwrap_or("")),
                            None => addr.address().unwrap_or("").to_string(),
                        }
                    })
                }).unwrap_or_default();

                let recipients = parsed.to().map(|a| {
                    a.iter().map(|addr| addr.address().unwrap_or("").to_string()).collect::<Vec<_>>().join(", ")
                }).unwrap_or_default();

                let subject = parsed.subject().unwrap_or("").to_string();
                let date = parsed.date().map(|d| d.to_timestamp()).unwrap_or(0);

                headers.push(ParsedMessageHeaders {
                    uid,
                    message_id,
                    in_reply_to,
                    references,
                    subject,
                    sender,
                    recipients,
                    date,
                });
            }
        }
    }

    let thread_groups = group_into_threads(account_id, &headers);
    let special_use = crate::provider::folder_mapping::detect_special_use_from_name(folder);
    let label_id = crate::provider::folder_mapping::imap_folder_to_label_id(
        folder,
        special_use.as_ref(),
    );

    let mut new_thread_ids = Vec::new();
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    for group in &thread_groups {
        let last_uid = group.message_uids.last().copied().unwrap_or(0);
        let latest_sender = headers.iter().find(|h| h.uid == last_uid)
            .map(|h| h.sender.clone()).unwrap_or_default();

        sqlx::query(
            "INSERT INTO threads (id, account_id, snippet, history_id, unread, sender, subject, latest_date, metadata_synced)
             VALUES (?, ?, '', '', 0, ?, ?, ?, 1)
             ON CONFLICT(id) DO UPDATE SET latest_date = excluded.latest_date, sender = excluded.sender, subject = excluded.subject",
        )
        .bind(&group.thread_id)
        .bind(account_id)
        .bind(&latest_sender)
        .bind(&group.subject)
        .bind(group.latest_date)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        sqlx::query(
            "INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, ?)",
        )
        .bind(&group.thread_id)
        .bind(&label_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        for &uid in &group.message_uids {
            if let Some(hdr) = headers.iter().find(|h| h.uid == uid) {
                let msg_id = format!("imap:{}:{}:{}", account_id, folder, uid);

                sqlx::query(
                    "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments, rfc_message_id)
                     VALUES (?, ?, ?, ?, ?, ?, '', ?, '', '', 0, ?)
                     ON CONFLICT(id) DO UPDATE SET rfc_message_id = COALESCE(excluded.rfc_message_id, messages.rfc_message_id)",
                )
                .bind(&msg_id)
                .bind(&group.thread_id)
                .bind(account_id)
                .bind(&hdr.sender)
                .bind(&hdr.recipients)
                .bind(&hdr.subject)
                .bind(hdr.date)
                .bind(hdr.message_id.as_deref())
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;

                sqlx::query(
                    "INSERT OR REPLACE INTO messages_fts(rowid, sender, subject, body_plain)
                     SELECT rowid, sender, subject, body_plain FROM messages WHERE id = ?",
                )
                .bind(&msg_id)
                .execute(&mut *tx)
                .await
                .ok();
            }
        }

        new_thread_ids.push(group.thread_id.clone());
    }

    tx.commit().await.map_err(|e| e.to_string())?;

    save_sync_state(pool, account_id, folder, uid_validity as i64, highest_uid as i64).await?;

    tracing::info!(
        "IMAP full sync complete: {} threads, {} messages for {}/{}",
        thread_groups.len(),
        headers.len(),
        account_id,
        folder,
    );

    Ok(SyncResult {
        new_thread_count: thread_groups.len() as u32,
        updated_thread_ids: new_thread_ids,
    })
}

async fn incremental_sync(
    session: &mut ImapSession,
    pool: &sqlx::SqlitePool,
    account_id: &str,
    folder: &str,
    last_uid: i64,
    uid_validity: u32,
) -> Result<SyncResult, String> {
    tracing::info!("Incremental IMAP sync for {}/{} from UID {}", account_id, folder, last_uid);

    let fetch_range = format!("{}:*", last_uid + 1);
    let fetch_stream = session
        .uid_fetch(&fetch_range, "(UID FLAGS ENVELOPE RFC822.HEADER)")
        .await
        .map_err(|e| format!("FETCH failed: {}", e))?;

    let fetched: Vec<_> = fetch_stream.filter_map(|r| async { r.ok() }).collect().await;

    if fetched.is_empty() {
        return Ok(SyncResult {
            new_thread_count: 0,
            updated_thread_ids: vec![],
        });
    }

    let mut headers: Vec<ParsedMessageHeaders> = Vec::new();
    let mut highest_uid = last_uid;

    for msg in fetched.iter() {
        let uid = msg.uid.unwrap_or(0);
        if uid as i64 <= last_uid {
            continue;
        }
        if uid as i64 > highest_uid {
            highest_uid = uid as i64;
        }

        if let Some(header_bytes) = msg.header() {
            let parsed = mail_parser::MessageParser::default().parse(header_bytes);

            if let Some(parsed) = parsed {
                let message_id = parsed.message_id().map(|s| s.to_string());
                let in_reply_to = parsed.in_reply_to().as_text().map(|s| s.to_string());
                let references = parsed.references().as_text_list().map(|list| list.join(" "));
                let sender = parsed.from().and_then(|a| {
                    a.first().map(|addr| {
                        match addr.name() {
                            Some(name) => format!("{} <{}>", name, addr.address().unwrap_or("")),
                            None => addr.address().unwrap_or("").to_string(),
                        }
                    })
                }).unwrap_or_default();
                let recipients = parsed.to().map(|a| {
                    a.iter().map(|addr| addr.address().unwrap_or("").to_string()).collect::<Vec<_>>().join(", ")
                }).unwrap_or_default();
                let subject = parsed.subject().unwrap_or("").to_string();
                let date = parsed.date().map(|d| d.to_timestamp()).unwrap_or(0);

                headers.push(ParsedMessageHeaders {
                    uid,
                    message_id,
                    in_reply_to,
                    references,
                    subject,
                    sender,
                    recipients,
                    date,
                });
            }
        }
    }

    let thread_groups = group_into_threads(account_id, &headers);
    let special_use = crate::provider::folder_mapping::detect_special_use_from_name(folder);
    let label_id = crate::provider::folder_mapping::imap_folder_to_label_id(folder, special_use.as_ref());

    let mut updated_ids = Vec::new();
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    for group in &thread_groups {
        let last_uid = group.message_uids.last().copied().unwrap_or(0);
        let latest_sender = headers.iter().find(|h| h.uid == last_uid)
            .map(|h| h.sender.clone()).unwrap_or_default();

        sqlx::query(
            "INSERT INTO threads (id, account_id, snippet, history_id, unread, sender, subject, latest_date, metadata_synced)
             VALUES (?, ?, '', '', 1, ?, ?, ?, 1)
             ON CONFLICT(id) DO UPDATE SET latest_date = MAX(threads.latest_date, excluded.latest_date), sender = excluded.sender",
        )
        .bind(&group.thread_id)
        .bind(account_id)
        .bind(&latest_sender)
        .bind(&group.subject)
        .bind(group.latest_date)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        sqlx::query("INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, ?)")
            .bind(&group.thread_id)
            .bind(&label_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        for &uid in &group.message_uids {
            if let Some(hdr) = headers.iter().find(|h| h.uid == uid) {
                let msg_id = format!("imap:{}:{}:{}", account_id, folder, uid);
                sqlx::query(
                    "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments, rfc_message_id)
                     VALUES (?, ?, ?, ?, ?, ?, '', ?, '', '', 0, ?)
                     ON CONFLICT(id) DO UPDATE SET rfc_message_id = COALESCE(excluded.rfc_message_id, messages.rfc_message_id)",
                )
                .bind(&msg_id)
                .bind(&group.thread_id)
                .bind(account_id)
                .bind(&hdr.sender)
                .bind(&hdr.recipients)
                .bind(&hdr.subject)
                .bind(hdr.date)
                .bind(hdr.message_id.as_deref())
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;

                sqlx::query(
                    "INSERT OR REPLACE INTO messages_fts(rowid, sender, subject, body_plain)
                     SELECT rowid, sender, subject, body_plain FROM messages WHERE id = ?",
                )
                .bind(&msg_id)
                .execute(&mut *tx)
                .await
                .ok();
            }
        }

        updated_ids.push(group.thread_id.clone());
    }

    tx.commit().await.map_err(|e| e.to_string())?;

    if highest_uid > last_uid {
        save_sync_state(pool, account_id, folder, uid_validity as i64, highest_uid).await?;
    }

    Ok(SyncResult {
        new_thread_count: thread_groups.len() as u32,
        updated_thread_ids: updated_ids,
    })
}

#[derive(sqlx::FromRow)]
struct SyncState {
    uid_validity: i64,
    highest_uid: i64,
}

async fn get_sync_state(pool: &sqlx::SqlitePool, account_id: &str, folder: &str) -> Option<SyncState> {
    sqlx::query_as::<_, SyncState>(
        "SELECT uid_validity, highest_uid FROM imap_sync_state WHERE account_id = ? AND folder = ?",
    )
    .bind(account_id)
    .bind(folder)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

async fn save_sync_state(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    folder: &str,
    uid_validity: i64,
    highest_uid: i64,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO imap_sync_state (account_id, folder, uid_validity, highest_uid)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(account_id, folder) DO UPDATE SET uid_validity = excluded.uid_validity, highest_uid = excluded.highest_uid",
    )
    .bind(account_id)
    .bind(folder)
    .bind(uid_validity)
    .bind(highest_uid)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}
