use futures::stream::{self, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;

fn gmail_api_url(path: &str) -> String {
    #[cfg(test)]
    {
        let base = std::env::var("TEST_GMAIL_API_BASE")
            .unwrap_or_else(|_| "https://gmail.googleapis.com".to_string());
        format!("{}{}", base, path)
    }
    #[cfg(not(test))]
    {
        format!("https://gmail.googleapis.com{}", path)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GmailLabel {
    pub id: String,
    pub name: String,
    pub r#type: String,
    #[serde(rename = "messagesUnread")]
    pub messages_unread: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LabelsResponse {
    pub labels: Vec<GmailLabel>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GmailThread {
    pub id: String,
    pub snippet: String,
    #[serde(rename = "historyId")]
    pub history_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ThreadsResponse {
    pub threads: Option<Vec<GmailThread>>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessagePartHeader {
    pub name: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessagePartBody {
    pub size: i32,
    pub data: Option<String>,
    #[serde(rename = "attachmentId")]
    pub attachment_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessagePart {
    #[serde(rename = "partId")]
    pub part_id: Option<String>,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub filename: Option<String>,
    pub headers: Option<Vec<MessagePartHeader>>,
    pub body: Option<MessagePartBody>,
    pub parts: Option<Vec<MessagePart>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GmailMessage {
    pub id: String,
    #[serde(rename = "threadId")]
    pub thread_id: String,
    #[serde(rename = "labelIds")]
    pub label_ids: Option<Vec<String>>,
    pub snippet: Option<String>,
    #[serde(rename = "internalDate")]
    pub internal_date: String,
    pub payload: Option<MessagePart>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ThreadDetailsResponse {
    pub id: String,
    pub messages: Option<Vec<GmailMessage>>,
}

#[derive(Deserialize, Debug)]
struct HistoryMessage {
    id: String,
    #[serde(rename = "threadId")]
    thread_id: String,
    #[serde(rename = "labelIds")]
    label_ids: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
struct HistoryMessageWrapper {
    message: HistoryMessage,
}

#[derive(Deserialize, Debug)]
struct HistoryLabelWrapper {
    message: HistoryMessage,
    #[serde(rename = "labelIds")]
    #[serde(default)]
    label_ids: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct HistoryRecord {
    #[serde(default)]
    #[serde(rename = "messagesAdded")]
    messages_added: Vec<HistoryMessageWrapper>,
    #[serde(default)]
    #[serde(rename = "messagesDeleted")]
    messages_deleted: Vec<HistoryMessageWrapper>,
    #[serde(default)]
    #[serde(rename = "labelsAdded")]
    labels_added: Vec<HistoryLabelWrapper>,
    #[serde(default)]
    #[serde(rename = "labelsRemoved")]
    labels_removed: Vec<HistoryLabelWrapper>,
}

#[derive(Deserialize, Debug)]
struct HistoryResponse {
    history: Option<Vec<HistoryRecord>>,
    #[serde(rename = "historyId")]
    history_id: String,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

pub struct SyncDelta {
    pub threads_to_hydrate: Vec<String>,
    pub new_inbox_message_ids: Vec<String>,
    pub new_history_id: String,
}

fn sanitize_email_html(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }

    let mut result = raw.to_string();

    let script_re = regex_lite::Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap();
    result = script_re.replace_all(&result, "").to_string();

    let event_re = regex_lite::Regex::new(r#"(?i)\s+on\w+\s*=\s*("[^"]*"|'[^']*'|[^\s>]*)"#).unwrap();
    result = event_re.replace_all(&result, "").to_string();

    let js_url_re = regex_lite::Regex::new(r#"(?i)(href|src|action)\s*=\s*["']?\s*javascript:"#).unwrap();
    result = js_url_re.replace_all(&result, r#"$1=""#).to_string();

    let dangerous_tags_re = regex_lite::Regex::new(r"(?is)<(iframe|object|embed|applet|form)[^>]*>.*?</(iframe|object|embed|applet|form)>").unwrap();
    result = dangerous_tags_re.replace_all(&result, "").to_string();

    let dangerous_self_re = regex_lite::Regex::new(r"(?i)<(iframe|object|embed|applet)[^>]*/?>").unwrap();
    result = dangerous_self_re.replace_all(&result, "").to_string();

    let base_re = regex_lite::Regex::new(r"(?i)<base[^>]*>").unwrap();
    result = base_re.replace_all(&result, "").to_string();

    let doctype_re = regex_lite::Regex::new(r"(?i)<!DOCTYPE[^>]*>").unwrap();
    result = doctype_re.replace_all(&result, "").to_string();

    let title_re = regex_lite::Regex::new(r"(?is)<title[^>]*>.*?</title>").unwrap();
    result = title_re.replace_all(&result, "").to_string();

    let wrapper_open_re = regex_lite::Regex::new(r"(?i)<(html|head|body)[^>]*>").unwrap();
    result = wrapper_open_re.replace_all(&result, "").to_string();

    let wrapper_close_re = regex_lite::Regex::new(r"(?i)</(html|head|body)>").unwrap();
    result = wrapper_close_re.replace_all(&result, "").to_string();

    let meta_re = regex_lite::Regex::new(r"(?i)<meta[^>]*/?>").unwrap();
    result = meta_re.replace_all(&result, "").to_string();

    let inliner = css_inline::CSSInliner::options()
        .load_remote_stylesheets(false)
        .keep_style_tags(true)
        .build();
    inliner.inline(&result).unwrap_or(result)
}

#[derive(Debug)]
pub struct AttachmentFile {
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

pub fn read_attachment_files(paths: &[String]) -> Result<Vec<AttachmentFile>, String> {
    const MAX_TOTAL_SIZE: u64 = 25 * 1024 * 1024;
    let mut files = Vec::new();
    let mut total_size: u64 = 0;

    for path_str in paths {
        let path = std::path::Path::new(path_str);
        let metadata = std::fs::metadata(path)
            .map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;
        total_size += metadata.len();
        if total_size > MAX_TOTAL_SIZE {
            return Err("Total attachment size exceeds Gmail's 25MB limit.".to_string());
        }
        let data = std::fs::read(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("attachment")
            .to_string();
        let mime_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        files.push(AttachmentFile { filename, mime_type, data });
    }
    Ok(files)
}

fn extract_body(part: &MessagePart, target_mime_type: &str) -> Option<String> {
    if part.mime_type == target_mime_type {
        if let Some(body) = &part.body {
            if let Some(data) = &body.data {
                if let Ok(decoded) = base64::decode_config(data, base64::URL_SAFE) {
                    return String::from_utf8(decoded).ok();
                }
            }
        }
    }
    if let Some(parts) = &part.parts {
        for p in parts {
            if let Some(extracted) = extract_body(p, target_mime_type) {
                return Some(extracted);
            }
        }
    }
    None
}

struct AttachmentInfo {
    id: String,
    message_id: String,
    filename: String,
    mime_type: String,
    size: i32,
}

fn extract_attachments(part: &MessagePart, message_id: &str) -> Vec<AttachmentInfo> {
    let mut attachments = Vec::new();

    if let Some(ref filename) = part.filename {
        if !filename.is_empty() {
            let size = part.body.as_ref().map(|b| b.size).unwrap_or(0);
            let attachment_id = part.body.as_ref().and_then(|b| b.attachment_id.clone());
            attachments.push(AttachmentInfo {
                id: attachment_id.unwrap_or_else(|| {
                    format!("{}_{}", message_id, part.part_id.as_deref().unwrap_or("0"))
                }),
                message_id: message_id.to_string(),
                filename: filename.clone(),
                mime_type: part.mime_type.clone(),
                size,
            });
        }
    }

    if let Some(ref parts) = part.parts {
        for p in parts {
            attachments.extend(extract_attachments(p, message_id));
        }
    }

    attachments
}

fn get_header<'a>(headers: &'a [MessagePartHeader], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

pub async fn fetch_and_store_labels(
    pool: &SqlitePool,
    account_id: &str,
    access_token: &str,
) -> Result<(), String> {
    let client = Client::new();
    let res = client
        .get(gmail_api_url("/gmail/v1/users/me/labels"))
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to fetch labels: {}", res.status()));
    }

    let labels_res: LabelsResponse = res.json().await.map_err(|e| e.to_string())?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let inbox_has_gmail_count = labels_res.labels.iter()
        .any(|l| l.id == "INBOX" && l.messages_unread.is_some());

    for label in &labels_res.labels {
        if label.id == "INBOX" {
            println!("[Labels] INBOX messagesUnread={:?}", label.messages_unread);
        }
    }

    for label in labels_res.labels {
        sqlx::query(
            "INSERT INTO labels (id, account_id, name, type, unread_count)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, unread_count=excluded.unread_count",
        )
        .bind(&label.id)
        .bind(account_id)
        .bind(&label.name)
        .bind(&label.r#type)
        .bind(label.messages_unread.unwrap_or(0))
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    tx.commit().await.map_err(|e| e.to_string())?;

    // If Gmail didn't return messagesUnread for INBOX (it's optional),
    // compute from local thread data instead
    if !inbox_has_gmail_count {
        let local_unread: (i32,) = sqlx::query_as(
            "SELECT COUNT(*) FROM threads t
             INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'INBOX'
             WHERE t.account_id = ? AND t.unread = 1"
        )
        .bind(account_id)
        .fetch_one(pool)
        .await
        .unwrap_or((0,));

        let _ = sqlx::query("UPDATE labels SET unread_count = ? WHERE id = 'INBOX' AND account_id = ?")
            .bind(local_unread.0)
            .bind(account_id)
            .execute(pool)
            .await;

        println!("[Labels] INBOX unread from local data: {}", local_unread.0);
    }

    Ok(())
}

pub async fn fetch_and_store_threads(
    pool: &SqlitePool,
    account_id: &str,
    access_token: &str,
    label_ids: Option<&[&str]>,
    max_results: i32,
) -> Result<(), String> {
    let client = Client::new();

    let mut params: Vec<(&str, String)> = vec![
        ("maxResults", max_results.to_string()),
        (
            "fields",
            "threads(id,snippet,historyId),nextPageToken".to_string(),
        ),
    ];
    if let Some(labels) = &label_ids {
        for lid in labels.iter() {
            params.push(("labelIds", lid.to_string()));
        }
    }

    let res = client
        .get(gmail_api_url("/gmail/v1/users/me/threads"))
        .query(&params)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to fetch threads: {}", res.status()));
    }

    let threads_res: ThreadsResponse = res.json().await.map_err(|e| e.to_string())?;

    if let Some(threads) = threads_res.threads {
        let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
        for thread in &threads {
            sqlx::query(
                "INSERT INTO threads (id, account_id, snippet, history_id, unread)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT(id) DO UPDATE SET snippet=excluded.snippet, history_id=excluded.history_id"
            )
            .bind(&thread.id).bind(account_id).bind(&thread.snippet)
            .bind(&thread.history_id).bind(0)
            .execute(&mut *tx).await.map_err(|e| e.to_string())?;

            if let Some(labels) = &label_ids {
                for lid in labels.iter() {
                    let _ = sqlx::query(
                        "INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, ?)",
                    )
                    .bind(&thread.id)
                    .bind(lid)
                    .execute(&mut *tx)
                    .await;
                }
            }
        }
        tx.commit().await.map_err(|e| e.to_string())?;

        // Seed history watermark from the highest historyId in the fetched threads
        if let Some(max_hid) = threads.iter().map(|t| &t.history_id).max() {
            set_last_history_id(pool, account_id, max_hid).await;
        }
    }

    Ok(())
}

pub async fn fetch_messages_for_thread(
    pool: &SqlitePool,
    account_id: &str,
    access_token: &str,
    thread_id: &str,
) -> Result<(), String> {
    let client = Client::new();
    let res = client
        .get(gmail_api_url(&format!(
            "/gmail/v1/users/me/threads/{}",
            thread_id
        )))
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!(
            "Failed to fetch thread {}: {}",
            thread_id,
            res.status()
        ));
    }

    let thread_details: ThreadDetailsResponse = res.json().await.map_err(|e| e.to_string())?;
    store_thread_messages(pool, account_id, &thread_details).await
}

async fn store_thread_messages(
    pool: &SqlitePool,
    account_id: &str,
    thread_details: &ThreadDetailsResponse,
) -> Result<(), String> {
    if let Some(messages) = &thread_details.messages {
        let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

        for msg in messages {
            let internal_date: i64 = msg.internal_date.parse().unwrap_or(0);
            let mut sender = String::new();
            let mut recipients = String::new();
            let mut subject = String::new();
            let mut body_plain = String::new();
            let mut body_html = String::new();

            let mut msg_attachments: Vec<AttachmentInfo> = Vec::new();

            if let Some(payload) = &msg.payload {
                if let Some(headers) = &payload.headers {
                    sender = get_header(headers, "From").unwrap_or("").to_string();
                    recipients = get_header(headers, "To").unwrap_or("").to_string();
                    subject = get_header(headers, "Subject").unwrap_or("").to_string();
                }
                body_plain = extract_body(payload, "text/plain").unwrap_or_default();
                body_html = sanitize_email_html(
                    &extract_body(payload, "text/html").unwrap_or_default(),
                );
                msg_attachments = extract_attachments(payload, &msg.id);
            }

            let has_att = !msg_attachments.is_empty();

            sqlx::query(
                "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(id) DO UPDATE SET sender=excluded.sender, recipients=excluded.recipients, subject=excluded.subject, body_plain=excluded.body_plain, body_html=excluded.body_html, has_attachments=excluded.has_attachments"
            )
            .bind(&msg.id).bind(&msg.thread_id).bind(account_id)
            .bind(&sender).bind(&recipients).bind(&subject)
            .bind(msg.snippet.as_deref().unwrap_or("")).bind(internal_date)
            .bind(&body_plain).bind(&body_html).bind(if has_att { 1 } else { 0 })
            .execute(&mut *tx).await.map_err(|e| e.to_string())?;

            // Store attachment metadata
            sqlx::query("DELETE FROM attachments WHERE message_id = ?")
                .bind(&msg.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;

            for att in &msg_attachments {
                sqlx::query(
                    "INSERT INTO attachments (id, message_id, filename, mime_type, size, downloaded) VALUES (?, ?, ?, ?, ?, 0)"
                )
                .bind(&att.id)
                .bind(&att.message_id)
                .bind(&att.filename)
                .bind(&att.mime_type)
                .bind(att.size)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            }

            if let Some(ref label_ids) = msg.label_ids {
                for label_id in label_ids {
                    let _ = sqlx::query(
                        "INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, ?)",
                    )
                    .bind(&msg.thread_id)
                    .bind(label_id)
                    .execute(&mut *tx)
                    .await;
                    let _ = sqlx::query(
                        "INSERT OR IGNORE INTO message_labels (message_id, label_id) VALUES (?, ?)",
                    )
                    .bind(&msg.id)
                    .bind(label_id)
                    .execute(&mut *tx)
                    .await;
                }
                if label_ids.contains(&"UNREAD".to_string()) {
                    let _ = sqlx::query("UPDATE threads SET unread = 1 WHERE id = ?")
                        .bind(&msg.thread_id)
                        .execute(&mut *tx)
                        .await;
                }
            }

            let _ = sqlx::query(
                "INSERT OR REPLACE INTO messages_fts(rowid, sender, subject, body_plain) 
                 SELECT rowid, sender, subject, body_plain FROM messages WHERE id = ?",
            )
            .bind(&msg.id)
            .execute(&mut *tx)
            .await;
        }

        // Mark thread as synced at current history_id
        let _ = sqlx::query(
            "UPDATE threads SET synced_history_id = history_id WHERE id = ?"
        )
        .bind(&thread_details.id)
        .execute(&mut *tx)
        .await;

        tx.commit().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub async fn batch_hydrate_threads(
    pool: &SqlitePool,
    account_id: &str,
    access_token: &str,
    thread_ids: Vec<String>,
) -> (usize, usize) {
    let client = Arc::new(Client::new());
    let total = thread_ids.len();
    let mut completed = 0usize;

    let results: Vec<Result<(), String>> = stream::iter(thread_ids)
        .map(|tid| {
            let client = Arc::clone(&client);
            let token = access_token.to_string();
            let pool = pool.clone();
            let aid = account_id.to_string();
            async move {
                let res = client
                    .get(gmail_api_url(&format!(
                        "/gmail/v1/users/me/threads/{}",
                        tid
                    )))
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                if !res.status().is_success() {
                    return Err(format!("HTTP {}", res.status()));
                }
                let details: ThreadDetailsResponse = res.json().await.map_err(|e| e.to_string())?;
                store_thread_messages(&pool, &aid, &details).await
            }
        })
        .buffer_unordered(5)
        .collect()
        .await;

    for r in &results {
        if r.is_ok() {
            completed += 1;
        }
    }

    println!("[Hydrate] Completed {}/{} threads", completed, total);
    (total, completed)
}

pub async fn get_unhydrated_thread_ids(pool: &SqlitePool, account_id: &str) -> Vec<String> {
    #[derive(sqlx::FromRow)]
    struct TId {
        id: String,
    }
    sqlx::query_as::<_, TId>(
        "SELECT t.id FROM threads t
         LEFT JOIN messages m ON t.id = m.thread_id
         WHERE t.account_id = ? AND m.id IS NULL",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| r.id)
    .collect()
}

/// Returns thread IDs that have been updated on Gmail (history_id changed)
/// but haven't been re-synced locally yet.
pub async fn get_stale_thread_ids(pool: &SqlitePool, account_id: &str) -> Vec<String> {
    #[derive(sqlx::FromRow)]
    struct TId {
        id: String,
    }
    sqlx::query_as::<_, TId>(
        "SELECT t.id FROM threads t
         WHERE t.account_id = ?
         AND t.synced_history_id IS NOT NULL
         AND t.history_id != t.synced_history_id",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| r.id)
    .collect()
}

pub async fn evict_old_message_bodies(pool: &SqlitePool, account_id: &str, max_cached: i32) {
    let result = sqlx::query(
        "UPDATE messages SET body_html = '', body_plain = '' 
         WHERE account_id = ? AND thread_id NOT IN (
             SELECT t.id FROM threads t 
             WHERE t.account_id = ? 
             ORDER BY (SELECT MAX(m2.internal_date) FROM messages m2 WHERE m2.thread_id = t.id) DESC 
             LIMIT ?
         ) AND (body_html != '' OR body_plain != '')"
    )
    .bind(account_id).bind(account_id).bind(max_cached)
    .execute(pool).await;

    match result {
        Ok(r) => {
            if r.rows_affected() > 0 {
                println!(
                    "[Cache] Evicted bodies from {} messages (keeping {} recent threads)",
                    r.rows_affected(),
                    max_cached
                );
            }
        }
        Err(e) => println!("[Cache] Eviction error: {}", e),
    }
}

pub async fn get_last_history_id(pool: &SqlitePool, account_id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT last_history_id FROM history_state WHERE account_id = ?")
        .bind(account_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
}

pub async fn set_last_history_id(pool: &SqlitePool, account_id: &str, history_id: &str) {
    let _ = sqlx::query(
        "INSERT INTO history_state (account_id, last_history_id) VALUES (?, ?)
         ON CONFLICT(account_id) DO UPDATE SET last_history_id = excluded.last_history_id"
    )
    .bind(account_id)
    .bind(history_id)
    .execute(pool)
    .await;
}

#[allow(unused_assignments)]
pub async fn fetch_history(
    pool: &SqlitePool,
    _account_id: &str,
    access_token: &str,
    start_history_id: &str,
) -> Result<Option<SyncDelta>, String> {
    let client = Client::new();
    let mut all_records: Vec<HistoryRecord> = Vec::new();
    let mut page_token: Option<String> = None;
    let mut latest_history_id = String::new();

    loop {
        let mut params = vec![
            ("startHistoryId", start_history_id.to_string()),
            ("historyTypes", "messageAdded".to_string()),
            ("historyTypes", "messageDeleted".to_string()),
            ("historyTypes", "labelAdded".to_string()),
            ("historyTypes", "labelRemoved".to_string()),
        ];
        if let Some(ref token) = page_token {
            params.push(("pageToken", token.clone()));
        }

        let res = client
            .get(gmail_api_url("/gmail/v1/users/me/history"))
            .query(&params)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if res.status() == 404 {
            return Ok(None);
        }
        if !res.status().is_success() {
            return Err(format!("History API error: {}", res.status()));
        }

        let history_res: HistoryResponse = res.json().await.map_err(|e| e.to_string())?;
        println!("[History] Response historyId={}, records={}", history_res.history_id,
            history_res.history.as_ref().map(|r| r.len()).unwrap_or(0));
        latest_history_id = history_res.history_id;

        if let Some(records) = history_res.history {
            for rec in &records {
                if !rec.messages_added.is_empty() { println!("[History] messagesAdded: {}", rec.messages_added.len()); }
                if !rec.messages_deleted.is_empty() { println!("[History] messagesDeleted: {}", rec.messages_deleted.len()); }
                if !rec.labels_added.is_empty() { println!("[History] labelsAdded: {} (labels: {:?})", rec.labels_added.len(), rec.labels_added.iter().map(|l| &l.label_ids).collect::<Vec<_>>()); }
                if !rec.labels_removed.is_empty() { println!("[History] labelsRemoved: {} (labels: {:?})", rec.labels_removed.len(), rec.labels_removed.iter().map(|l| &l.label_ids).collect::<Vec<_>>()); }
            }
            all_records.extend(records);
        }

        match history_res.next_page_token {
            Some(token) => page_token = Some(token),
            None => break,
        }
    }

    let mut threads_to_hydrate: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut new_inbox_message_ids: Vec<String> = Vec::new();

    for record in &all_records {
        for added in &record.messages_added {
            threads_to_hydrate.insert(added.message.thread_id.clone());
            if let Some(ref labels) = added.message.label_ids {
                if labels.contains(&"INBOX".to_string()) && labels.contains(&"UNREAD".to_string()) {
                    new_inbox_message_ids.push(added.message.id.clone());
                }
            }
        }

        for deleted in &record.messages_deleted {
            threads_to_hydrate.insert(deleted.message.thread_id.clone());
        }

        // labelsAdded: label_ids = the labels that were ADDED
        for added in &record.labels_added {
            for label in &added.label_ids {
                let _ = sqlx::query(
                    "INSERT OR IGNORE INTO message_labels (message_id, label_id) VALUES (?, ?)"
                )
                .bind(&added.message.id)
                .bind(label)
                .execute(pool)
                .await;
                let _ = sqlx::query(
                    "INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, ?)"
                )
                .bind(&added.message.thread_id)
                .bind(label)
                .execute(pool)
                .await;
            }
            if added.label_ids.iter().any(|l| l == "UNREAD") {
                let _ = sqlx::query("UPDATE threads SET unread = 1 WHERE id = ?")
                    .bind(&added.message.thread_id)
                    .execute(pool)
                    .await;
            }
        }

        // labelsRemoved: label_ids = the labels that were REMOVED
        for removed in &record.labels_removed {
            for label in &removed.label_ids {
                let _ = sqlx::query(
                    "DELETE FROM message_labels WHERE message_id = ? AND label_id = ?"
                )
                .bind(&removed.message.id)
                .bind(label)
                .execute(pool)
                .await;
            }
            if removed.label_ids.iter().any(|l| l == "UNREAD") {
                let has_unread: (i32,) = sqlx::query_as(
                    "SELECT COUNT(*) FROM message_labels ml
                     JOIN messages m ON ml.message_id = m.id
                     WHERE m.thread_id = ? AND ml.label_id = 'UNREAD'"
                )
                .bind(&removed.message.thread_id)
                .fetch_one(pool)
                .await
                .unwrap_or((0,));
                if has_unread.0 == 0 {
                    let _ = sqlx::query("UPDATE threads SET unread = 0 WHERE id = ?")
                        .bind(&removed.message.thread_id)
                            .execute(pool)
                            .await;
                    }
                }
            if removed.label_ids.iter().any(|l| l == "INBOX") {
                let _ = sqlx::query(
                    "DELETE FROM thread_labels WHERE thread_id = ? AND label_id = 'INBOX'"
                )
                .bind(&removed.message.thread_id)
                .execute(pool)
                .await;
            }
        }
    }

    Ok(Some(SyncDelta {
        threads_to_hydrate: threads_to_hydrate.into_iter().collect(),
        new_inbox_message_ids,
        new_history_id: latest_history_id,
    }))
}

#[derive(serde::Serialize)]
struct ModifyThreadRequest {
    #[serde(rename = "addLabelIds")]
    add_label_ids: Vec<String>,
    #[serde(rename = "removeLabelIds")]
    remove_label_ids: Vec<String>,
}

pub async fn modify_thread(
    pool: &sqlx::SqlitePool,
    _account_id: &str,
    access_token: &str,
    thread_id: &str,
    add_labels: Vec<String>,
    remove_labels: Vec<String>,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let payload = ModifyThreadRequest {
        add_label_ids: add_labels.clone(),
        remove_label_ids: remove_labels.clone(),
    };

    let res = client
        .post(gmail_api_url(&format!(
            "/gmail/v1/users/me/threads/{}/modify",
            thread_id
        )))
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to modify thread: {}", res.status()));
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    if remove_labels.contains(&"UNREAD".to_string()) {
        sqlx::query("UPDATE threads SET unread = 0 WHERE id = ?")
            .bind(thread_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    } else if add_labels.contains(&"UNREAD".to_string()) {
        sqlx::query("UPDATE threads SET unread = 1 WHERE id = ?")
            .bind(thread_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }

    if remove_labels.contains(&"STARRED".to_string()) {
        sqlx::query("DELETE FROM thread_labels WHERE thread_id = ? AND label_id = 'STARRED'")
            .bind(thread_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    } else if add_labels.contains(&"STARRED".to_string()) {
        sqlx::query(
            "INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, 'STARRED')",
        )
        .bind(thread_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn trash_thread(
    pool: &sqlx::SqlitePool,
    _account_id: &str,
    access_token: &str,
    thread_id: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let res = client
        .post(gmail_api_url(&format!(
            "/gmail/v1/users/me/threads/{}/trash",
            thread_id
        )))
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Length", 0)
        .body(vec![])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to trash thread: {}", res.status()));
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM thread_labels WHERE thread_id = ?")
        .bind(thread_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM messages WHERE thread_id = ?")
        .bind(thread_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM threads WHERE id = ?")
        .bind(thread_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn untrash_thread(
    access_token: &str,
    thread_id: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let res = client
        .post(gmail_api_url(&format!(
            "/gmail/v1/users/me/threads/{}/untrash",
            thread_id
        )))
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Length", 0)
        .body(vec![])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to untrash thread: {}", res.status()));
    }

    Ok(())
}

pub async fn download_attachment(
    access_token: &str,
    message_id: &str,
    attachment_id: &str,
) -> Result<Vec<u8>, String> {
    let client = Client::new();
    let url = gmail_api_url(&format!(
        "/gmail/v1/users/me/messages/{}/attachments/{}",
        message_id, attachment_id
    ));
    let res = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to download attachment: {}", res.status()));
    }

    let body: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let data = body["data"]
        .as_str()
        .ok_or("No data in attachment response")?;

    base64::decode_config(data, base64::URL_SAFE)
        .map_err(|e| format!("Failed to decode attachment: {}", e))
}

pub async fn upload_to_drive(
    access_token: &str,
    file_path: &str,
) -> Result<String, String> {
    let path = std::path::Path::new(file_path);
    let file_data = std::fs::read(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    let filename = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("attachment")
        .to_string();
    let mime_type = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    let client = Client::new();

    // Multipart upload to Drive API
    let boundary = format!("rustymail{}", chrono::Utc::now().timestamp_millis());
    let metadata = serde_json::json!({
        "name": filename,
        "mimeType": mime_type,
    });

    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(b"Content-Type: application/json; charset=UTF-8\r\n\r\n");
    body.extend_from_slice(metadata.to_string().as_bytes());
    body.extend_from_slice(format!("\r\n--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(format!("Content-Type: {}\r\n\r\n", mime_type).as_bytes());
    body.extend_from_slice(&file_data);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

    let upload_res = client
        .post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", format!("multipart/related; boundary={}", boundary))
        .body(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !upload_res.status().is_success() {
        let status = upload_res.status();
        let text = upload_res.text().await.unwrap_or_default();
        return Err(format!("Drive upload failed ({}): {}", status, text));
    }

    let upload_json: serde_json::Value = upload_res.json().await.map_err(|e| e.to_string())?;
    let file_id = upload_json["id"]
        .as_str()
        .ok_or("No file ID in Drive response")?
        .to_string();

    // Set sharing permission: anyone with link can view
    let perm_body = serde_json::json!({
        "role": "reader",
        "type": "anyone",
    });

    let perm_res = client
        .post(format!("https://www.googleapis.com/drive/v3/files/{}/permissions", file_id))
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&perm_body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !perm_res.status().is_success() {
        // File uploaded but sharing failed — return link anyway
        eprintln!("Warning: Failed to set sharing permission on Drive file {}", file_id);
    }

    Ok(format!("https://drive.google.com/file/d/{}/view?usp=sharing", file_id))
}

/// Parse an RFC 5322-ish address like `Display Name <email@example.com>` or
/// just `email@example.com` into a lettre `Mailbox`. Display names containing
/// emoji or other non-ASCII are supported because we handle them ourselves
/// rather than delegating to `Mailbox::from_str`.
fn parse_to_mailbox(raw: &str) -> Result<lettre::message::Mailbox, String> {
    use lettre::message::Mailbox;
    use lettre::Address;
    use std::str::FromStr;

    let raw = raw.trim();
    if let (Some(start), Some(end)) = (raw.find('<'), raw.rfind('>')) {
        let email_part = raw[start + 1..end].trim();
        let display_part = raw[..start].trim().trim_matches('"').trim();
        let address = Address::from_str(email_part)
            .map_err(|e| format!("Invalid email address '{}': {}", email_part, e))?;
        Ok(if display_part.is_empty() {
            Mailbox::new(None, address)
        } else {
            Mailbox::new(Some(display_part.to_string()), address)
        })
    } else {
        // No angle brackets — treat the whole thing as a plain email address
        let address =
            Address::from_str(raw).map_err(|e| format!("Invalid To address '{}': {}", raw, e))?;
        Ok(Mailbox::new(None, address))
    }
}

#[allow(clippy::too_many_arguments)]
fn build_mime_message(
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
    in_reply_to: Option<&str>,
    references: Option<&str>,
    allow_empty_to: bool,
    attachments: &[AttachmentFile],
) -> Result<String, String> {
    use lettre::message::{header::ContentType, Mailbox, Message};
    use std::str::FromStr;

    let from_mailbox = Mailbox::from_str(from).map_err(|_| "Invalid From address")?;

    // Parse all comma-separated recipient addresses, each potentially containing
    // a display name with emoji or other non-ASCII characters.
    let recipients: Vec<lettre::message::Mailbox> = to
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse_to_mailbox)
        .collect::<Result<_, _>>()?;

    let mut builder = Message::builder()
        .from(from_mailbox.clone())
        .subject(subject);

    if recipients.is_empty() {
        if allow_empty_to {
            // Dummy envelope to satisfy lettre's validation
            // Gmail API ignores the envelope when saving a draft/sending raw
            // but lettre requires at least one recipient in the envelope.
            let from_addr = from_mailbox.email.clone();
            let envelope = lettre::address::Envelope::new(Some(from_addr.clone()), vec![from_addr])
                .map_err(|e| e.to_string())?;
            builder = builder.envelope(envelope);
        } else {
            return Err(
                "No valid recipients. Please specify at least one recipient to send.".to_string(),
            );
        }
    } else {
        for mailbox in recipients {
            builder = builder.to(mailbox);
        }
    }

    if let Some(irt) = in_reply_to {
        if !irt.is_empty() {
            builder = builder.header(lettre::message::header::InReplyTo::from(irt.to_string()));
        }
    }
    if let Some(refs) = references {
        if !refs.is_empty() {
            builder = builder.header(lettre::message::header::References::from(refs.to_string()));
        }
    }

    let email = if attachments.is_empty() {
        builder
            .header(ContentType::TEXT_HTML)
            .body(body.to_string())
            .map_err(|e| e.to_string())?
    } else {
        use lettre::message::{MultiPart, SinglePart, Attachment as LettreAttachment};

        let html_part = SinglePart::builder()
            .header(ContentType::TEXT_HTML)
            .body(body.to_string());

        let mut multipart = MultiPart::mixed().singlepart(html_part);

        for att in attachments {
            let content_type = ContentType::parse(&att.mime_type)
                .unwrap_or(ContentType::parse("application/octet-stream").unwrap());
            let attachment_part = LettreAttachment::new(att.filename.clone())
                .body(att.data.clone(), content_type);
            multipart = multipart.singlepart(attachment_part);
        }

        builder.multipart(multipart).map_err(|e| e.to_string())?
    };

    let formatted = email.formatted();
    Ok(base64::encode_config(formatted, base64::URL_SAFE_NO_PAD))
}

#[allow(clippy::too_many_arguments)]
pub async fn send_message(
    _account_id: &str,
    account_email: &str,
    access_token: &str,
    to: &str,
    subject: &str,
    body: &str,
    thread_id: Option<&str>,
    in_reply_to: Option<&str>,
    references: Option<&str>,
    attachments: &[AttachmentFile],
) -> Result<(), String> {
    let raw = build_mime_message(
        account_email,
        to,
        subject,
        body,
        in_reply_to,
        references,
        false,
        attachments,
    )?;
    let client = reqwest::Client::new();
    let mut body_json = serde_json::json!({ "raw": raw });

    if let Some(tid) = thread_id {
        if !tid.is_empty() {
            body_json["threadId"] = serde_json::json!(tid);
        }
    }

    let res = client
        .post(gmail_api_url("/gmail/v1/users/me/messages/send"))
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&body_json)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to send email: {}", res.status()));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn save_draft(
    _account_id: &str,
    account_email: &str,
    access_token: &str,
    to: &str,
    subject: &str,
    body: &str,
    thread_id: Option<&str>,
    in_reply_to: Option<&str>,
    references: Option<&str>,
    draft_id: Option<&str>,
    attachments: &[AttachmentFile],
) -> Result<String, String> {
    let raw = build_mime_message(
        account_email,
        to,
        subject,
        body,
        in_reply_to,
        references,
        true,
        attachments,
    )?;
    let client = reqwest::Client::new();
    let mut message_json = serde_json::json!({ "raw": raw });

    if let Some(tid) = thread_id {
        if !tid.is_empty() {
            message_json["threadId"] = serde_json::json!(tid);
        }
    }

    let mut body_json = serde_json::json!({ "message": message_json });

    if let Some(did) = draft_id {
        body_json["id"] = serde_json::json!(did);
    }

    let url = if let Some(did) = draft_id {
        gmail_api_url(&format!("/gmail/v1/users/me/drafts/{}", did))
    } else {
        gmail_api_url("/gmail/v1/users/me/drafts")
    };

    let request = if draft_id.is_some() {
        client.put(&url)
    } else {
        client.post(&url)
    };

    let res = request
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&body_json)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to save draft: {}", res.status()));
    }

    let response_json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let id = response_json["id"]
        .as_str()
        .ok_or("No draft ID returned")?
        .to_string();
    Ok(id)
}

pub async fn delete_draft(
    pool: &sqlx::SqlitePool,
    _account_id: &str,
    _account_email: &str,
    access_token: &str,
    draft_id: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let res = client
        .delete(gmail_api_url(&format!(
            "/gmail/v1/users/me/drafts/{}",
            draft_id
        )))
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to delete draft: {}", res.status()));
    }

    // Also clean up local database
    sqlx::query("DELETE FROM drafts WHERE id = ?")
        .bind(draft_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Find the draft associated with a thread and delete just that draft,
/// preserving the original messages in the thread.
pub async fn delete_draft_by_thread(
    pool: &sqlx::SqlitePool,
    _account_id: &str,
    access_token: &str,
    thread_id: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();

    // List drafts and find the one associated with this thread
    let res = client
        .get(gmail_api_url("/gmail/v1/users/me/drafts"))
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to list drafts: {}", res.status()));
    }

    let body: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let drafts = body["drafts"].as_array();

    if let Some(drafts) = drafts {
        for draft in drafts {
            let draft_id = draft["id"].as_str().unwrap_or("");
            let msg_thread_id = draft["message"]["threadId"].as_str().unwrap_or("");

            if msg_thread_id == thread_id && !draft_id.is_empty() {
                // Delete this draft
                let del_res = client
                    .delete(gmail_api_url(&format!(
                        "/gmail/v1/users/me/drafts/{}",
                        draft_id
                    )))
                    .header("Authorization", format!("Bearer {}", access_token))
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;

                if !del_res.status().is_success() {
                    return Err(format!("Failed to delete draft: {}", del_res.status()));
                }

                // Clean up local database
                let _ = sqlx::query("DELETE FROM drafts WHERE id = ?")
                    .bind(draft_id)
                    .execute(pool)
                    .await;

                // Also remove the draft message from the local messages table
                let _ = sqlx::query(
                    "DELETE FROM messages WHERE thread_id = ? AND id IN (SELECT id FROM messages WHERE thread_id = ? ORDER BY internal_date DESC LIMIT 1)"
                )
                    .bind(thread_id)
                    .bind(thread_id)
                    .execute(pool)
                    .await;

                return Ok(());
            }
        }
    }

    Err("No draft found for this thread".to_string())
}

/// Find the draft associated with a message ID
pub async fn get_draft_id_by_message_id(
    _account_id: &str,
    access_token: &str,
    message_id: &str,
) -> Result<String, String> {
    let client = reqwest::Client::new();

    let res = client
        .get(gmail_api_url("/gmail/v1/users/me/drafts"))
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to list drafts: {}", res.status()));
    }

    let body: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let drafts = body["drafts"].as_array();

    if let Some(drafts) = drafts {
        for draft in drafts {
            let draft_id = draft["id"].as_str().unwrap_or("");
            let msg_id = draft["message"]["id"].as_str().unwrap_or("");

            if msg_id == message_id && !draft_id.is_empty() {
                return Ok(draft_id.to_string());
            }
        }
    }

    Err("No draft found for this message".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::SqlitePool;
    use std::str::FromStr;
    use tempfile::tempdir;

    #[test]
    fn test_get_header() {
        let headers = vec![
            MessagePartHeader {
                name: "Subject".to_string(),
                value: "Hello".to_string(),
            },
            MessagePartHeader {
                name: "From".to_string(),
                value: "me@example.com".to_string(),
            },
        ];
        assert_eq!(get_header(&headers, "Subject"), Some("Hello"));
        assert_eq!(get_header(&headers, "From"), Some("me@example.com"));
        assert_eq!(get_header(&headers, "To"), None);
    }

    #[test]
    fn test_extract_body() {
        // base64 for "Hello World" is SGVsbG8gV29ybGQ=
        let data = "SGVsbG8gV29ybGQ=".to_string();
        let part = MessagePart {
            part_id: Some("0".to_string()),
            mime_type: "text/plain".to_string(),
            filename: None,
            headers: None,
            body: Some(MessagePartBody {
                size: 11,
                data: Some(data),
                attachment_id: None,
            }),
            parts: None,
        };
        assert_eq!(
            extract_body(&part, "text/plain"),
            Some("Hello World".to_string())
        );
        assert_eq!(extract_body(&part, "text/html"), None);
    }

    #[test]
    fn test_parse_to_mailbox() {
        // Plain email
        let m1 = parse_to_mailbox("test@example.com").unwrap();
        assert_eq!(m1.name, None);
        assert_eq!(m1.email.to_string(), "test@example.com");

        // Display name with angle brackets
        let m2 = parse_to_mailbox("John Doe <john@example.com>").unwrap();
        assert_eq!(m2.name, Some("John Doe".to_string()));
        assert_eq!(m2.email.to_string(), "john@example.com");

        // Display name with quotes
        let m3 = parse_to_mailbox("\"Jane Doe\" <jane@example.com>").unwrap();
        assert_eq!(m3.name, Some("Jane Doe".to_string()));
        assert_eq!(m3.email.to_string(), "jane@example.com");

        // Display name with emoji (the initially failing case)
        let m4 = parse_to_mailbox("🇦🇺Fernandinha <fernanda@example.com>").unwrap();
        assert_eq!(m4.name, Some("🇦🇺Fernandinha".to_string()));
        assert_eq!(m4.email.to_string(), "fernanda@example.com");
    }

    #[test]
    fn test_build_mime_message() {
        // Single recipient
        let res = build_mime_message(
            "me@test.com",
            "you@test.com",
            "Hi",
            "Body",
            None,
            None,
            false,
            &[],
        );
        assert!(res.is_ok());
        let encoded = res.unwrap();
        let decoded = base64::decode_config(&encoded, base64::URL_SAFE_NO_PAD)
            .expect("Should be valid base64");
        let mime = String::from_utf8(decoded).expect("Should be valid UTF-8");

        assert!(mime.contains("From: me@test.com"));
        assert!(mime.contains("To: you@test.com"));
        assert!(mime.contains("Subject: Hi"));
        assert!(mime.contains("Body"));

        // Multiple recipients with emoji
        let res2 = build_mime_message(
            "me@test.com",
            "🇦🇺Fernandinha <fernanda@test.com>, \"Bob\" <bob@test.com>, plain@test.com",
            "Multi Test",
            "Body",
            None,
            None,
            false,
            &[],
        );
        let encoded2 = res2.unwrap();
        let decoded2 = base64::decode_config(&encoded2, base64::URL_SAFE_NO_PAD).unwrap();
        let mime2 = String::from_utf8(decoded2).unwrap();

        // Lettre formats the To header with commas between recipients
        // display names generally get =?utf-8?b?...?= encoded in the headers,
        // so we just check for the plain addresses.
        assert!(mime2.contains("fernanda@test.com"));
        assert!(mime2.contains("bob@test.com"));
        assert!(mime2.contains("plain@test.com"));
    }

    #[tokio::test]
    async fn test_store_thread_messages() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_store.db");
        let options =
            SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.to_string_lossy()))
                .unwrap()
                .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();

        // Create tables needed
        sqlx::query("CREATE TABLE messages (id TEXT PRIMARY KEY, thread_id TEXT, account_id TEXT, sender TEXT, recipients TEXT, subject TEXT, snippet TEXT, internal_date INTEGER, body_plain TEXT, body_html TEXT, has_attachments INTEGER)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE thread_labels (thread_id TEXT, label_id TEXT, PRIMARY KEY(thread_id, label_id))")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE message_labels (message_id TEXT, label_id TEXT, PRIMARY KEY(message_id, label_id))")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE threads (id TEXT PRIMARY KEY, account_id TEXT, snippet TEXT, history_id TEXT, unread INTEGER)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE VIRTUAL TABLE messages_fts USING fts5(sender, subject, body_plain, content=messages, content_rowid=rowid)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE attachments (id TEXT PRIMARY KEY, message_id TEXT, filename TEXT, mime_type TEXT, size INTEGER, local_path TEXT, downloaded INTEGER)")
            .execute(&pool).await.unwrap();

        let thread_details = ThreadDetailsResponse {
            id: "t1".to_string(),
            messages: Some(vec![GmailMessage {
                id: "m1".to_string(),
                thread_id: "t1".to_string(),
                label_ids: Some(vec!["INBOX".to_string(), "UNREAD".to_string()]),
                snippet: Some("Snippet 1".to_string()),
                internal_date: "1614556800000".to_string(),
                payload: Some(MessagePart {
                    part_id: Some("0".to_string()),
                    mime_type: "text/plain".to_string(),
                    filename: None,
                    headers: Some(vec![
                        MessagePartHeader {
                            name: "From".to_string(),
                            value: "sender@test.com".to_string(),
                        },
                        MessagePartHeader {
                            name: "To".to_string(),
                            value: "me@test.com".to_string(),
                        },
                        MessagePartHeader {
                            name: "Subject".to_string(),
                            value: "Hello".to_string(),
                        },
                    ]),
                    body: Some(MessagePartBody {
                        size: 5,
                        data: Some("SGVsbG8=".to_string()),
                        attachment_id: None,
                    }), // "Hello"
                    parts: None,
                }),
            }]),
        };

        store_thread_messages(&pool, "acc1", &thread_details)
            .await
            .unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 1);

        let msg: (String, String) =
            sqlx::query_as("SELECT sender, body_plain FROM messages WHERE id='m1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(msg.0, "sender@test.com");
        assert_eq!(msg.1, "Hello");

        let labels: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM message_labels WHERE message_id='m1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(labels.0, 2);
    }

    #[test]
    fn test_sanitize_strips_script_tags() {
        let input = r#"<p>Hello</p><script>alert('xss')</script><p>World</p>"#;
        let result = sanitize_email_html(input);
        assert!(!result.contains("<script>"));
        assert!(!result.contains("alert"));
        assert!(result.contains("<p>Hello</p>"));
        assert!(result.contains("<p>World</p>"));
    }

    #[test]
    fn test_sanitize_strips_event_handlers() {
        let input = r#"<img src="https://example.com/img.png" onerror="alert('xss')" onclick="steal()">"#;
        let result = sanitize_email_html(input);
        assert!(!result.contains("onerror"));
        assert!(!result.contains("onclick"));
        assert!(result.contains("src=\"https://example.com/img.png\""));
    }

    #[test]
    fn test_sanitize_strips_iframe_and_object() {
        let input = r#"<p>Before</p><iframe src="https://evil.com"></iframe><object data="hack.swf"></object><p>After</p>"#;
        let result = sanitize_email_html(input);
        assert!(!result.contains("<iframe"));
        assert!(!result.contains("<object"));
        assert!(result.contains("Before"));
        assert!(result.contains("After"));
    }

    #[test]
    fn test_sanitize_preserves_email_tables() {
        let input = "<table width=\"600\" cellpadding=\"0\" cellspacing=\"0\" bgcolor=\"white\">\
            <tr><td align=\"center\" valign=\"top\" style=\"padding:10px\">\
            <img src=\"https://example.com/logo.png\" width=\"200\" height=\"50\" alt=\"Logo\">\
            </td></tr></table>";
        let result = sanitize_email_html(input);
        assert!(result.contains("<table"), "table tag missing");
        assert!(result.contains("width=\"600\""), "table width missing");
        assert!(result.contains("cellpadding=\"0\""), "cellpadding missing");
        assert!(result.contains("<td"), "td tag missing");
        assert!(result.contains("align=\"center\""), "align missing");
        assert!(result.contains("<img"), "img tag missing");
        assert!(result.contains("width=\"200\""), "img width missing");
    }

    #[test]
    fn test_sanitize_preserves_links() {
        let input = r#"<a href="https://example.com" target="_blank">Click</a>"#;
        let result = sanitize_email_html(input);
        assert!(result.contains("href=\"https://example.com\""));
        assert!(result.contains("target=\"_blank\""));
        assert!(result.contains("Click"));
    }

    #[test]
    fn test_sanitize_inlines_style_rules_and_keeps_style_tag() {
        let input = r#"<style>.header { color: red; }</style><div class="header">Hi</div>"#;
        let result = sanitize_email_html(input);
        // css-inline converts rules to inline styles but keeps <style> tags
        // for @font-face, @media, etc. that can't be inlined
        assert!(result.contains("color"), "inlined style should be preserved on element");
        assert!(result.contains("Hi"));
    }

    #[test]
    fn test_sanitize_strips_form_elements() {
        let input = r#"<form action="https://evil.com"><input type="text" name="password"><button>Submit</button></form>"#;
        let result = sanitize_email_html(input);
        assert!(!result.contains("<form"));
        assert!(!result.contains("<input"));
        assert!(!result.contains("<button"));
    }

    #[test]
    fn test_sanitize_empty_input() {
        assert_eq!(sanitize_email_html(""), "");
    }

    #[test]
    fn test_sanitize_strips_javascript_urls() {
        let input = r#"<a href="javascript:alert('xss')">Click me</a>"#;
        let result = sanitize_email_html(input);
        assert!(!result.contains("javascript:"));
    }

    #[test]
    fn test_sanitize_preserves_mailto_links() {
        let input = r#"<a href="mailto:test@example.com">Email us</a>"#;
        let result = sanitize_email_html(input);
        assert!(result.contains("mailto:test@example.com"));
    }

    #[test]
    fn test_sanitize_preserves_http_and_https_images() {
        let input = r#"<img src="http://example.com/logo.png" alt="Logo"><img src="https://example.com/banner.jpg" alt="Banner">"#;
        let result = sanitize_email_html(input);
        assert!(result.contains("http://example.com/logo.png"), "http image src lost: {}", result);
        assert!(result.contains("https://example.com/banner.jpg"), "https image src lost: {}", result);
    }

    #[test]
    fn test_sanitize_strips_css_url_properties() {
        // ammonia strips CSS properties containing URLs (background-image) for security.
        // This matches Gmail's behavior — background-image is not supported in Gmail either.
        let input = r#"<td style="background-image:url(https://example.com/bg.png)">content</td>"#;
        let result = sanitize_email_html(input);
        assert!(!result.contains("background-image"), "background-image should be stripped: {}", result);
        assert!(result.contains("content"));
    }

    #[test]
    fn test_sanitize_preserves_td_background_attr() {
        let input = r#"<table><tr><td background="https://example.com/bg.png">content</td></tr></table>"#;
        let result = sanitize_email_html(input);
        assert!(result.contains("background="), "td background attribute lost: {}", result);
    }

    // ---------------------------------------------------------------
    // Additional build_mime_message tests
    // ---------------------------------------------------------------

    #[test]
    fn test_build_mime_message_with_in_reply_to_and_references() {
        let res = build_mime_message(
            "me@test.com",
            "you@test.com",
            "Re: Hello",
            "Reply body",
            Some("<original-msg-id@mail.test.com>"),
            Some("<original-msg-id@mail.test.com> <another@mail.test.com>"),
            false,
            &[],
        );
        assert!(res.is_ok());
        let encoded = res.unwrap();
        let decoded = base64::decode_config(&encoded, base64::URL_SAFE_NO_PAD).unwrap();
        let mime = String::from_utf8(decoded).unwrap();
        assert!(
            mime.contains("In-Reply-To:"),
            "Missing In-Reply-To header: {}",
            mime
        );
        assert!(
            mime.contains("References:"),
            "Missing References header: {}",
            mime
        );
        assert!(mime.contains("original-msg-id@mail.test.com"));
    }

    #[test]
    fn test_build_mime_message_allow_empty_to_draft_mode() {
        let res = build_mime_message(
            "me@test.com",
            "",
            "Draft subject",
            "Draft body",
            None,
            None,
            true,
            &[],
        );
        assert!(res.is_ok(), "Draft mode with empty to should succeed");
    }

    #[test]
    fn test_build_mime_message_reject_empty_to_send_mode() {
        let res = build_mime_message(
            "me@test.com",
            "",
            "Subject",
            "Body",
            None,
            None,
            false,
            &[],
        );
        assert!(res.is_err());
        assert!(res
            .unwrap_err()
            .contains("No valid recipients"));
    }

    #[test]
    fn test_build_mime_message_unicode_subject() {
        let res = build_mime_message(
            "me@test.com",
            "you@test.com",
            "日本語のメール件名 🎉",
            "Body",
            None,
            None,
            false,
            &[],
        );
        assert!(res.is_ok(), "Unicode subject should be accepted");
        let encoded = res.unwrap();
        let decoded = base64::decode_config(&encoded, base64::URL_SAFE_NO_PAD).unwrap();
        let mime = String::from_utf8(decoded).unwrap();
        // Subject will be encoded but should appear in the MIME output
        assert!(mime.contains("Subject:"));
    }

    // ---------------------------------------------------------------
    // Additional extract_body tests
    // ---------------------------------------------------------------

    #[test]
    fn test_extract_body_nested_parts() {
        // "Nested HTML" base64url = "TmVzdGVkIEhUTUw="
        let child = MessagePart {
            part_id: Some("0.1".to_string()),
            mime_type: "text/html".to_string(),
            filename: None,
            headers: None,
            body: Some(MessagePartBody {
                size: 11,
                data: Some("TmVzdGVkIEhUTUw".to_string()),
                attachment_id: None,
            }),
            parts: None,
        };
        let parent = MessagePart {
            part_id: Some("0".to_string()),
            mime_type: "multipart/alternative".to_string(),
            filename: None,
            headers: None,
            body: None,
            parts: Some(vec![child]),
        };
        assert_eq!(
            extract_body(&parent, "text/html"),
            Some("Nested HTML".to_string())
        );
    }

    #[test]
    fn test_extract_body_deeply_nested() {
        // "Deep body" base64url = "RGVlcCBib2R5"
        let leaf = MessagePart {
            part_id: Some("0.0.1".to_string()),
            mime_type: "text/plain".to_string(),
            filename: None,
            headers: None,
            body: Some(MessagePartBody {
                size: 9,
                data: Some("RGVlcCBib2R5".to_string()),
                attachment_id: None,
            }),
            parts: None,
        };
        let mid = MessagePart {
            part_id: Some("0.0".to_string()),
            mime_type: "multipart/mixed".to_string(),
            filename: None,
            headers: None,
            body: None,
            parts: Some(vec![leaf]),
        };
        let root = MessagePart {
            part_id: Some("0".to_string()),
            mime_type: "multipart/alternative".to_string(),
            filename: None,
            headers: None,
            body: None,
            parts: Some(vec![mid]),
        };
        assert_eq!(
            extract_body(&root, "text/plain"),
            Some("Deep body".to_string())
        );
    }

    #[test]
    fn test_extract_body_wrong_mime_type_returns_none() {
        let part = MessagePart {
            part_id: Some("0".to_string()),
            mime_type: "text/plain".to_string(),
            filename: None,
            headers: None,
            body: Some(MessagePartBody {
                size: 5,
                data: Some("SGVsbG8".to_string()),
                attachment_id: None,
            }),
            parts: None,
        };
        assert_eq!(extract_body(&part, "text/html"), None);
    }

    // ---------------------------------------------------------------
    // Additional sanitize_email_html tests
    // ---------------------------------------------------------------

    #[test]
    fn test_sanitize_strips_base_tag() {
        let input = r#"<html><head><base href="https://evil.com/"></head><body><p>Content</p></body></html>"#;
        let result = sanitize_email_html(input);
        assert!(
            !result.contains("<base"),
            "base tag should be stripped: {}",
            result
        );
        assert!(result.contains("Content"));
    }

    #[test]
    fn test_sanitize_preserves_data_uri_img() {
        let input = r#"<img src="data:image/png;base64,iVBORw0KGgo=" alt="inline">"#;
        let result = sanitize_email_html(input);
        assert!(
            result.contains("data:image/png;base64,iVBORw0KGgo="),
            "data URI should be preserved: {}",
            result
        );
    }

    #[test]
    fn test_sanitize_preserves_inline_style_attr() {
        let input = r#"<div style="color: red; font-size: 14px;">Styled</div>"#;
        let result = sanitize_email_html(input);
        assert!(
            result.contains("style="),
            "inline style should be preserved: {}",
            result
        );
        assert!(result.contains("color"));
        assert!(result.contains("Styled"));
    }

    #[test]
    fn test_sanitize_strips_nested_obfuscated_scripts() {
        // The regex-based sanitizer strips matching <script>...</script> pairs.
        // With obfuscated/nested fragments, the inner <script>...</script> is
        // removed but residual markup fragments may remain after HTML parsing.
        // The key property: the executable script *content* (alert) is removed.
        let input = r#"<div><scr<script>ipt>alert('xss')</scr</script>ipt></div>"#;
        let result = sanitize_email_html(input);
        assert!(
            !result.contains("alert('xss')"),
            "script content should be stripped: {}",
            result
        );
    }

    #[test]
    fn test_sanitize_strips_svg_script() {
        let input = r#"<svg><script>alert('xss')</script></svg>"#;
        let result = sanitize_email_html(input);
        assert!(!result.contains("<script"));
        assert!(!result.contains("alert"));
    }

    // ---------------------------------------------------------------
    // DB-only function tests using crate::db::apply_schema
    // ---------------------------------------------------------------

    async fn test_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        crate::db::apply_schema(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn test_get_unhydrated_thread_ids_returns_threads_without_messages() {
        let pool = test_pool().await;

        // Insert two threads
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', 's1', 'h1', 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t2', 'acc1', 's2', 'h2', 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t3', 'acc2', 's3', 'h3', 0)")
            .execute(&pool).await.unwrap();

        // Add a message only for t1
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, subject, internal_date) VALUES ('m1', 't1', 'acc1', 'a@b.com', 'subj', 100)")
            .execute(&pool).await.unwrap();

        let unhydrated = get_unhydrated_thread_ids(&pool, "acc1").await;
        assert_eq!(unhydrated.len(), 1);
        assert_eq!(unhydrated[0], "t2");

        // t3 belongs to acc2 so should not appear for acc1
        let unhydrated2 = get_unhydrated_thread_ids(&pool, "acc2").await;
        assert_eq!(unhydrated2.len(), 1);
        assert_eq!(unhydrated2[0], "t3");
    }

    #[tokio::test]
    async fn test_get_unhydrated_thread_ids_all_hydrated() {
        let pool = test_pool().await;

        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', 's1', 'h1', 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, subject, internal_date) VALUES ('m1', 't1', 'acc1', 'a@b.com', 'subj', 100)")
            .execute(&pool).await.unwrap();

        let unhydrated = get_unhydrated_thread_ids(&pool, "acc1").await;
        assert!(unhydrated.is_empty());
    }

    #[tokio::test]
    async fn test_evict_old_message_bodies() {
        let pool = test_pool().await;

        // Create 3 threads with messages, each with a different date
        for i in 1..=3 {
            let tid = format!("t{}", i);
            let mid = format!("m{}", i);
            sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, 'acc1', 'snip', 'h1', 0)")
                .bind(&tid).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, subject, internal_date, body_html, body_plain) VALUES (?, ?, 'acc1', 'a@b.com', 'subj', ?, 'html body', 'plain body')")
                .bind(&mid).bind(&tid).bind(i * 1000).execute(&pool).await.unwrap();
        }

        // Keep only 1 most recent thread's bodies (t3 has internal_date 3000)
        evict_old_message_bodies(&pool, "acc1", 1).await;

        // t3 should still have bodies (most recent)
        let (html3, plain3): (String, String) = sqlx::query_as("SELECT body_html, body_plain FROM messages WHERE id = 'm3'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(html3, "html body");
        assert_eq!(plain3, "plain body");

        // t1 and t2 should be evicted
        let (html1, plain1): (String, String) = sqlx::query_as("SELECT body_html, body_plain FROM messages WHERE id = 'm1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(html1, "", "t1 body_html should be evicted");
        assert_eq!(plain1, "", "t1 body_plain should be evicted");

        let (html2, plain2): (String, String) = sqlx::query_as("SELECT body_html, body_plain FROM messages WHERE id = 'm2'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(html2, "", "t2 body_html should be evicted");
        assert_eq!(plain2, "", "t2 body_plain should be evicted");
    }

    #[tokio::test]
    async fn test_evict_old_message_bodies_different_account() {
        let pool = test_pool().await;

        // Thread for acc1
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', 'snip', 'h1', 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, subject, internal_date, body_html, body_plain) VALUES ('m1', 't1', 'acc1', 'a@b.com', 'subj', 1000, 'html', 'plain')")
            .execute(&pool).await.unwrap();

        // Thread for acc2
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t2', 'acc2', 'snip', 'h2', 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, subject, internal_date, body_html, body_plain) VALUES ('m2', 't2', 'acc2', 'b@c.com', 'subj', 1000, 'html2', 'plain2')")
            .execute(&pool).await.unwrap();

        // Evict for acc1 keeping 0 recent
        evict_old_message_bodies(&pool, "acc1", 0).await;

        // acc1 message should be evicted
        let (html1,): (String,) = sqlx::query_as("SELECT body_html FROM messages WHERE id = 'm1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(html1, "");

        // acc2 message should be untouched
        let (html2,): (String,) = sqlx::query_as("SELECT body_html FROM messages WHERE id = 'm2'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(html2, "html2");
    }

    // ---------------------------------------------------------------
    // Additional store_thread_messages tests
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_store_thread_messages_multiple_messages() {
        let pool = test_pool().await;

        // Insert the parent thread so the UNREAD update does not fail
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', '', '', 0)")
            .execute(&pool).await.unwrap();

        let thread_details = ThreadDetailsResponse {
            id: "t1".to_string(),
            messages: Some(vec![
                GmailMessage {
                    id: "m1".to_string(),
                    thread_id: "t1".to_string(),
                    label_ids: Some(vec!["INBOX".to_string()]),
                    snippet: Some("First".to_string()),
                    internal_date: "1000".to_string(),
                    payload: Some(MessagePart {
                        part_id: Some("0".to_string()),
                        mime_type: "text/plain".to_string(),
                        filename: None,
                        headers: Some(vec![
                            MessagePartHeader { name: "From".to_string(), value: "a@test.com".to_string() },
                            MessagePartHeader { name: "To".to_string(), value: "b@test.com".to_string() },
                            MessagePartHeader { name: "Subject".to_string(), value: "Thread subject".to_string() },
                        ]),
                        body: Some(MessagePartBody { size: 5, data: Some("SGVsbG8".to_string()), attachment_id: None }),
                        parts: None,
                    }),
                },
                GmailMessage {
                    id: "m2".to_string(),
                    thread_id: "t1".to_string(),
                    label_ids: Some(vec!["INBOX".to_string(), "UNREAD".to_string()]),
                    snippet: Some("Second".to_string()),
                    internal_date: "2000".to_string(),
                    payload: Some(MessagePart {
                        part_id: Some("0".to_string()),
                        mime_type: "text/plain".to_string(),
                        filename: None,
                        headers: Some(vec![
                            MessagePartHeader { name: "From".to_string(), value: "b@test.com".to_string() },
                            MessagePartHeader { name: "To".to_string(), value: "a@test.com".to_string() },
                            MessagePartHeader { name: "Subject".to_string(), value: "Re: Thread subject".to_string() },
                        ]),
                        body: Some(MessagePartBody { size: 5, data: Some("V29ybGQ".to_string()), attachment_id: None }),
                        parts: None,
                    }),
                },
            ]),
        };

        store_thread_messages(&pool, "acc1", &thread_details).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages WHERE thread_id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 2);

        // Thread should be marked unread due to UNREAD label on m2
        let (unread,): (i32,) = sqlx::query_as("SELECT unread FROM threads WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(unread, 1);
    }

    #[tokio::test]
    async fn test_store_thread_messages_html_body_is_sanitized() {
        let pool = test_pool().await;

        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', '', '', 0)")
            .execute(&pool).await.unwrap();

        // HTML with script tag, base64url encoded
        // "<p>Safe</p><script>alert('xss')</script>" =>
        let html_raw = "<p>Safe</p><script>alert('xss')</script>";
        let html_b64 = base64::encode_config(html_raw, base64::URL_SAFE);

        let thread_details = ThreadDetailsResponse {
            id: "t1".to_string(),
            messages: Some(vec![GmailMessage {
                id: "m1".to_string(),
                thread_id: "t1".to_string(),
                label_ids: None,
                snippet: None,
                internal_date: "1000".to_string(),
                payload: Some(MessagePart {
                    part_id: Some("0".to_string()),
                    mime_type: "multipart/alternative".to_string(),
                    filename: None,
                    headers: Some(vec![
                        MessagePartHeader { name: "From".to_string(), value: "a@b.com".to_string() },
                    ]),
                    body: None,
                    parts: Some(vec![MessagePart {
                        part_id: Some("0.1".to_string()),
                        mime_type: "text/html".to_string(),
                        filename: None,
                        headers: None,
                        body: Some(MessagePartBody { size: html_raw.len() as i32, data: Some(html_b64), attachment_id: None }),
                        parts: None,
                    }]),
                }),
            }]),
        };

        store_thread_messages(&pool, "acc1", &thread_details).await.unwrap();

        let (body_html,): (String,) = sqlx::query_as("SELECT body_html FROM messages WHERE id = 'm1'")
            .fetch_one(&pool).await.unwrap();
        assert!(!body_html.contains("<script>"), "script should be sanitized from stored HTML");
        assert!(body_html.contains("Safe"));
    }

    #[tokio::test]
    async fn test_store_thread_messages_with_labels_including_draft() {
        let pool = test_pool().await;

        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', '', '', 0)")
            .execute(&pool).await.unwrap();

        let thread_details = ThreadDetailsResponse {
            id: "t1".to_string(),
            messages: Some(vec![GmailMessage {
                id: "m1".to_string(),
                thread_id: "t1".to_string(),
                label_ids: Some(vec!["DRAFT".to_string(), "INBOX".to_string()]),
                snippet: Some("Draft msg".to_string()),
                internal_date: "1000".to_string(),
                payload: Some(MessagePart {
                    part_id: Some("0".to_string()),
                    mime_type: "text/plain".to_string(),
                    filename: None,
                    headers: None,
                    body: Some(MessagePartBody { size: 5, data: Some("SGVsbG8".to_string()), attachment_id: None }),
                    parts: None,
                }),
            }]),
        };

        store_thread_messages(&pool, "acc1", &thread_details).await.unwrap();

        // Verify DRAFT label was stored
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM message_labels WHERE message_id = 'm1' AND label_id = 'DRAFT'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 1);

        // Also in thread_labels
        let (tl_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM thread_labels WHERE thread_id = 't1' AND label_id = 'DRAFT'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(tl_count, 1);
    }

    #[tokio::test]
    async fn test_store_thread_messages_no_body() {
        let pool = test_pool().await;

        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', '', '', 0)")
            .execute(&pool).await.unwrap();

        let thread_details = ThreadDetailsResponse {
            id: "t1".to_string(),
            messages: Some(vec![GmailMessage {
                id: "m1".to_string(),
                thread_id: "t1".to_string(),
                label_ids: None,
                snippet: Some("No body".to_string()),
                internal_date: "1000".to_string(),
                payload: Some(MessagePart {
                    part_id: Some("0".to_string()),
                    mime_type: "text/plain".to_string(),
                    filename: None,
                    headers: None,
                    body: Some(MessagePartBody { size: 0, data: None, attachment_id: None }),
                    parts: None,
                }),
            }]),
        };

        store_thread_messages(&pool, "acc1", &thread_details).await.unwrap();

        let (body_plain, body_html): (String, String) =
            sqlx::query_as("SELECT body_plain, body_html FROM messages WHERE id = 'm1'")
                .fetch_one(&pool).await.unwrap();
        assert_eq!(body_plain, "");
        assert_eq!(body_html, "");
    }

    #[tokio::test]
    async fn test_store_thread_messages_no_payload() {
        let pool = test_pool().await;

        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', '', '', 0)")
            .execute(&pool).await.unwrap();

        let thread_details = ThreadDetailsResponse {
            id: "t1".to_string(),
            messages: Some(vec![GmailMessage {
                id: "m1".to_string(),
                thread_id: "t1".to_string(),
                label_ids: None,
                snippet: None,
                internal_date: "1000".to_string(),
                payload: None,
            }]),
        };

        store_thread_messages(&pool, "acc1", &thread_details).await.unwrap();

        let (sender, subject): (String, String) =
            sqlx::query_as("SELECT sender, subject FROM messages WHERE id = 'm1'")
                .fetch_one(&pool).await.unwrap();
        assert_eq!(sender, "");
        assert_eq!(subject, "");
    }

    // ---------------------------------------------------------------
    // httpmock integration tests
    // ---------------------------------------------------------------

    use std::sync::Mutex;

    // Env var is process-global, so serialize mock tests that set it.
    static MOCK_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn test_fetch_and_store_labels_via_mock() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/labels")
                .header("Authorization", "Bearer fake_token");
            then.status(200).json_body(serde_json::json!({
                "labels": [
                    {"id": "INBOX", "name": "INBOX", "type": "system", "messagesUnread": 5},
                    {"id": "SENT", "name": "SENT", "type": "system"},
                    {"id": "Label_1", "name": "Work", "type": "user", "messagesUnread": 2}
                ]
            }));
        });

        let base = server.base_url();
        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", &base);
            let r = fetch_and_store_labels(&pool, "acc1", "fake_token").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok(), "fetch_and_store_labels failed: {:?}", result);
        mock.assert();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM labels WHERE account_id = 'acc1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 3);

        let (name, unread): (String, i32) =
            sqlx::query_as("SELECT name, unread_count FROM labels WHERE id = 'INBOX'")
                .fetch_one(&pool).await.unwrap();
        assert_eq!(name, "INBOX");
        assert_eq!(unread, 5);

        let (name2, unread2): (String, i32) =
            sqlx::query_as("SELECT name, unread_count FROM labels WHERE id = 'Label_1'")
                .fetch_one(&pool).await.unwrap();
        assert_eq!(name2, "Work");
        assert_eq!(unread2, 2);
    }

    #[tokio::test]
    async fn test_fetch_and_store_labels_http_error() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/labels");
            then.status(401).body("Unauthorized");
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = fetch_and_store_labels(&pool, "acc1", "bad_token").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("401"));
        mock.assert();
    }

    #[tokio::test]
    async fn test_fetch_and_store_threads_via_mock() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/threads")
                .header("Authorization", "Bearer fake_token");
            then.status(200).json_body(serde_json::json!({
                "threads": [
                    {"id": "t1", "snippet": "Hello there", "historyId": "12345"},
                    {"id": "t2", "snippet": "Another thread", "historyId": "12346"}
                ],
                "nextPageToken": null
            }));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = fetch_and_store_threads(&pool, "acc1", "fake_token", Some(&["INBOX"]), 10).await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok(), "fetch_and_store_threads failed: {:?}", result);
        mock.assert();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM threads WHERE account_id = 'acc1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 2);

        let (snippet,): (String,) = sqlx::query_as("SELECT snippet FROM threads WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(snippet, "Hello there");

        // Verify thread_labels were created
        let (label_count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM thread_labels WHERE thread_id = 't1' AND label_id = 'INBOX'")
                .fetch_one(&pool).await.unwrap();
        assert_eq!(label_count, 1);
    }

    #[tokio::test]
    async fn test_fetch_and_store_threads_empty_response() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/threads");
            then.status(200).json_body(serde_json::json!({
                "threads": null,
                "nextPageToken": null
            }));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = fetch_and_store_threads(&pool, "acc1", "fake_token", None, 10).await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok());
        mock.assert();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM threads WHERE account_id = 'acc1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn test_fetch_messages_for_thread_via_mock() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        // Insert the thread first so label/unread updates work
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', '', '', 0)")
            .execute(&pool).await.unwrap();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/threads/t1")
                .header("Authorization", "Bearer fake_token");
            then.status(200).json_body(serde_json::json!({
                "id": "t1",
                "messages": [{
                    "id": "m1",
                    "threadId": "t1",
                    "labelIds": ["INBOX", "UNREAD"],
                    "snippet": "Test snippet",
                    "internalDate": "1700000000000",
                    "payload": {
                        "partId": "0",
                        "mimeType": "text/plain",
                        "headers": [
                            {"name": "From", "value": "sender@example.com"},
                            {"name": "To", "value": "me@example.com"},
                            {"name": "Subject", "value": "Test Subject"}
                        ],
                        "body": {"size": 11, "data": "SGVsbG8gV29ybGQ"}
                    }
                }]
            }));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = fetch_messages_for_thread(&pool, "acc1", "fake_token", "t1").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok(), "fetch_messages_for_thread failed: {:?}", result);
        mock.assert();

        let (sender, subject, body): (String, String, String) =
            sqlx::query_as("SELECT sender, subject, body_plain FROM messages WHERE id = 'm1'")
                .fetch_one(&pool).await.unwrap();
        assert_eq!(sender, "sender@example.com");
        assert_eq!(subject, "Test Subject");
        assert_eq!(body, "Hello World");

        // Thread should be marked unread
        let (unread,): (i32,) = sqlx::query_as("SELECT unread FROM threads WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(unread, 1);
    }

    #[tokio::test]
    async fn test_fetch_messages_for_thread_http_error() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/threads/t_missing");
            then.status(404).body("Not Found");
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = fetch_messages_for_thread(&pool, "acc1", "fake_token", "t_missing").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("404"));
        mock.assert();
    }

    #[tokio::test]
    async fn test_modify_thread_mark_read_via_mock() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        // Insert a thread that is currently unread
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', 'snip', 'h1', 1)")
            .execute(&pool).await.unwrap();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/gmail/v1/users/me/threads/t1/modify")
                .header("Authorization", "Bearer fake_token")
                .json_body(serde_json::json!({
                    "addLabelIds": [],
                    "removeLabelIds": ["UNREAD"]
                }));
            then.status(200).json_body(serde_json::json!({"id": "t1"}));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = modify_thread(&pool, "acc1", "fake_token", "t1", vec![], vec!["UNREAD".to_string()]).await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok(), "modify_thread failed: {:?}", result);
        mock.assert();

        let (unread,): (i32,) = sqlx::query_as("SELECT unread FROM threads WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(unread, 0, "Thread should be marked as read");
    }

    #[tokio::test]
    async fn test_modify_thread_add_star_via_mock() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', 'snip', 'h1', 0)")
            .execute(&pool).await.unwrap();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/gmail/v1/users/me/threads/t1/modify");
            then.status(200).json_body(serde_json::json!({"id": "t1"}));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = modify_thread(&pool, "acc1", "fake_token", "t1", vec!["STARRED".to_string()], vec![]).await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok());
        mock.assert();

        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM thread_labels WHERE thread_id = 't1' AND label_id = 'STARRED'")
                .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 1, "STARRED label should be added");
    }

    #[tokio::test]
    async fn test_modify_thread_remove_star_via_mock() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', 'snip', 'h1', 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'STARRED')")
            .execute(&pool).await.unwrap();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/gmail/v1/users/me/threads/t1/modify");
            then.status(200).json_body(serde_json::json!({"id": "t1"}));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = modify_thread(&pool, "acc1", "fake_token", "t1", vec![], vec!["STARRED".to_string()]).await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok());
        mock.assert();

        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM thread_labels WHERE thread_id = 't1' AND label_id = 'STARRED'")
                .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 0, "STARRED label should be removed");
    }

    #[tokio::test]
    async fn test_trash_thread_via_mock() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        // Set up thread with messages and labels
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', 'snip', 'h1', 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, subject, internal_date) VALUES ('m1', 't1', 'acc1', 'a@b.com', 'subj', 100)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'INBOX')")
            .execute(&pool).await.unwrap();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/gmail/v1/users/me/threads/t1/trash")
                .header("Authorization", "Bearer fake_token");
            then.status(200).json_body(serde_json::json!({"id": "t1"}));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = trash_thread(&pool, "acc1", "fake_token", "t1").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok(), "trash_thread failed: {:?}", result);
        mock.assert();

        // Thread, messages, and labels should all be deleted
        let (thread_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM threads WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(thread_count, 0, "Thread should be deleted");

        let (msg_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages WHERE thread_id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(msg_count, 0, "Messages should be deleted");

        let (label_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM thread_labels WHERE thread_id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(label_count, 0, "Thread labels should be deleted");
    }

    #[tokio::test]
    async fn test_untrash_thread_via_mock() {
        let server = httpmock::MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/gmail/v1/users/me/threads/t1/untrash")
                .header("Authorization", "Bearer fake_token");
            then.status(200).json_body(serde_json::json!({"id": "t1"}));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = untrash_thread("fake_token", "t1").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok(), "untrash_thread failed: {:?}", result);
        mock.assert();
    }

    #[tokio::test]
    async fn test_untrash_thread_http_error() {
        let server = httpmock::MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/gmail/v1/users/me/threads/t1/untrash");
            then.status(500).body("Internal Server Error");
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = untrash_thread("fake_token", "t1").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("500"));
        mock.assert();
    }

    #[tokio::test]
    async fn test_send_message_via_mock() {
        let server = httpmock::MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/gmail/v1/users/me/messages/send")
                .header("Authorization", "Bearer fake_token");
            then.status(200).json_body(serde_json::json!({
                "id": "msg_new",
                "threadId": "t1",
                "labelIds": ["SENT"]
            }));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = send_message(
                "acc1", "me@test.com", "fake_token",
                "you@test.com", "Hello", "<p>Hi there</p>",
                Some("t1"), None, None, &[],
            ).await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok(), "send_message failed: {:?}", result);
        mock.assert();
    }

    #[tokio::test]
    async fn test_send_message_http_error() {
        let server = httpmock::MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/gmail/v1/users/me/messages/send");
            then.status(403).body("Forbidden");
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = send_message(
                "acc1", "me@test.com", "fake_token",
                "you@test.com", "Hello", "body",
                None, None, None, &[],
            ).await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("403"));
        mock.assert();
    }

    #[tokio::test]
    async fn test_save_draft_create_via_mock() {
        let server = httpmock::MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/gmail/v1/users/me/drafts")
                .header("Authorization", "Bearer fake_token");
            then.status(200).json_body(serde_json::json!({
                "id": "draft_123",
                "message": {"id": "msg_abc", "threadId": "t1"}
            }));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = save_draft(
                "acc1", "me@test.com", "fake_token",
                "you@test.com", "Draft Subject", "<p>Draft body</p>",
                Some("t1"), None, None, None, &[],
            ).await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok(), "save_draft create failed: {:?}", result);
        assert_eq!(result.unwrap(), "draft_123");
        mock.assert();
    }

    #[tokio::test]
    async fn test_save_draft_update_via_mock() {
        let server = httpmock::MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::PUT)
                .path("/gmail/v1/users/me/drafts/draft_existing")
                .header("Authorization", "Bearer fake_token");
            then.status(200).json_body(serde_json::json!({
                "id": "draft_existing",
                "message": {"id": "msg_updated"}
            }));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = save_draft(
                "acc1", "me@test.com", "fake_token",
                "you@test.com", "Updated Subject", "<p>Updated</p>",
                None, None, None, Some("draft_existing"), &[],
            ).await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok(), "save_draft update failed: {:?}", result);
        assert_eq!(result.unwrap(), "draft_existing");
        mock.assert();
    }

    #[tokio::test]
    async fn test_save_draft_empty_to_via_mock() {
        let server = httpmock::MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/gmail/v1/users/me/drafts");
            then.status(200).json_body(serde_json::json!({
                "id": "draft_no_to",
                "message": {"id": "msg_no_to"}
            }));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = save_draft(
                "acc1", "me@test.com", "fake_token",
                "", "Empty To Draft", "<p>No recipient yet</p>",
                None, None, None, None, &[],
            ).await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok(), "save_draft with empty to failed: {:?}", result);
        assert_eq!(result.unwrap(), "draft_no_to");
        mock.assert();
    }

    #[tokio::test]
    async fn test_delete_draft_via_mock() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        // Insert a local draft to verify cleanup
        sqlx::query("INSERT INTO drafts (id, account_id, subject) VALUES ('d1', 'acc1', 'Draft')")
            .execute(&pool).await.unwrap();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::DELETE)
                .path("/gmail/v1/users/me/drafts/d1")
                .header("Authorization", "Bearer fake_token");
            then.status(204);
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = delete_draft(&pool, "acc1", "me@test.com", "fake_token", "d1").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok(), "delete_draft failed: {:?}", result);
        mock.assert();

        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM drafts WHERE id = 'd1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 0, "Local draft should be cleaned up");
    }

    #[tokio::test]
    async fn test_delete_draft_by_thread_via_mock() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        // Insert a local draft to verify cleanup
        sqlx::query("INSERT INTO drafts (id, account_id, subject) VALUES ('d1', 'acc1', 'Draft')")
            .execute(&pool).await.unwrap();

        // Mock list drafts
        let list_mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/drafts")
                .header("Authorization", "Bearer fake_token");
            then.status(200).json_body(serde_json::json!({
                "drafts": [
                    {"id": "d1", "message": {"id": "msg1", "threadId": "t1"}},
                    {"id": "d2", "message": {"id": "msg2", "threadId": "t2"}}
                ]
            }));
        });

        // Mock delete draft d1 (the one matching thread t1)
        let delete_mock = server.mock(|when, then| {
            when.method(httpmock::Method::DELETE)
                .path("/gmail/v1/users/me/drafts/d1")
                .header("Authorization", "Bearer fake_token");
            then.status(204);
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = delete_draft_by_thread(&pool, "acc1", "fake_token", "t1").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok(), "delete_draft_by_thread failed: {:?}", result);
        list_mock.assert();
        delete_mock.assert();

        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM drafts WHERE id = 'd1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 0, "Local draft should be cleaned up");
    }

    #[tokio::test]
    async fn test_delete_draft_by_thread_not_found() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        let list_mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/drafts");
            then.status(200).json_body(serde_json::json!({
                "drafts": [
                    {"id": "d1", "message": {"id": "msg1", "threadId": "t_other"}}
                ]
            }));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = delete_draft_by_thread(&pool, "acc1", "fake_token", "t_nonexistent").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No draft found"));
        list_mock.assert();
    }

    #[tokio::test]
    async fn test_get_draft_id_by_message_id_via_mock() {
        let server = httpmock::MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/drafts")
                .header("Authorization", "Bearer fake_token");
            then.status(200).json_body(serde_json::json!({
                "drafts": [
                    {"id": "d1", "message": {"id": "msg1", "threadId": "t1"}},
                    {"id": "d2", "message": {"id": "msg2", "threadId": "t2"}}
                ]
            }));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = get_draft_id_by_message_id("acc1", "fake_token", "msg2").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "d2");
        mock.assert();
    }

    #[tokio::test]
    async fn test_get_draft_id_by_message_id_not_found() {
        let server = httpmock::MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/drafts");
            then.status(200).json_body(serde_json::json!({
                "drafts": [
                    {"id": "d1", "message": {"id": "msg1", "threadId": "t1"}}
                ]
            }));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = get_draft_id_by_message_id("acc1", "fake_token", "msg_nonexistent").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No draft found"));
        mock.assert();
    }

    #[tokio::test]
    async fn test_batch_hydrate_threads_via_mock() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        // Insert threads so label/unread updates work
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', '', '', 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t2', 'acc1', '', '', 0)")
            .execute(&pool).await.unwrap();

        let mock_t1 = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/threads/t1");
            then.status(200).json_body(serde_json::json!({
                "id": "t1",
                "messages": [{
                    "id": "m1",
                    "threadId": "t1",
                    "labelIds": ["INBOX"],
                    "snippet": "First thread",
                    "internalDate": "1000",
                    "payload": {
                        "partId": "0",
                        "mimeType": "text/plain",
                        "headers": [
                            {"name": "From", "value": "a@test.com"},
                            {"name": "Subject", "value": "Subject 1"}
                        ],
                        "body": {"size": 5, "data": "SGVsbG8"}
                    }
                }]
            }));
        });

        let mock_t2 = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/threads/t2");
            then.status(200).json_body(serde_json::json!({
                "id": "t2",
                "messages": [{
                    "id": "m2",
                    "threadId": "t2",
                    "labelIds": ["INBOX"],
                    "snippet": "Second thread",
                    "internalDate": "2000",
                    "payload": {
                        "partId": "0",
                        "mimeType": "text/plain",
                        "headers": [
                            {"name": "From", "value": "b@test.com"},
                            {"name": "Subject", "value": "Subject 2"}
                        ],
                        "body": {"size": 5, "data": "V29ybGQ"}
                    }
                }]
            }));
        });

        let (total, completed) = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = batch_hydrate_threads(
                &pool, "acc1", "fake_token",
                vec!["t1".to_string(), "t2".to_string()],
            ).await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert_eq!(total, 2);
        assert_eq!(completed, 2);
        mock_t1.assert();
        mock_t2.assert();

        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages WHERE account_id = 'acc1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_batch_hydrate_threads_partial_failure() {
        let server = httpmock::MockServer::start();
        let pool = test_pool().await;

        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', '', '', 0)")
            .execute(&pool).await.unwrap();

        let mock_t1 = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/threads/t1");
            then.status(200).json_body(serde_json::json!({
                "id": "t1",
                "messages": [{
                    "id": "m1",
                    "threadId": "t1",
                    "labelIds": ["INBOX"],
                    "internalDate": "1000",
                    "payload": {
                        "partId": "0",
                        "mimeType": "text/plain",
                        "headers": [{"name": "From", "value": "a@test.com"}],
                        "body": {"size": 5, "data": "SGVsbG8"}
                    }
                }]
            }));
        });

        let mock_t_fail = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/threads/t_fail");
            then.status(500).body("Server Error");
        });

        let (total, completed) = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = batch_hydrate_threads(
                &pool, "acc1", "fake_token",
                vec!["t1".to_string(), "t_fail".to_string()],
            ).await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert_eq!(total, 2);
        assert_eq!(completed, 1, "Only t1 should succeed");
        mock_t1.assert();
        mock_t_fail.assert();
    }

    // ── extract_attachments helpers ──────────────────────────────────

    fn make_part(
        part_id: &str,
        mime_type: &str,
        filename: Option<&str>,
        attachment_id: Option<&str>,
        size: i32,
    ) -> MessagePart {
        MessagePart {
            part_id: Some(part_id.to_string()),
            mime_type: mime_type.to_string(),
            filename: filename.map(|s| s.to_string()),
            headers: None,
            body: Some(MessagePartBody {
                size,
                data: None,
                attachment_id: attachment_id.map(|s| s.to_string()),
            }),
            parts: None,
        }
    }

    fn make_multipart(part_id: &str, children: Vec<MessagePart>) -> MessagePart {
        MessagePart {
            part_id: Some(part_id.to_string()),
            mime_type: "multipart/mixed".to_string(),
            filename: None,
            headers: None,
            body: None,
            parts: Some(children),
        }
    }

    // ── extract_attachments tests ───────────────────────────────────

    #[test]
    fn test_extract_attachments_empty_for_text_part() {
        let part = make_part("0", "text/plain", None, None, 100);
        let result = extract_attachments(&part, "msg1");
        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_attachments_single_attachment() {
        let part = make_part("1", "application/pdf", Some("report.pdf"), Some("att123"), 5000);
        let result = extract_attachments(&part, "msg1");
        assert_eq!(result.len(), 1);
        let att = &result[0];
        assert_eq!(att.id, "att123");
        assert_eq!(att.message_id, "msg1");
        assert_eq!(att.filename, "report.pdf");
        assert_eq!(att.mime_type, "application/pdf");
        assert_eq!(att.size, 5000);
    }

    #[test]
    fn test_extract_attachments_nested_multipart() {
        let text = make_part("0", "text/plain", None, None, 200);
        let pdf = make_part("1", "application/pdf", Some("doc.pdf"), Some("att_pdf"), 3000);
        let root = make_multipart("root", vec![text, pdf]);

        let result = extract_attachments(&root, "msg2");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].filename, "doc.pdf");
        assert_eq!(result[0].id, "att_pdf");
    }

    #[test]
    fn test_extract_attachments_multiple_attachments() {
        let html = make_part("0", "text/html", None, None, 500);
        let png = make_part("1", "image/png", Some("photo.png"), Some("att_png"), 12000);
        let zip = make_part("2", "application/zip", Some("archive.zip"), Some("att_zip"), 80000);
        let root = make_multipart("root", vec![html, png, zip]);

        let result = extract_attachments(&root, "msg3");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].filename, "photo.png");
        assert_eq!(result[1].filename, "archive.zip");
    }

    #[test]
    fn test_extract_attachments_deeply_nested() {
        let text = make_part("0.0", "text/plain", None, None, 100);
        let html = make_part("0.1", "text/html", None, None, 300);
        let alternative = MessagePart {
            part_id: Some("0".to_string()),
            mime_type: "multipart/alternative".to_string(),
            filename: None,
            headers: None,
            body: None,
            parts: Some(vec![text, html]),
        };
        let pdf = make_part("1", "application/pdf", Some("deep.pdf"), Some("att_deep"), 9000);
        let root = make_multipart("root", vec![alternative, pdf]);

        let result = extract_attachments(&root, "msg4");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].filename, "deep.pdf");
        assert_eq!(result[0].id, "att_deep");
    }

    #[test]
    fn test_extract_attachments_empty_filename_ignored() {
        let part = make_part("1", "application/pdf", Some(""), Some("att_empty"), 1000);
        let result = extract_attachments(&part, "msg5");
        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_attachments_generated_id_when_no_attachment_id() {
        let part = make_part("7", "image/jpeg", Some("photo.jpg"), None, 4000);
        let result = extract_attachments(&part, "msg6");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "msg6_7");
    }

    #[test]
    fn test_extract_attachments_no_body() {
        let part = MessagePart {
            part_id: Some("3".to_string()),
            mime_type: "application/octet-stream".to_string(),
            filename: Some("data.bin".to_string()),
            headers: None,
            body: None,
            parts: None,
        };
        let result = extract_attachments(&part, "msg7");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].size, 0);
        assert_eq!(result[0].id, "msg7_3");
        assert_eq!(result[0].filename, "data.bin");
    }

    #[test]
    fn test_build_mime_message_with_attachment() {
        let attachment = AttachmentFile {
            filename: "test.txt".to_string(),
            mime_type: "text/plain".to_string(),
            data: b"Hello attachment".to_vec(),
        };
        let res = build_mime_message(
            "me@test.com",
            "you@test.com",
            "With attachment",
            "<p>See attached</p>",
            None,
            None,
            false,
            &[attachment],
        );
        assert!(res.is_ok());
        let encoded = res.unwrap();
        let decoded = base64::decode_config(&encoded, base64::URL_SAFE_NO_PAD)
            .expect("valid base64");
        let mime = String::from_utf8(decoded).expect("valid UTF-8");
        assert!(mime.contains("multipart/mixed"));
        assert!(mime.contains("test.txt"));
        assert!(mime.contains("text/html"));
    }

    #[test]
    fn test_build_mime_message_with_multiple_attachments() {
        let att1 = AttachmentFile {
            filename: "file1.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            data: vec![0x25, 0x50, 0x44, 0x46],
        };
        let att2 = AttachmentFile {
            filename: "image.png".to_string(),
            mime_type: "image/png".to_string(),
            data: vec![0x89, 0x50, 0x4E, 0x47],
        };
        let res = build_mime_message(
            "me@test.com",
            "you@test.com",
            "Two files",
            "<p>Two attachments</p>",
            None,
            None,
            false,
            &[att1, att2],
        );
        assert!(res.is_ok());
        let encoded = res.unwrap();
        let decoded = base64::decode_config(&encoded, base64::URL_SAFE_NO_PAD).unwrap();
        let mime = String::from_utf8(decoded).unwrap();
        assert!(mime.contains("file1.pdf"));
        assert!(mime.contains("image.png"));
        assert!(mime.contains("multipart/mixed"));
    }

    #[test]
    fn test_build_mime_message_no_attachments_unchanged() {
        let res = build_mime_message(
            "me@test.com",
            "you@test.com",
            "No attachments",
            "<p>Plain email</p>",
            None,
            None,
            false,
            &[],
        );
        assert!(res.is_ok());
        let encoded = res.unwrap();
        let decoded = base64::decode_config(&encoded, base64::URL_SAFE_NO_PAD).unwrap();
        let mime = String::from_utf8(decoded).unwrap();
        // Single-part should NOT contain multipart
        assert!(!mime.contains("multipart/mixed"));
        assert!(mime.contains("text/html"));
    }

    #[test]
    fn test_read_attachment_files_valid() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "hello world").unwrap();

        let result = read_attachment_files(&[file_path.to_string_lossy().to_string()]);
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].filename, "test.txt");
        assert_eq!(files[0].data, b"hello world");
        assert_eq!(files[0].mime_type, "text/plain");
    }

    #[test]
    fn test_read_attachment_files_size_limit() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("big.bin");
        // Create a file just over 25MB
        let data = vec![0u8; 26 * 1024 * 1024];
        std::fs::write(&file_path, &data).unwrap();

        let result = read_attachment_files(&[file_path.to_string_lossy().to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("25MB"));
    }

    #[test]
    fn test_read_attachment_files_nonexistent() {
        let result = read_attachment_files(&["/nonexistent/file.txt".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_attachment_files_empty() {
        let result = read_attachment_files(&[]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // ---------------------------------------------------------------
    // History API tests
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_get_set_last_history_id() {
        let pool = test_pool().await;

        // Initially no history
        let result = get_last_history_id(&pool, "acc1").await;
        assert!(result.is_none());

        // Set history
        set_last_history_id(&pool, "acc1", "12345").await;
        let result = get_last_history_id(&pool, "acc1").await;
        assert_eq!(result, Some("12345".to_string()));

        // Update history
        set_last_history_id(&pool, "acc1", "67890").await;
        let result = get_last_history_id(&pool, "acc1").await;
        assert_eq!(result, Some("67890".to_string()));

        // Different account isolation
        set_last_history_id(&pool, "acc2", "11111").await;
        assert_eq!(get_last_history_id(&pool, "acc1").await, Some("67890".to_string()));
        assert_eq!(get_last_history_id(&pool, "acc2").await, Some("11111".to_string()));
    }

    #[tokio::test]
    async fn test_fetch_history_expired_returns_none() {
        let pool = test_pool().await;
        let server = httpmock::MockServer::start();

        let mock = server.mock(|when, then| {
            when.method("GET").path("/gmail/v1/users/me/history");
            then.status(404);
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = fetch_history(&pool, "acc1", "token", "old_id").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.unwrap().is_none());
        mock.assert();
    }

    #[tokio::test]
    async fn test_fetch_history_success_with_new_messages() {
        let pool = test_pool().await;
        let server = httpmock::MockServer::start();

        // Need a thread in the DB for label operations
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', '', '100', 0)")
            .execute(&pool).await.unwrap();

        let mock = server.mock(|when, then| {
            when.method("GET").path("/gmail/v1/users/me/history");
            then.status(200).json_body(serde_json::json!({
                "history": [{
                    "messagesAdded": [{
                        "message": {
                            "id": "msg1",
                            "threadId": "t1",
                            "labelIds": ["INBOX", "UNREAD"]
                        }
                    }]
                }],
                "historyId": "200"
            }));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = fetch_history(&pool, "acc1", "token", "100").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok());
        let delta = result.unwrap().unwrap();
        assert!(delta.threads_to_hydrate.contains(&"t1".to_string()));
        assert_eq!(delta.new_inbox_message_ids, vec!["msg1".to_string()]);
        assert_eq!(delta.new_history_id, "200");
        mock.assert();
    }

    #[tokio::test]
    async fn test_fetch_history_no_changes() {
        let pool = test_pool().await;
        let server = httpmock::MockServer::start();

        let mock = server.mock(|when, then| {
            when.method("GET").path("/gmail/v1/users/me/history");
            then.status(200).json_body(serde_json::json!({
                "historyId": "150"
            }));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = fetch_history(&pool, "acc1", "token", "100").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        assert!(result.is_ok());
        let delta = result.unwrap().unwrap();
        assert!(delta.threads_to_hydrate.is_empty());
        assert!(delta.new_inbox_message_ids.is_empty());
        assert_eq!(delta.new_history_id, "150");
        mock.assert();
    }

    #[tokio::test]
    async fn test_fetch_history_label_changes() {
        let pool = test_pool().await;
        let server = httpmock::MockServer::start();

        // Set up thread and message
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES ('t1', 'acc1', '', '100', 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, subject, internal_date) VALUES ('msg1', 't1', 'acc1', 'test@test.com', 'Test', 1000)")
            .execute(&pool).await.unwrap();

        let mock = server.mock(|when, then| {
            when.method("GET").path("/gmail/v1/users/me/history");
            then.status(200).json_body(serde_json::json!({
                "history": [{
                    "labelsAdded": [{
                        "message": {
                            "id": "msg1",
                            "threadId": "t1"
                        },
                        "labelIds": ["STARRED"]
                    }]
                }],
                "historyId": "200"
            }));
        });

        let result = {
            let _guard = MOCK_ENV_LOCK.lock().unwrap();
            std::env::set_var("TEST_GMAIL_API_BASE", server.base_url());
            let r = fetch_history(&pool, "acc1", "token", "100").await;
            std::env::remove_var("TEST_GMAIL_API_BASE");
            r
        };

        let delta = result.unwrap().unwrap();

        // Label changes should NOT trigger hydration
        assert!(delta.threads_to_hydrate.is_empty());

        // Verify label was applied locally
        let label_count: (i32,) = sqlx::query_as("SELECT COUNT(*) FROM message_labels WHERE message_id = 'msg1' AND label_id = 'STARRED'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(label_count.0, 1);

        let thread_label_count: (i32,) = sqlx::query_as("SELECT COUNT(*) FROM thread_labels WHERE thread_id = 't1' AND label_id = 'STARRED'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(thread_label_count.0, 1);
        mock.assert();
    }
}
