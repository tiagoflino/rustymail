use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use futures::stream::{self, StreamExt};
use std::sync::Arc;





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

fn get_header<'a>(headers: &'a [MessagePartHeader], name: &str) -> Option<&'a str> {
    headers.iter().find(|h| h.name.eq_ignore_ascii_case(name)).map(|h| h.value.as_str())
}





pub async fn fetch_and_store_labels(pool: &SqlitePool, account_id: &str, access_token: &str) -> Result<(), String> {
    let client = Client::new();
    let res = client
        .get("https://gmail.googleapis.com/gmail/v1/users/me/labels")
        .header("Authorization", format!("Bearer {}", access_token))
        .send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to fetch labels: {}", res.status()));
    }

    let labels_res: LabelsResponse = res.json().await.map_err(|e| e.to_string())?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    for label in labels_res.labels {
        sqlx::query(
            "INSERT INTO labels (id, account_id, name, type, unread_count)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, unread_count=excluded.unread_count"
        )
        .bind(&label.id).bind(account_id).bind(&label.name)
        .bind(&label.r#type).bind(label.messages_unread.unwrap_or(0))
        .execute(&mut *tx).await.map_err(|e| e.to_string())?;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
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
        ("fields", "threads(id,snippet,historyId),nextPageToken".to_string()),
    ];
    if let Some(labels) = &label_ids {
        for lid in labels.iter() {
            params.push(("labelIds", lid.to_string()));
        }
    }

    let res = client
        .get("https://gmail.googleapis.com/gmail/v1/users/me/threads")
        .query(&params)
        .header("Authorization", format!("Bearer {}", access_token))
        .send().await.map_err(|e| e.to_string())?;

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
                    let _ = sqlx::query("INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, ?)")
                        .bind(&thread.id).bind(lid)
                        .execute(&mut *tx).await;
                }
            }
        }
        tx.commit().await.map_err(|e| e.to_string())?;
    }

    Ok(())
}





pub async fn fetch_messages_for_thread(pool: &SqlitePool, account_id: &str, access_token: &str, thread_id: &str) -> Result<(), String> {
    let client = Client::new();
    let res = client
        .get(&format!("https://gmail.googleapis.com/gmail/v1/users/me/threads/{}", thread_id))
        .header("Authorization", format!("Bearer {}", access_token))
        .send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to fetch thread {}: {}", thread_id, res.status()));
    }

    let thread_details: ThreadDetailsResponse = res.json().await.map_err(|e| e.to_string())?;
    store_thread_messages(pool, account_id, &thread_details).await
}

async fn store_thread_messages(pool: &SqlitePool, account_id: &str, thread_details: &ThreadDetailsResponse) -> Result<(), String> {
    if let Some(messages) = &thread_details.messages {
        let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

        for msg in messages {
            let internal_date: i64 = msg.internal_date.parse().unwrap_or(0);
            let mut sender = String::new();
            let mut recipients = String::new();
            let mut subject = String::new();
            let mut body_plain = String::new();
            let mut body_html = String::new();

            if let Some(payload) = &msg.payload {
                if let Some(headers) = &payload.headers {
                    sender = get_header(headers, "From").unwrap_or("").to_string();
                    recipients = get_header(headers, "To").unwrap_or("").to_string();
                    subject = get_header(headers, "Subject").unwrap_or("").to_string();
                }
                body_plain = extract_body(payload, "text/plain").unwrap_or_default();
                body_html = extract_body(payload, "text/html").unwrap_or_default();
            }

            sqlx::query(
                "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(id) DO UPDATE SET sender=excluded.sender, recipients=excluded.recipients, subject=excluded.subject, body_plain=excluded.body_plain, body_html=excluded.body_html"
            )
            .bind(&msg.id).bind(&msg.thread_id).bind(account_id)
            .bind(&sender).bind(&recipients).bind(&subject)
            .bind(msg.snippet.as_deref().unwrap_or("")).bind(internal_date)
            .bind(&body_plain).bind(&body_html).bind(0)
            .execute(&mut *tx).await.map_err(|e| e.to_string())?;

            if let Some(ref label_ids) = msg.label_ids {
                for label_id in label_ids {
                    let _ = sqlx::query("INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, ?)")
                        .bind(&msg.thread_id).bind(label_id).execute(&mut *tx).await;
                    let _ = sqlx::query("INSERT OR IGNORE INTO message_labels (message_id, label_id) VALUES (?, ?)")
                        .bind(&msg.id).bind(label_id).execute(&mut *tx).await;
                }
                if label_ids.contains(&"UNREAD".to_string()) {
                    let _ = sqlx::query("UPDATE threads SET unread = 1 WHERE id = ?")
                        .bind(&msg.thread_id).execute(&mut *tx).await;
                }
            }

            let _ = sqlx::query(
                "INSERT OR REPLACE INTO messages_fts(rowid, sender, subject, body_plain) 
                 SELECT rowid, sender, subject, body_plain FROM messages WHERE id = ?"
            ).bind(&msg.id).execute(&mut *tx).await;
        }

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
                    .get(&format!("https://gmail.googleapis.com/gmail/v1/users/me/threads/{}", tid))
                    .header("Authorization", format!("Bearer {}", token))
                    .send().await.map_err(|e| e.to_string())?;
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
        if r.is_ok() { completed += 1; }
    }

    println!("[Hydrate] Completed {}/{} threads", completed, total);
    (total, completed)
}


