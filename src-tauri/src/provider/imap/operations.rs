use super::connection::{connect, ImapConfig, ImapSession};
use futures::StreamExt;

pub struct ImapMessageHeader {
    pub message_uid: u32,
    pub sender: String,
    pub subject: String,
    pub headers: Vec<(String, String)>,
    pub date: i64,
}

pub async fn imap_remote_scan(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    max_messages: i32,
) -> Result<Vec<ImapMessageHeader>, String> {
    let config = ImapConfig::from_db(pool, account_id).await?;
    let mut session = connect(&config).await?;

    session
        .select("INBOX")
        .await
        .map_err(|e| format!("SELECT INBOX failed: {}", e))?;

    let all_uids = session
        .uid_search("ALL")
        .await
        .map_err(|e| format!("UID SEARCH failed: {}", e))?;

    let mut uids: Vec<u32> = all_uids.into_iter().collect();
    uids.sort_unstable_by(|a, b| b.cmp(a));
    uids.truncate(max_messages as usize);

    if uids.is_empty() {
        let _ = session.logout().await;
        return Ok(vec![]);
    }

    let uid_set = uids
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let fetch_stream = session
        .uid_fetch(&uid_set, "(UID FLAGS ENVELOPE RFC822.HEADER RFC822.SIZE)")
        .await
        .map_err(|e| format!("UID FETCH failed: {}", e))?;

    let fetched: Vec<_> = fetch_stream.filter_map(|r| async { r.ok() }).collect().await;

    let mut results = Vec::new();
    for msg in &fetched {
        let uid = msg.uid.unwrap_or(0);
        if let Some(header_bytes) = msg.header() {
            if let Some(parsed) = mail_parser::MessageParser::default().parse(header_bytes) {
                let sender = parsed
                    .from()
                    .and_then(|a| {
                        a.first().map(|addr| match addr.name() {
                            Some(name) => {
                                format!("{} <{}>", name, addr.address().unwrap_or(""))
                            }
                            None => addr.address().unwrap_or("").to_string(),
                        })
                    })
                    .unwrap_or_default();

                let subject = parsed.subject().unwrap_or("").to_string();
                let date = parsed.date().map(|d| d.to_timestamp()).unwrap_or(0);

                let mut headers: Vec<(String, String)> = Vec::new();
                for h in parsed.headers() {
                    let key = h.name().to_string();
                    let val = h.value().as_text().unwrap_or("").to_string();
                    headers.push((key, val));
                }

                results.push(ImapMessageHeader {
                    message_uid: uid,
                    sender,
                    subject,
                    headers,
                    date,
                });
            }
        }
    }

    let _ = session.logout().await;
    Ok(results)
}

pub async fn set_flags(
    session: &mut ImapSession,
    folder: &str,
    uids: &[u32],
    flags: &str,
    add: bool,
) -> Result<(), String> {
    if uids.is_empty() {
        return Ok(());
    }

    session
        .select(folder)
        .await
        .map_err(|e| format!("SELECT {} failed: {}", folder, e))?;

    let uid_set = uids
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let query = if add {
        format!("+FLAGS ({})", flags)
    } else {
        format!("-FLAGS ({})", flags)
    };

    let store_stream = session
        .uid_store(&uid_set, &query)
        .await
        .map_err(|e| format!("STORE failed: {}", e))?;

    let _: Vec<_> = store_stream.filter_map(|r| async { r.ok() }).collect().await;

    Ok(())
}

pub async fn move_messages(
    session: &mut ImapSession,
    from_folder: &str,
    to_folder: &str,
    uids: &[u32],
    rfc_message_ids: &[Option<String>],
) -> Result<Vec<(String, u32)>, String> {
    if uids.is_empty() {
        return Ok(vec![]);
    }

    session
        .select(from_folder)
        .await
        .map_err(|e| format!("SELECT {} failed: {}", from_folder, e))?;

    let uid_set = uids
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    session
        .uid_copy(&uid_set, to_folder)
        .await
        .map_err(|e| format!("COPY to {} failed: {}", to_folder, e))?;

    let store_stream = session
        .uid_store(&uid_set, "+FLAGS (\\Deleted)")
        .await
        .map_err(|e| format!("STORE \\Deleted failed: {}", e))?;

    let _: Vec<_> = store_stream.filter_map(|r| async { r.ok() }).collect().await;

    let expunge_stream = session
        .expunge()
        .await
        .map_err(|e| format!("EXPUNGE failed: {}", e))?;

    let _: Vec<_> = expunge_stream.filter_map(|r| async { r.ok() }).collect().await;

    // Discover new UIDs in destination folder
    session
        .select(to_folder)
        .await
        .map_err(|e| format!("SELECT {} failed: {}", to_folder, e))?;

    let mut mappings = Vec::new();
    for msg_id in rfc_message_ids.iter().flatten() {
        let clean_id = msg_id.trim_matches('<').trim_matches('>');
        let query = format!("HEADER Message-Id \"<{}>\"", clean_id);
        let search_results = session
            .uid_search(&query)
            .await
            .map_err(|e| format!("SEARCH failed: {}", e))?;
        if let Some(&new_uid) = search_results.iter().max() {
            mappings.push((msg_id.clone(), new_uid));
        }
    }

    Ok(mappings)
}

pub async fn mark_read(
    session: &mut ImapSession,
    folder: &str,
    uids: &[u32],
    read: bool,
) -> Result<(), String> {
    set_flags(session, folder, uids, "\\Seen", read).await
}

pub async fn set_starred(
    session: &mut ImapSession,
    folder: &str,
    uids: &[u32],
    starred: bool,
) -> Result<(), String> {
    set_flags(session, folder, uids, "\\Flagged", starred).await
}

pub async fn trash_messages(
    session: &mut ImapSession,
    from_folder: &str,
    trash_folder: &str,
    uids: &[u32],
    rfc_message_ids: &[Option<String>],
) -> Result<Vec<(String, u32)>, String> {
    move_messages(session, from_folder, trash_folder, uids, rfc_message_ids).await
}

pub async fn archive_messages(
    session: &mut ImapSession,
    from_folder: &str,
    archive_folder: &str,
    uids: &[u32],
    rfc_message_ids: &[Option<String>],
) -> Result<Vec<(String, u32)>, String> {
    move_messages(session, from_folder, archive_folder, uids, rfc_message_ids).await
}