pub async fn get_unhydrated_thread_ids(pool: &SqlitePool, account_id: &str) -> Vec<String> {
    #[derive(sqlx::FromRow)]
    struct TId { id: String }
    sqlx::query_as::<_, TId>(
        "SELECT t.id FROM threads t 
         LEFT JOIN messages m ON t.id = m.thread_id 
         WHERE t.account_id = ? AND m.id IS NULL"
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter().map(|r| r.id).collect()
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
                println!("[Cache] Evicted bodies from {} messages (keeping {} recent threads)", r.rows_affected(), max_cached);
            }
        }
        Err(e) => println!("[Cache] Eviction error: {}", e),
    }
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
        .post(&format!("https://gmail.googleapis.com/gmail/v1/users/me/threads/{}/modify", thread_id))
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&payload).send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to modify thread: {}", res.status()));
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    if remove_labels.contains(&"UNREAD".to_string()) {
        sqlx::query("UPDATE threads SET unread = 0 WHERE id = ?")
            .bind(thread_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    } else if add_labels.contains(&"UNREAD".to_string()) {
        sqlx::query("UPDATE threads SET unread = 1 WHERE id = ?")
            .bind(thread_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    }

    if remove_labels.contains(&"STARRED".to_string()) {
        sqlx::query("DELETE FROM thread_labels WHERE thread_id = ? AND label_id = 'STARRED'")
            .bind(thread_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    } else if add_labels.contains(&"STARRED".to_string()) {
        sqlx::query("INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, 'STARRED')")
            .bind(thread_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
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
        .post(&format!("https://gmail.googleapis.com/gmail/v1/users/me/threads/{}/trash", thread_id))
        .header("Authorization", format!("Bearer {}", access_token))
        .send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to trash thread: {}", res.status()));
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM threads WHERE id = ?").bind(thread_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM messages WHERE thread_id = ?").bind(thread_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
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
        let address = Address::from_str(raw)
            .map_err(|e| format!("Invalid To address '{}': {}", raw, e))?;
        Ok(Mailbox::new(None, address))
    }
}

fn build_mime_message(from: &str, to: &str, subject: &str, body: &str) -> Result<String, String> {
    use lettre::message::{Message, header::ContentType, Mailbox};
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

    if recipients.is_empty() {
        return Err("No valid recipients".to_string());
    }

    let mut builder = Message::builder()
        .from(from_mailbox)
        .subject(subject);
    for mailbox in recipients {
        builder = builder.to(mailbox);
    }
    let email = builder
        .header(ContentType::TEXT_HTML)
        .body(body.to_string())
        .map_err(|e| e.to_string())?;

    let formatted = email.formatted();
    Ok(base64::encode_config(formatted, base64::URL_SAFE_NO_PAD))
}

pub async fn send_message(
    _account_id: &str,
    account_email: &str,
    access_token: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let raw = build_mime_message(account_email, to, subject, body)?;
    let client = reqwest::Client::new();
    let body_json = serde_json::json!({ "raw": raw });

    let res = client
        .post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send")
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&body_json)
        .send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to send email: {}", res.status()));
    }
    Ok(())
}

pub async fn save_draft(
    _account_id: &str,
    account_email: &str,
    access_token: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let raw = build_mime_message(account_email, to, subject, body)?;
    let client = reqwest::Client::new();
    let message_json = serde_json::json!({ "raw": raw });
    let body_json = serde_json::json!({ "message": message_json });

    let res = client
        .post("https://gmail.googleapis.com/gmail/v1/users/me/drafts")
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&body_json)
        .send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to save draft: {}", res.status()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use tempfile::tempdir;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::SqlitePool;

    #[test]
    fn test_get_header() {
        let headers = vec![
            MessagePartHeader { name: "Subject".to_string(), value: "Hello".to_string() },
            MessagePartHeader { name: "From".to_string(), value: "me@example.com".to_string() },
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
            }),
            parts: None,
        };
        assert_eq!(extract_body(&part, "text/plain"), Some("Hello World".to_string()));
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
        let res = build_mime_message("me@test.com", "you@test.com", "Hi", "Body");
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
            "Body"
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
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.to_string_lossy()))
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

        let thread_details = ThreadDetailsResponse {
            id: "t1".to_string(),
            messages: Some(vec![
                GmailMessage {
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
                            MessagePartHeader { name: "From".to_string(), value: "sender@test.com".to_string() },
                            MessagePartHeader { name: "To".to_string(), value: "me@test.com".to_string() },
                            MessagePartHeader { name: "Subject".to_string(), value: "Hello".to_string() },
                        ]),
                        body: Some(MessagePartBody { size: 5, data: Some("SGVsbG8=".to_string()) }), // "Hello"
                        parts: None,
                    }),
                }
            ]),
        };

        store_thread_messages(&pool, "acc1", &thread_details).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages").fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 1);

        let msg: (String, String) = sqlx::query_as("SELECT sender, body_plain FROM messages WHERE id='m1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(msg.0, "sender@test.com");
        assert_eq!(msg.1, "Hello");

        let labels: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM message_labels WHERE message_id='m1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(labels.0, 2);
    }
}
