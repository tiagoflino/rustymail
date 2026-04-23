use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::calendar_api::{CalendarDateTime, CalendarEvent, NewCalendarEvent};
use crate::email_utils::{sanitize_email_html, AttachmentFile};

// ---------------------------------------------------------------------------
// Base URL helper (mirrors gmail_api_url pattern)
// ---------------------------------------------------------------------------

fn graph_api_url(path: &str) -> String {
    #[cfg(test)]
    {
        let base = std::env::var("TEST_GRAPH_API_BASE")
            .unwrap_or_else(|_| "https://graph.microsoft.com".to_string());
        format!("{}{}", base, path)
    }
    #[cfg(not(test))]
    {
        format!("https://graph.microsoft.com{}", path)
    }
}

fn graph_request(
    client: &Client,
    method: reqwest::Method,
    path: &str,
    access_token: &str,
) -> reqwest::RequestBuilder {
    client
        .request(method, graph_api_url(path))
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Prefer", "IdType=\"ImmutableId\"")
}

// ---------------------------------------------------------------------------
// Response structs
// ---------------------------------------------------------------------------

#[derive(Deserialize, Debug)]
struct GraphMessage {
    id: String,
    subject: Option<String>,
    #[serde(rename = "bodyPreview")]
    body_preview: Option<String>,
    #[serde(rename = "conversationId")]
    conversation_id: Option<String>,
    #[serde(rename = "receivedDateTime")]
    received_date_time: Option<String>,
    #[serde(rename = "isRead")]
    is_read: Option<bool>,
    #[serde(rename = "hasAttachments")]
    has_attachments: Option<bool>,
    from: Option<GraphEmailAddress>,
    #[allow(dead_code)]
    sender: Option<GraphEmailAddress>,
    #[serde(rename = "toRecipients")]
    to_recipients: Option<Vec<GraphRecipient>>,
    flag: Option<GraphFlag>,
    #[serde(rename = "parentFolderId")]
    #[allow(dead_code)]
    parent_folder_id: Option<String>,
    body: Option<GraphBody>,
    #[allow(dead_code)]
    importance: Option<String>,
    #[serde(rename = "@removed")]
    removed: Option<GraphRemovedAnnotation>,
}

#[derive(Deserialize, Debug)]
struct GraphEmailAddress {
    #[serde(rename = "emailAddress")]
    email_address: GraphEmailInfo,
}

#[derive(Deserialize, Debug)]
struct GraphEmailInfo {
    name: Option<String>,
    address: Option<String>,
}

#[derive(Deserialize, Debug)]
struct GraphRecipient {
    #[serde(rename = "emailAddress")]
    email_address: GraphEmailInfo,
}

#[derive(Deserialize, Debug)]
struct GraphFlag {
    #[serde(rename = "flagStatus")]
    flag_status: Option<String>,
}

#[derive(Deserialize, Debug)]
struct GraphBody {
    #[serde(rename = "contentType")]
    content_type: Option<String>,
    content: Option<String>,
}

#[derive(Deserialize, Debug)]
struct GraphFolder {
    id: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "unreadItemCount")]
    #[allow(dead_code)]
    unread_item_count: Option<i32>,
    #[serde(rename = "totalItemCount")]
    #[allow(dead_code)]
    total_item_count: Option<i32>,
}

#[derive(Deserialize, Debug)]
struct GraphDeltaResponse {
    value: Vec<GraphMessage>,
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
    #[serde(rename = "@odata.deltaLink")]
    delta_link: Option<String>,
}

#[derive(Deserialize, Debug)]
struct GraphFoldersResponse {
    value: Vec<GraphFolder>,
}

#[derive(Deserialize, Debug)]
struct GraphRemovedAnnotation {
    #[allow(dead_code)]
    reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Sync result type
// ---------------------------------------------------------------------------

pub struct OutlookSyncDelta {
    pub new_thread_ids: Vec<String>,
    pub updated_thread_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// ID helpers
// ---------------------------------------------------------------------------

pub fn extract_graph_id(prefixed_id: &str) -> Result<&str, String> {
    prefixed_id
        .strip_prefix("outlook:")
        .ok_or_else(|| format!("Invalid Outlook ID format: {}", prefixed_id))
}

fn make_thread_id(account_id: &str, conversation_id: &str) -> String {
    format!("outlook:{}:{}", account_id, conversation_id)
}

fn make_message_id(graph_id: &str) -> String {
    format!("outlook:{}", graph_id)
}

/// Extract the raw Graph API message ID from a prefixed message id.
/// Handles both `outlook:{graph_id}` format.
fn extract_message_graph_id(prefixed_id: &str) -> Result<&str, String> {
    prefixed_id
        .strip_prefix("outlook:")
        .ok_or_else(|| format!("Invalid Outlook message ID format: {}", prefixed_id))
}

// ---------------------------------------------------------------------------
// DB helpers for delta sync state
// ---------------------------------------------------------------------------

async fn get_outlook_delta_link(
    pool: &SqlitePool,
    account_id: &str,
    folder_id: &str,
) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT delta_link FROM outlook_sync_state WHERE account_id = ? AND folder_id = ?",
    )
    .bind(account_id)
    .bind(folder_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

async fn set_outlook_delta_link(
    pool: &SqlitePool,
    account_id: &str,
    folder_id: &str,
    delta_link: &str,
) {
    let _ = sqlx::query(
        "INSERT INTO outlook_sync_state (account_id, folder_id, delta_link) VALUES (?, ?, ?)
         ON CONFLICT(account_id, folder_id) DO UPDATE SET delta_link = excluded.delta_link",
    )
    .bind(account_id)
    .bind(folder_id)
    .bind(delta_link)
    .execute(pool)
    .await;
}

// ---------------------------------------------------------------------------
// Helper: get all Graph message IDs for an Outlook thread
// ---------------------------------------------------------------------------

async fn get_outlook_thread_messages(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<String>, String> {
    let prefixed_ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM messages WHERE thread_id = ?")
            .bind(thread_id)
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;

    let mut graph_ids = Vec::new();
    for pid in &prefixed_ids {
        if let Ok(gid) = extract_message_graph_id(pid) {
            graph_ids.push(gid.to_string());
        }
    }
    Ok(graph_ids)
}

// ---------------------------------------------------------------------------
// Folder name mapping
// ---------------------------------------------------------------------------

fn map_folder_to_label(display_name: &str) -> String {
    match display_name.to_lowercase().as_str() {
        "inbox" => "INBOX".to_string(),
        "sent items" | "sentitems" => "SENT".to_string(),
        "drafts" => "DRAFT".to_string(),
        "deleted items" | "deleteditems" => "TRASH".to_string(),
        "junk email" | "junkemail" => "SPAM".to_string(),
        "archive" => "ARCHIVE".to_string(),
        _ => format!("outlook:{}", display_name),
    }
}

// ---------------------------------------------------------------------------
// 1. Fetch and store folders as labels
// ---------------------------------------------------------------------------

pub async fn fetch_and_store_outlook_folders(
    pool: &SqlitePool,
    account_id: &str,
    access_token: &str,
) -> Result<(), String> {
    let client = Client::new();
    let res = graph_request(
        &client,
        reqwest::Method::GET,
        "/v1.0/me/mailFolders",
        access_token,
    )
    .query(&[("$top", "100")])
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to fetch Outlook folders: {}", res.status()));
    }

    let folders_res: GraphFoldersResponse = res.json().await.map_err(|e| e.to_string())?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    for folder in &folders_res.value {
        let display_name = folder.display_name.as_deref().unwrap_or(&folder.id);
        let label_id = map_folder_to_label(display_name);

        sqlx::query(
            "INSERT INTO labels (id, account_id, name, type, unread_count, threads_total, threads_unread)
             VALUES (?, ?, ?, 'system', ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, unread_count=excluded.unread_count, threads_total=excluded.threads_total",
        )
        .bind(&label_id)
        .bind(account_id)
        .bind(display_name)
        .bind(folder.unread_item_count.unwrap_or(0))
        .bind(folder.total_item_count.unwrap_or(0))
        .bind(folder.unread_item_count.unwrap_or(0))
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 2. Delta sync
// ---------------------------------------------------------------------------

fn parse_received_datetime(dt_str: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(dt_str)
        .map(|d| d.timestamp_millis())
        .unwrap_or(0)
}

pub async fn outlook_delta_sync(
    pool: &SqlitePool,
    account_id: &str,
    access_token: &str,
    folder_id: &str,
) -> Result<OutlookSyncDelta, String> {
    let client = Client::new();
    let existing_delta = get_outlook_delta_link(pool, account_id, folder_id).await;

    let mut all_messages: Vec<GraphMessage> = Vec::new();
    let mut final_delta_link: Option<String> = None;

    // First request: either resume from delta link or start fresh
    let first_response = if let Some(ref link) = existing_delta {
        // Incremental: delta link is a full URL from the API
        client
            .get(link)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Prefer", "IdType=\"ImmutableId\"")
            .send()
            .await
            .map_err(|e| e.to_string())?
    } else {
        // Initial sync: construct with proper query params
        let delta_select = "subject,from,sender,receivedDateTime,conversationId,isRead,hasAttachments,bodyPreview,flag,importance,parentFolderId";
        graph_request(
            &client,
            reqwest::Method::GET,
            &format!("/v1.0/me/mailFolders/{}/messages/delta", folder_id),
            access_token,
        )
        .query(&[("$select", delta_select), ("$top", "100")])
        .send()
        .await
        .map_err(|e| e.to_string())?
    };

    if !first_response.status().is_success() {
        let status = first_response.status();
        if status.as_u16() == 410 {
            set_outlook_delta_link(pool, account_id, folder_id, "").await;
            return Box::pin(outlook_delta_sync(pool, account_id, access_token, folder_id))
                .await;
        }
        return Err(format!("Outlook delta sync error: {}", status));
    }

    let first_delta: GraphDeltaResponse =
        first_response.json().await.map_err(|e| e.to_string())?;
    all_messages.extend(first_delta.value);

    let mut next_url = first_delta.next_link;
    if next_url.is_none() {
        final_delta_link = first_delta.delta_link;
    }

    // Follow pagination links
    while let Some(url) = next_url.take() {
        let res = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Prefer", "IdType=\"ImmutableId\"")
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !res.status().is_success() {
            let status = res.status();
            if status.as_u16() == 410 {
                set_outlook_delta_link(pool, account_id, folder_id, "").await;
                return Box::pin(outlook_delta_sync(pool, account_id, access_token, folder_id))
                    .await;
            }
            return Err(format!("Outlook delta sync error: {}", status));
        }

        let delta_res: GraphDeltaResponse = res.json().await.map_err(|e| e.to_string())?;
        all_messages.extend(delta_res.value);

        if let Some(next) = delta_res.next_link {
            next_url = Some(next);
        } else {
            final_delta_link = delta_res.delta_link;
        }
    }

    // Persist delta link
    if let Some(ref link) = final_delta_link {
        set_outlook_delta_link(pool, account_id, folder_id, link).await;
    }

    // Process messages
    let mut new_thread_ids: Vec<String> = Vec::new();
    let mut updated_thread_ids: Vec<String> = Vec::new();

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    for msg in &all_messages {
        let conv_id = msg
            .conversation_id
            .as_deref()
            .unwrap_or(&msg.id);
        let thread_id = make_thread_id(account_id, conv_id);
        let message_id = make_message_id(&msg.id);

        if msg.removed.is_some() {
            // Message was deleted remotely
            sqlx::query("DELETE FROM messages WHERE id = ?")
                .bind(&message_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;

            // Check if thread still has messages
            let remaining: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM messages WHERE thread_id = ?",
            )
            .bind(&thread_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

            if remaining == 0 {
                sqlx::query("DELETE FROM threads WHERE id = ?")
                    .bind(&thread_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM thread_labels WHERE thread_id = ?")
                    .bind(&thread_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            updated_thread_ids.push(thread_id);
            continue;
        }

        // Determine sender
        let sender = msg
            .from
            .as_ref()
            .map(|f| {
                let name = f.email_address.name.as_deref().unwrap_or("");
                let addr = f.email_address.address.as_deref().unwrap_or("");
                if name.is_empty() {
                    addr.to_string()
                } else {
                    format!("{} <{}>", name, addr)
                }
            })
            .unwrap_or_default();

        let recipients = msg
            .to_recipients
            .as_ref()
            .map(|recips| {
                recips
                    .iter()
                    .map(|r| {
                        let addr = r.email_address.address.as_deref().unwrap_or("");
                        let name = r.email_address.name.as_deref().unwrap_or("");
                        if name.is_empty() {
                            addr.to_string()
                        } else {
                            format!("{} <{}>", name, addr)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();

        let subject = msg.subject.as_deref().unwrap_or("").to_string();
        let snippet = msg.body_preview.as_deref().unwrap_or("").to_string();
        let internal_date = msg
            .received_date_time
            .as_deref()
            .map(parse_received_datetime)
            .unwrap_or(0);
        let is_read = msg.is_read.unwrap_or(true);
        let has_att = msg.has_attachments.unwrap_or(false);

        let is_flagged = msg
            .flag
            .as_ref()
            .and_then(|f| f.flag_status.as_deref())
            .map(|s| s == "flagged")
            .unwrap_or(false);

        // Body content (only available if delta returns it, usually not for metadata-only)
        let body_html = msg
            .body
            .as_ref()
            .filter(|b| {
                b.content_type
                    .as_deref()
                    .map(|ct| ct.eq_ignore_ascii_case("html"))
                    .unwrap_or(false)
            })
            .and_then(|b| b.content.as_deref())
            .map(sanitize_email_html)
            .unwrap_or_default();

        let body_plain = msg
            .body
            .as_ref()
            .filter(|b| {
                b.content_type
                    .as_deref()
                    .map(|ct| ct.eq_ignore_ascii_case("text"))
                    .unwrap_or(false)
            })
            .and_then(|b| b.content.as_deref())
            .unwrap_or("")
            .to_string();

        // Check if thread already exists
        let thread_exists: bool = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM threads WHERE id = ?",
        )
        .bind(&thread_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?
            > 0;

        // Upsert thread
        sqlx::query(
            "INSERT INTO threads (id, account_id, snippet, history_id, unread, sender, subject, latest_date, metadata_synced)
             VALUES (?, ?, ?, '', ?, ?, ?, ?, 1)
             ON CONFLICT(id) DO UPDATE SET
                snippet = CASE WHEN excluded.latest_date > COALESCE(threads.latest_date, 0) THEN excluded.snippet ELSE threads.snippet END,
                sender = CASE WHEN excluded.latest_date > COALESCE(threads.latest_date, 0) THEN excluded.sender ELSE threads.sender END,
                subject = CASE WHEN excluded.latest_date > COALESCE(threads.latest_date, 0) THEN excluded.subject ELSE threads.subject END,
                latest_date = MAX(COALESCE(threads.latest_date, 0), excluded.latest_date),
                unread = CASE WHEN excluded.unread = 1 THEN 1 ELSE threads.unread END,
                metadata_synced = 1",
        )
        .bind(&thread_id)
        .bind(account_id)
        .bind(&snippet)
        .bind(if is_read { 0 } else { 1 })
        .bind(&sender)
        .bind(&subject)
        .bind(internal_date)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        // Upsert message
        sqlx::query(
            "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                sender=excluded.sender, recipients=excluded.recipients, subject=excluded.subject,
                snippet=excluded.snippet, body_plain=CASE WHEN excluded.body_plain != '' THEN excluded.body_plain ELSE messages.body_plain END,
                body_html=CASE WHEN excluded.body_html != '' THEN excluded.body_html ELSE messages.body_html END,
                has_attachments=excluded.has_attachments",
        )
        .bind(&message_id)
        .bind(&thread_id)
        .bind(account_id)
        .bind(&sender)
        .bind(&recipients)
        .bind(&subject)
        .bind(&snippet)
        .bind(internal_date)
        .bind(&body_plain)
        .bind(&body_html)
        .bind(if has_att { 1 } else { 0 })
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        // Thread labels: INBOX for inbox folder
        let label = map_folder_to_label(folder_id);
        sqlx::query("INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, ?)")
            .bind(&thread_id)
            .bind(&label)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        // Starred (flagged)
        if is_flagged {
            sqlx::query(
                "INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, 'STARRED')",
            )
            .bind(&thread_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        }

        // FTS index
        let _ = sqlx::query(
            "INSERT OR REPLACE INTO messages_fts(rowid, sender, subject, body_plain)
             SELECT rowid, sender, subject, body_plain FROM messages WHERE id = ?",
        )
        .bind(&message_id)
        .execute(&mut *tx)
        .await;

        if thread_exists {
            updated_thread_ids.push(thread_id);
        } else {
            new_thread_ids.push(thread_id);
        }
    }

    tx.commit().await.map_err(|e| e.to_string())?;

    // Deduplicate
    new_thread_ids.sort();
    new_thread_ids.dedup();
    updated_thread_ids.sort();
    updated_thread_ids.dedup();

    Ok(OutlookSyncDelta {
        new_thread_ids,
        updated_thread_ids,
    })
}

// ---------------------------------------------------------------------------
// 3. Fetch message body
// ---------------------------------------------------------------------------

pub async fn fetch_outlook_message_body(
    pool: &SqlitePool,
    access_token: &str,
    message_id: &str,
) -> Result<(), String> {
    let graph_id = extract_message_graph_id(message_id)?;
    let client = Client::new();

    let res = graph_request(
        &client,
        reqwest::Method::GET,
        &format!(
            "/v1.0/me/messages/{}?$select=body,uniqueBody",
            graph_id
        ),
        access_token,
    )
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!(
            "Failed to fetch Outlook message body: {}",
            res.status()
        ));
    }

    #[derive(Deserialize)]
    struct BodyResponse {
        body: Option<GraphBody>,
    }

    let body_res: BodyResponse = res.json().await.map_err(|e| e.to_string())?;

    let (body_html, body_plain) = if let Some(body) = body_res.body {
        let content = body.content.unwrap_or_default();
        let ct = body
            .content_type
            .as_deref()
            .unwrap_or("text");
        if ct.eq_ignore_ascii_case("html") {
            (sanitize_email_html(&content), String::new())
        } else {
            (String::new(), content)
        }
    } else {
        (String::new(), String::new())
    };

    sqlx::query(
        "UPDATE messages SET body_html = ?, body_plain = ? WHERE id = ?",
    )
    .bind(&body_html)
    .bind(&body_plain)
    .bind(message_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

// ---------------------------------------------------------------------------
// 4. Mark read/unread
// ---------------------------------------------------------------------------

pub async fn outlook_mark_read(
    pool: &SqlitePool,
    access_token: &str,
    thread_id: &str,
    is_read: bool,
) -> Result<(), String> {
    let graph_ids = get_outlook_thread_messages(pool, thread_id).await?;
    let client = Client::new();

    for gid in &graph_ids {
        let res = graph_request(
            &client,
            reqwest::Method::PATCH,
            &format!("/v1.0/me/messages/{}", gid),
            access_token,
        )
        .json(&serde_json::json!({"isRead": is_read}))
        .send()
        .await
        .map_err(|e| e.to_string())?;

        if !res.status().is_success() {
            tracing::error!(
                "Failed to mark message {} as read={}: {}",
                gid,
                is_read,
                res.status()
            );
        }
    }

    // Update locally
    let unread_val = if is_read { 0 } else { 1 };
    sqlx::query("UPDATE threads SET unread = ? WHERE id = ?")
        .bind(unread_val)
        .bind(thread_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

// ---------------------------------------------------------------------------
// 5. Set star (flag)
// ---------------------------------------------------------------------------

pub async fn outlook_set_star(
    pool: &SqlitePool,
    access_token: &str,
    thread_id: &str,
    starred: bool,
) -> Result<(), String> {
    let graph_ids = get_outlook_thread_messages(pool, thread_id).await?;
    let client = Client::new();

    let flag_status = if starred { "flagged" } else { "notFlagged" };
    let body = serde_json::json!({"flag": {"flagStatus": flag_status}});

    for gid in &graph_ids {
        let res = graph_request(
            &client,
            reqwest::Method::PATCH,
            &format!("/v1.0/me/messages/{}", gid),
            access_token,
        )
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

        if !res.status().is_success() {
            tracing::error!(
                "Failed to set flag on message {}: {}",
                gid,
                res.status()
            );
        }
    }

    // Update locally
    if starred {
        sqlx::query(
            "INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, 'STARRED')",
        )
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

// ---------------------------------------------------------------------------
// 6. Archive thread (move out of Inbox)
// ---------------------------------------------------------------------------

pub async fn outlook_archive_thread(
    pool: &SqlitePool,
    access_token: &str,
    thread_id: &str,
) -> Result<(), String> {
    let graph_ids = get_outlook_thread_messages(pool, thread_id).await?;
    let client = Client::new();

    for gid in &graph_ids {
        let res = graph_request(
            &client,
            reqwest::Method::POST,
            &format!("/v1.0/me/messages/{}/move", gid),
            access_token,
        )
        .json(&serde_json::json!({"destinationId": "archive"}))
        .send()
        .await
        .map_err(|e| e.to_string())?;

        if !res.status().is_success() {
            tracing::error!(
                "Failed to archive message {}: {}",
                gid,
                res.status()
            );
        }
    }

    // Remove INBOX label locally
    sqlx::query("DELETE FROM thread_labels WHERE thread_id = ? AND label_id = 'INBOX'")
        .bind(thread_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

// ---------------------------------------------------------------------------
// 7. Trash thread
// ---------------------------------------------------------------------------

pub async fn outlook_trash_thread(
    pool: &SqlitePool,
    access_token: &str,
    thread_id: &str,
) -> Result<(), String> {
    let graph_ids = get_outlook_thread_messages(pool, thread_id).await?;
    let client = Client::new();

    for gid in &graph_ids {
        let res = graph_request(
            &client,
            reqwest::Method::POST,
            &format!("/v1.0/me/messages/{}/move", gid),
            access_token,
        )
        .json(&serde_json::json!({"destinationId": "deleteditems"}))
        .send()
        .await
        .map_err(|e| e.to_string())?;

        if !res.status().is_success() {
            tracing::error!(
                "Failed to trash message {}: {}",
                gid,
                res.status()
            );
        }
    }

    // Clean up locally (mirrors gmail trash behavior)
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

// ---------------------------------------------------------------------------
// 8. Send message
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct OutlookSendMailRequest {
    message: OutlookMessagePayload,
}

#[derive(Serialize)]
struct OutlookMessagePayload {
    subject: String,
    body: OutlookBodyPayload,
    #[serde(rename = "toRecipients")]
    to_recipients: Vec<OutlookRecipientPayload>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachments: Vec<OutlookAttachmentPayload>,
}

#[derive(Serialize)]
struct OutlookBodyPayload {
    #[serde(rename = "contentType")]
    content_type: String,
    content: String,
}

#[derive(Serialize)]
struct OutlookRecipientPayload {
    #[serde(rename = "emailAddress")]
    email_address: OutlookEmailPayload,
}

#[derive(Serialize)]
struct OutlookEmailPayload {
    address: String,
}

#[derive(Serialize)]
struct OutlookAttachmentPayload {
    #[serde(rename = "@odata.type")]
    odata_type: String,
    name: String,
    #[serde(rename = "contentType")]
    content_type: String,
    #[serde(rename = "contentBytes")]
    content_bytes: String,
}

fn parse_recipients(to: &str) -> Vec<OutlookRecipientPayload> {
    to.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            // Handle "Name <email>" format
            let addr = if let Some(start) = s.find('<') {
                s[start + 1..].trim_end_matches('>').trim().to_string()
            } else {
                s.to_string()
            };
            OutlookRecipientPayload {
                email_address: OutlookEmailPayload { address: addr },
            }
        })
        .collect()
}

pub async fn outlook_send_message(
    access_token: &str,
    to: &str,
    subject: &str,
    body_html: &str,
    attachments: &[AttachmentFile],
) -> Result<(), String> {
    let client = Client::new();

    let att_payloads: Vec<OutlookAttachmentPayload> = attachments
        .iter()
        .map(|a| OutlookAttachmentPayload {
            odata_type: "#microsoft.graph.fileAttachment".to_string(),
            name: a.filename.clone(),
            content_type: a.mime_type.clone(),
            content_bytes: base64::encode(&a.data),
        })
        .collect();

    let payload = OutlookSendMailRequest {
        message: OutlookMessagePayload {
            subject: subject.to_string(),
            body: OutlookBodyPayload {
                content_type: "HTML".to_string(),
                content: body_html.to_string(),
            },
            to_recipients: parse_recipients(to),
            attachments: att_payloads,
        },
    };

    let res = graph_request(
        &client,
        reqwest::Method::POST,
        "/v1.0/me/sendMail",
        access_token,
    )
    .json(&payload)
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Failed to send Outlook message: {}", body));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 9. Save draft
// ---------------------------------------------------------------------------

pub async fn outlook_save_draft(
    pool: &SqlitePool,
    access_token: &str,
    to: &str,
    subject: &str,
    body_html: &str,
    draft_id: Option<&str>,
) -> Result<String, String> {
    let client = Client::new();

    let payload = serde_json::json!({
        "subject": subject,
        "body": {
            "contentType": "HTML",
            "content": body_html
        },
        "toRecipients": parse_recipients(to).iter().map(|r| {
            serde_json::json!({"emailAddress": {"address": &r.email_address.address}})
        }).collect::<Vec<_>>()
    });

    let res = if let Some(did) = draft_id {
        let graph_id = extract_message_graph_id(did)?;
        graph_request(
            &client,
            reqwest::Method::PATCH,
            &format!("/v1.0/me/messages/{}", graph_id),
            access_token,
        )
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?
    } else {
        graph_request(
            &client,
            reqwest::Method::POST,
            "/v1.0/me/messages",
            access_token,
        )
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?
    };

    if !res.status().is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Failed to save Outlook draft: {}", body));
    }

    #[derive(Deserialize)]
    struct DraftResponse {
        id: String,
    }

    let draft: DraftResponse = res.json().await.map_err(|e| e.to_string())?;
    let prefixed_id = make_message_id(&draft.id);

    // Store draft metadata locally
    let _ = sqlx::query(
        "INSERT OR REPLACE INTO drafts (id, account_id, to_field, subject, body_html, created_at)
         VALUES (?, (SELECT id FROM accounts WHERE is_active = 1 LIMIT 1), ?, ?, ?, ?)",
    )
    .bind(&prefixed_id)
    .bind(to)
    .bind(subject)
    .bind(body_html)
    .bind(chrono::Utc::now().timestamp())
    .execute(pool)
    .await;

    Ok(prefixed_id)
}

// ---------------------------------------------------------------------------
// 10. Send draft
// ---------------------------------------------------------------------------

pub async fn outlook_send_draft(
    access_token: &str,
    draft_id: &str,
) -> Result<(), String> {
    let graph_id = extract_message_graph_id(draft_id)?;
    let client = Client::new();

    let res = graph_request(
        &client,
        reqwest::Method::POST,
        &format!("/v1.0/me/messages/{}/send", graph_id),
        access_token,
    )
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Failed to send Outlook draft: {}", body));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 11. Delete draft
// ---------------------------------------------------------------------------

pub async fn outlook_delete_draft(
    access_token: &str,
    draft_id: &str,
) -> Result<(), String> {
    let graph_id = extract_message_graph_id(draft_id)?;
    let client = Client::new();

    let res = graph_request(
        &client,
        reqwest::Method::DELETE,
        &format!("/v1.0/me/messages/{}", graph_id),
        access_token,
    )
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!(
            "Failed to delete Outlook draft: {}",
            res.status()
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 12. Download attachment
// ---------------------------------------------------------------------------

pub async fn outlook_download_attachment(
    access_token: &str,
    message_id: &str,
    attachment_id: &str,
) -> Result<Vec<u8>, String> {
    let msg_graph_id = extract_message_graph_id(message_id)?;
    let client = Client::new();

    let res = graph_request(
        &client,
        reqwest::Method::GET,
        &format!(
            "/v1.0/me/messages/{}/attachments/{}",
            msg_graph_id, attachment_id
        ),
        access_token,
    )
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!(
            "Failed to download Outlook attachment: {}",
            res.status()
        ));
    }

    #[derive(Deserialize)]
    struct AttachmentResponse {
        #[serde(rename = "contentBytes")]
        content_bytes: Option<String>,
    }

    let att: AttachmentResponse = res.json().await.map_err(|e| e.to_string())?;
    let content = att
        .content_bytes
        .ok_or("No contentBytes in attachment response")?;

    base64::decode(&content).map_err(|e| format!("Failed to decode attachment: {}", e))
}

// ---------------------------------------------------------------------------
// 13. Calendar
// ---------------------------------------------------------------------------

pub async fn outlook_get_events(
    access_token: &str,
    start: &str,
    end: &str,
) -> Result<Vec<CalendarEvent>, String> {
    let client = Client::new();
    let path = format!(
        "/v1.0/me/calendarView?startDateTime={}&endDateTime={}&$select=id,subject,start,end,location,isAllDay,organizer,attendees,bodyPreview&$top=250&$orderby=start/dateTime",
        urlencoding::encode(start),
        urlencoding::encode(end)
    );
    let res = graph_request(&client, reqwest::Method::GET, &path, access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("Calendar fetch failed: {}", res.status()));
    }
    let body: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let events = body["value"].as_array().cloned().unwrap_or_default();

    let mapped: Vec<CalendarEvent> = events
        .iter()
        .map(|e| CalendarEvent {
            id: e["id"].as_str().unwrap_or_default().to_string(),
            summary: e["subject"].as_str().map(String::from),
            start: Some(CalendarDateTime {
                date: None,
                date_time: e["start"]["dateTime"].as_str().map(String::from),
                time_zone: e["start"]["timeZone"].as_str().map(String::from),
            }),
            end: Some(CalendarDateTime {
                date: None,
                date_time: e["end"]["dateTime"].as_str().map(String::from),
                time_zone: e["end"]["timeZone"].as_str().map(String::from),
            }),
            location: e["location"]["displayName"].as_str().map(String::from),
            description: e["bodyPreview"].as_str().map(String::from),
            html_link: None,
            hangout_link: None,
        })
        .collect();
    Ok(mapped)
}

pub async fn outlook_create_event(
    access_token: &str,
    event: &NewCalendarEvent,
) -> Result<CalendarEvent, String> {
    let client = Client::new();
    let graph_event = serde_json::json!({
        "subject": event.summary.as_deref().unwrap_or(""),
        "start": {
            "dateTime": event.start.as_ref().and_then(|s| s.date_time.as_deref()).unwrap_or(""),
            "timeZone": event.start.as_ref().and_then(|s| s.time_zone.as_deref()).unwrap_or("UTC"),
        },
        "end": {
            "dateTime": event.end.as_ref().and_then(|s| s.date_time.as_deref()).unwrap_or(""),
            "timeZone": event.end.as_ref().and_then(|s| s.time_zone.as_deref()).unwrap_or("UTC"),
        },
        "location": { "displayName": event.location.as_deref().unwrap_or("") },
        "body": { "contentType": "Text", "content": event.description.as_deref().unwrap_or("") },
    });

    let res = graph_request(&client, reqwest::Method::POST, "/v1.0/me/events", access_token)
        .json(&graph_event)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("Create event failed: {}", res.status()));
    }
    let created: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(CalendarEvent {
        id: created["id"].as_str().unwrap_or_default().to_string(),
        summary: created["subject"].as_str().map(String::from),
        start: Some(CalendarDateTime {
            date: None,
            date_time: created["start"]["dateTime"].as_str().map(String::from),
            time_zone: created["start"]["timeZone"].as_str().map(String::from),
        }),
        end: Some(CalendarDateTime {
            date: None,
            date_time: created["end"]["dateTime"].as_str().map(String::from),
            time_zone: created["end"]["timeZone"].as_str().map(String::from),
        }),
        location: None,
        description: None,
        html_link: None,
        hangout_link: None,
    })
}

pub async fn outlook_update_event(
    access_token: &str,
    event_id: &str,
    event: &NewCalendarEvent,
) -> Result<CalendarEvent, String> {
    let client = Client::new();
    let graph_event = serde_json::json!({
        "subject": event.summary.as_deref().unwrap_or(""),
        "start": {
            "dateTime": event.start.as_ref().and_then(|s| s.date_time.as_deref()).unwrap_or(""),
            "timeZone": event.start.as_ref().and_then(|s| s.time_zone.as_deref()).unwrap_or("UTC"),
        },
        "end": {
            "dateTime": event.end.as_ref().and_then(|s| s.date_time.as_deref()).unwrap_or(""),
            "timeZone": event.end.as_ref().and_then(|s| s.time_zone.as_deref()).unwrap_or("UTC"),
        },
    });

    let path = format!("/v1.0/me/events/{}", event_id);
    let res = graph_request(&client, reqwest::Method::PATCH, &path, access_token)
        .json(&graph_event)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("Update event failed: {}", res.status()));
    }
    let updated: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(CalendarEvent {
        id: updated["id"].as_str().unwrap_or_default().to_string(),
        summary: updated["subject"].as_str().map(String::from),
        start: Some(CalendarDateTime {
            date: None,
            date_time: updated["start"]["dateTime"].as_str().map(String::from),
            time_zone: updated["start"]["timeZone"].as_str().map(String::from),
        }),
        end: Some(CalendarDateTime {
            date: None,
            date_time: updated["end"]["dateTime"].as_str().map(String::from),
            time_zone: updated["end"]["timeZone"].as_str().map(String::from),
        }),
        location: None,
        description: None,
        html_link: None,
        hangout_link: None,
    })
}

pub async fn outlook_delete_event(
    access_token: &str,
    event_id: &str,
) -> Result<(), String> {
    let client = Client::new();
    let path = format!("/v1.0/me/events/{}", event_id);
    let res = graph_request(&client, reqwest::Method::DELETE, &path, access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("Delete event failed: {}", res.status()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_helpers::setup_test_db;

    // Mock tests share a process-global env var (TEST_GRAPH_API_BASE).
    // A mutex ensures they don't race each other.
    static MOCK_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_graph_api_url() {
        let _guard = MOCK_ENV_LOCK.lock().unwrap();
        std::env::remove_var("TEST_GRAPH_API_BASE");
        let url = graph_api_url("/v1.0/me/messages");
        assert_eq!(url, "https://graph.microsoft.com/v1.0/me/messages");

        std::env::set_var("TEST_GRAPH_API_BASE", "http://localhost:1234");
        let url = graph_api_url("/v1.0/me/messages");
        assert_eq!(url, "http://localhost:1234/v1.0/me/messages");
        std::env::remove_var("TEST_GRAPH_API_BASE");
    }

    #[test]
    fn test_extract_graph_id() {
        assert_eq!(
            extract_graph_id("outlook:AAkALgAAAAAAHYQ").unwrap(),
            "AAkALgAAAAAAHYQ"
        );
        assert!(extract_graph_id("gmail:abc123").is_err());
        assert!(extract_graph_id("noprefixid").is_err());
    }

    #[test]
    fn test_outlook_thread_id_format() {
        let tid = make_thread_id("user@outlook.com", "conv123");
        assert_eq!(tid, "outlook:user@outlook.com:conv123");
        assert!(tid.starts_with("outlook:"));
        assert!(tid.contains("user@outlook.com"));
        assert!(tid.contains("conv123"));
    }

    #[test]
    fn test_make_message_id() {
        let mid = make_message_id("AAMkAGI2TG93AAA=");
        assert_eq!(mid, "outlook:AAMkAGI2TG93AAA=");
    }

    #[test]
    fn test_parse_graph_message() {
        let json = r#"{
            "id": "AAMkAGI2TG93AAA=",
            "subject": "Test Subject",
            "bodyPreview": "This is a preview",
            "conversationId": "AAQkAGI2TG93AAA=",
            "receivedDateTime": "2024-01-15T10:30:00Z",
            "isRead": false,
            "hasAttachments": true,
            "from": {
                "emailAddress": {
                    "name": "John Doe",
                    "address": "john@example.com"
                }
            },
            "toRecipients": [
                {
                    "emailAddress": {
                        "name": "Jane Smith",
                        "address": "jane@example.com"
                    }
                }
            ],
            "flag": {
                "flagStatus": "notFlagged"
            },
            "importance": "normal"
        }"#;

        let msg: GraphMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.id, "AAMkAGI2TG93AAA=");
        assert_eq!(msg.subject.as_deref(), Some("Test Subject"));
        assert_eq!(msg.body_preview.as_deref(), Some("This is a preview"));
        assert_eq!(msg.conversation_id.as_deref(), Some("AAQkAGI2TG93AAA="));
        assert_eq!(msg.is_read, Some(false));
        assert_eq!(msg.has_attachments, Some(true));
        assert_eq!(
            msg.from
                .as_ref()
                .unwrap()
                .email_address
                .name
                .as_deref(),
            Some("John Doe")
        );
        assert_eq!(
            msg.to_recipients
                .as_ref()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            msg.flag
                .as_ref()
                .unwrap()
                .flag_status
                .as_deref(),
            Some("notFlagged")
        );
        assert!(msg.removed.is_none());
    }

    #[test]
    fn test_parse_delta_response() {
        let json = r#"{
            "value": [
                {
                    "id": "msg1",
                    "subject": "Hello",
                    "conversationId": "conv1",
                    "receivedDateTime": "2024-01-15T10:30:00Z",
                    "isRead": true
                }
            ],
            "@odata.nextLink": "https://graph.microsoft.com/v1.0/me/messages/delta?$skiptoken=abc",
            "@odata.deltaLink": null
        }"#;

        let delta: GraphDeltaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(delta.value.len(), 1);
        assert_eq!(delta.value[0].id, "msg1");
        assert!(delta.next_link.is_some());
        assert!(delta.delta_link.is_none());
    }

    #[test]
    fn test_parse_delta_response_with_delta_link() {
        let json = r#"{
            "value": [],
            "@odata.deltaLink": "https://graph.microsoft.com/v1.0/me/messages/delta?$deltatoken=xyz"
        }"#;

        let delta: GraphDeltaResponse = serde_json::from_str(json).unwrap();
        assert!(delta.value.is_empty());
        assert!(delta.next_link.is_none());
        assert!(delta.delta_link.is_some());
        assert!(delta.delta_link.unwrap().contains("deltatoken"));
    }

    #[test]
    fn test_parse_delta_removed_message() {
        let json = r#"{
            "id": "msg-deleted",
            "@removed": {
                "reason": "deleted"
            }
        }"#;

        let msg: GraphMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.id, "msg-deleted");
        assert!(msg.removed.is_some());
        assert_eq!(
            msg.removed.as_ref().unwrap().reason.as_deref(),
            Some("deleted")
        );
    }

    #[tokio::test]
    async fn test_outlook_sync_state_crud() {
        let pool = setup_test_db().await;

        // Initially no delta link
        let link = get_outlook_delta_link(&pool, "acc1", "inbox").await;
        assert!(link.is_none());

        // Set a delta link
        set_outlook_delta_link(&pool, "acc1", "inbox", "https://delta.link/token=abc").await;
        let link = get_outlook_delta_link(&pool, "acc1", "inbox").await;
        assert_eq!(link.as_deref(), Some("https://delta.link/token=abc"));

        // Update the delta link
        set_outlook_delta_link(&pool, "acc1", "inbox", "https://delta.link/token=def").await;
        let link = get_outlook_delta_link(&pool, "acc1", "inbox").await;
        assert_eq!(link.as_deref(), Some("https://delta.link/token=def"));

        // Different folder has no link
        let link2 = get_outlook_delta_link(&pool, "acc1", "sentitems").await;
        assert!(link2.is_none());
    }

    #[test]
    fn test_folder_to_label_mapping() {
        assert_eq!(map_folder_to_label("Inbox"), "INBOX");
        assert_eq!(map_folder_to_label("INBOX"), "INBOX");
        assert_eq!(map_folder_to_label("Sent Items"), "SENT");
        assert_eq!(map_folder_to_label("sentitems"), "SENT");
        assert_eq!(map_folder_to_label("Drafts"), "DRAFT");
        assert_eq!(map_folder_to_label("Deleted Items"), "TRASH");
        assert_eq!(map_folder_to_label("deleteditems"), "TRASH");
        assert_eq!(map_folder_to_label("Junk Email"), "SPAM");
        assert_eq!(map_folder_to_label("junkemail"), "SPAM");
        assert_eq!(map_folder_to_label("Archive"), "ARCHIVE");
        assert_eq!(
            map_folder_to_label("My Custom Folder"),
            "outlook:My Custom Folder"
        );
    }

    #[test]
    fn test_parse_received_datetime() {
        let ts = parse_received_datetime("2024-01-15T10:30:00Z");
        assert!(ts > 0);

        let ts_invalid = parse_received_datetime("not-a-date");
        assert_eq!(ts_invalid, 0);
    }

    #[test]
    fn test_parse_recipients() {
        let recips = parse_recipients("john@example.com, Jane <jane@example.com>");
        assert_eq!(recips.len(), 2);
        assert_eq!(recips[0].email_address.address, "john@example.com");
        assert_eq!(recips[1].email_address.address, "jane@example.com");

        let empty = parse_recipients("");
        assert!(empty.is_empty());
    }

    #[test]
    fn test_parse_folders_response() {
        let json = r#"{
            "value": [
                {
                    "id": "folder-id-1",
                    "displayName": "Inbox",
                    "unreadItemCount": 5,
                    "totalItemCount": 100
                },
                {
                    "id": "folder-id-2",
                    "displayName": "Sent Items",
                    "unreadItemCount": 0,
                    "totalItemCount": 50
                }
            ]
        }"#;

        let folders: GraphFoldersResponse = serde_json::from_str(json).unwrap();
        assert_eq!(folders.value.len(), 2);
        assert_eq!(
            folders.value[0].display_name.as_deref(),
            Some("Inbox")
        );
        assert_eq!(folders.value[0].unread_item_count, Some(5));
        assert_eq!(folders.value[1].total_item_count, Some(50));
    }

    #[tokio::test]
    async fn test_get_outlook_thread_messages_empty() {
        let pool = setup_test_db().await;
        let msgs = get_outlook_thread_messages(&pool, "nonexistent-thread").await.unwrap();
        assert!(msgs.is_empty());
    }

    #[tokio::test]
    async fn test_get_outlook_thread_messages_returns_graph_ids() {
        let pool = setup_test_db().await;
        // Insert a thread and messages
        sqlx::query(
            "INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0)",
        )
        .bind("outlook:acc1:conv1")
        .bind("acc1")
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, '', '', '', '', 0, '', '', 0)",
        )
        .bind("outlook:AAMkAG1")
        .bind("outlook:acc1:conv1")
        .bind("acc1")
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, '', '', '', '', 0, '', '', 0)",
        )
        .bind("outlook:AAMkAG2")
        .bind("outlook:acc1:conv1")
        .bind("acc1")
        .execute(&pool)
        .await
        .unwrap();

        let ids = get_outlook_thread_messages(&pool, "outlook:acc1:conv1")
            .await
            .unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"AAMkAG1".to_string()));
        assert!(ids.contains(&"AAMkAG2".to_string()));
    }

    // -----------------------------------------------------------------------
    // Mock-based integration tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_fetch_outlook_folders_via_mock() {
        let _guard = MOCK_ENV_LOCK.lock().unwrap();
        let server = httpmock::MockServer::start();
        std::env::set_var(
            "TEST_GRAPH_API_BASE",
            format!("http://{}", server.address()),
        );

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/v1.0/me/mailFolders")
                .query_param("$top", "100");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(
                    r#"{
                    "value": [
                        {"id": "f1", "displayName": "Inbox", "unreadItemCount": 3, "totalItemCount": 20},
                        {"id": "f2", "displayName": "Sent Items", "unreadItemCount": 0, "totalItemCount": 10},
                        {"id": "f3", "displayName": "My Folder", "unreadItemCount": 1, "totalItemCount": 5}
                    ]
                }"#,
                );
        });

        let pool = setup_test_db().await;
        fetch_and_store_outlook_folders(&pool, "acc1", "test-token")
            .await
            .unwrap();

        mock.assert();

        // Verify labels were stored
        let labels: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, name FROM labels WHERE account_id = 'acc1' ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(labels.len(), 3);
        assert!(labels.iter().any(|(id, _)| id == "INBOX"));
        assert!(labels.iter().any(|(id, _)| id == "SENT"));
        assert!(labels.iter().any(|(id, _)| id == "outlook:My Folder"));

        std::env::remove_var("TEST_GRAPH_API_BASE");
    }

    #[tokio::test]
    async fn test_outlook_delta_sync_initial_via_mock() {
        let _guard = MOCK_ENV_LOCK.lock().unwrap();
        let server = httpmock::MockServer::start();
        std::env::set_var(
            "TEST_GRAPH_API_BASE",
            format!("http://{}", server.address()),
        );

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/v1.0/me/mailFolders/inbox/messages/delta");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(
                    r#"{
                    "value": [
                        {
                            "id": "msg-001",
                            "subject": "Welcome",
                            "bodyPreview": "Welcome to Outlook",
                            "conversationId": "conv-001",
                            "receivedDateTime": "2024-01-15T10:30:00Z",
                            "isRead": false,
                            "hasAttachments": false,
                            "from": {"emailAddress": {"name": "Support", "address": "support@example.com"}},
                            "toRecipients": [{"emailAddress": {"name": "User", "address": "user@outlook.com"}}],
                            "flag": {"flagStatus": "notFlagged"}
                        }
                    ],
                    "@odata.deltaLink": "https://graph.microsoft.com/delta?token=initial"
                }"#,
                );
        });

        let pool = setup_test_db().await;
        let delta = outlook_delta_sync(&pool, "user@outlook.com", "test-token", "inbox")
            .await
            .unwrap();

        mock.assert();

        assert_eq!(delta.new_thread_ids.len(), 1);
        assert!(delta.new_thread_ids[0].starts_with("outlook:user@outlook.com:conv-001"));

        // Verify thread stored
        let thread_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM threads WHERE account_id = 'user@outlook.com'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(thread_count, 1);

        // Verify message stored
        let msg_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE account_id = 'user@outlook.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(msg_count, 1);

        // Verify delta link saved
        let link = get_outlook_delta_link(&pool, "user@outlook.com", "inbox").await;
        assert!(link.is_some());
        assert!(link.unwrap().contains("token=initial"));

        std::env::remove_var("TEST_GRAPH_API_BASE");
    }

    #[tokio::test]
    async fn test_outlook_delta_sync_incremental_via_mock() {
        let _guard = MOCK_ENV_LOCK.lock().unwrap();
        let server = httpmock::MockServer::start();
        std::env::set_var(
            "TEST_GRAPH_API_BASE",
            format!("http://{}", server.address()),
        );

        let pool = setup_test_db().await;

        // Set an existing delta link
        let delta_url = format!(
            "http://{}/v1.0/me/messages/delta?token=existing",
            server.address()
        );
        set_outlook_delta_link(&pool, "acc1", "inbox", &delta_url).await;

        // Seed an existing thread and message
        sqlx::query(
            "INSERT INTO threads (id, account_id, snippet, history_id, unread, sender, subject, latest_date, metadata_synced) VALUES (?, ?, '', '', 0, 'Old', 'Old Subject', 100, 1)",
        )
        .bind("outlook:acc1:conv-exist")
        .bind("acc1")
        .execute(&pool)
        .await
        .unwrap();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/v1.0/me/messages/delta")
                .query_param("token", "existing");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(
                    r#"{
                    "value": [
                        {
                            "id": "msg-new",
                            "subject": "New Message",
                            "bodyPreview": "New content",
                            "conversationId": "conv-new",
                            "receivedDateTime": "2024-02-01T12:00:00Z",
                            "isRead": false,
                            "hasAttachments": false,
                            "from": {"emailAddress": {"name": "Sender", "address": "sender@example.com"}},
                            "flag": {"flagStatus": "notFlagged"}
                        },
                        {
                            "id": "msg-update",
                            "subject": "Updated Subject",
                            "conversationId": "conv-exist",
                            "receivedDateTime": "2024-02-01T12:05:00Z",
                            "isRead": true,
                            "from": {"emailAddress": {"name": "Updater", "address": "update@example.com"}},
                            "flag": {"flagStatus": "notFlagged"}
                        }
                    ],
                    "@odata.deltaLink": "https://graph.microsoft.com/delta?token=new"
                }"#,
                );
        });

        let delta = outlook_delta_sync(&pool, "acc1", "test-token", "inbox")
            .await
            .unwrap();

        mock.assert();

        assert_eq!(delta.new_thread_ids.len(), 1);
        assert!(delta.updated_thread_ids.len() >= 1);

        std::env::remove_var("TEST_GRAPH_API_BASE");
    }

    #[tokio::test]
    async fn test_outlook_send_message_via_mock() {
        let _guard = MOCK_ENV_LOCK.lock().unwrap();
        let server = httpmock::MockServer::start();
        std::env::set_var(
            "TEST_GRAPH_API_BASE",
            format!("http://{}", server.address()),
        );

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/v1.0/me/sendMail");
            then.status(202);
        });

        outlook_send_message(
            "test-token",
            "recipient@example.com",
            "Test Subject",
            "<p>Hello</p>",
            &[],
        )
        .await
        .unwrap();

        mock.assert();
        std::env::remove_var("TEST_GRAPH_API_BASE");
    }

    #[tokio::test]
    async fn test_outlook_mark_read_via_mock() {
        let _guard = MOCK_ENV_LOCK.lock().unwrap();
        let server = httpmock::MockServer::start();
        std::env::set_var(
            "TEST_GRAPH_API_BASE",
            format!("http://{}", server.address()),
        );

        let pool = setup_test_db().await;

        // Set up thread and message
        sqlx::query(
            "INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 1)",
        )
        .bind("outlook:acc1:conv1")
        .bind("acc1")
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, '', '', '', '', 0, '', '', 0)",
        )
        .bind("outlook:AAMkMsg1")
        .bind("outlook:acc1:conv1")
        .bind("acc1")
        .execute(&pool)
        .await
        .unwrap();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::PATCH)
                .path("/v1.0/me/messages/AAMkMsg1");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(r#"{"id": "AAMkMsg1"}"#);
        });

        outlook_mark_read(&pool, "test-token", "outlook:acc1:conv1", true)
            .await
            .unwrap();

        mock.assert();

        // Verify local state
        let unread: i32 = sqlx::query_scalar(
            "SELECT unread FROM threads WHERE id = 'outlook:acc1:conv1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(unread, 0);

        std::env::remove_var("TEST_GRAPH_API_BASE");
    }

    #[tokio::test]
    async fn test_outlook_trash_thread_via_mock() {
        let _guard = MOCK_ENV_LOCK.lock().unwrap();
        let server = httpmock::MockServer::start();
        std::env::set_var(
            "TEST_GRAPH_API_BASE",
            format!("http://{}", server.address()),
        );

        let pool = setup_test_db().await;

        // Set up thread and message
        sqlx::query(
            "INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0)",
        )
        .bind("outlook:acc1:conv-trash")
        .bind("acc1")
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, '', '', '', '', 0, '', '', 0)",
        )
        .bind("outlook:AAMkTrash1")
        .bind("outlook:acc1:conv-trash")
        .bind("acc1")
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES (?, 'INBOX')")
            .bind("outlook:acc1:conv-trash")
            .execute(&pool)
            .await
            .unwrap();

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/v1.0/me/messages/AAMkTrash1/move");
            then.status(201)
                .header("Content-Type", "application/json")
                .body(r#"{"id": "AAMkTrash1"}"#);
        });

        outlook_trash_thread(&pool, "test-token", "outlook:acc1:conv-trash")
            .await
            .unwrap();

        mock.assert();

        // Verify thread is deleted locally
        let thread_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM threads WHERE id = 'outlook:acc1:conv-trash'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(thread_count, 0);

        // Verify labels cleaned up
        let label_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM thread_labels WHERE thread_id = 'outlook:acc1:conv-trash'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(label_count, 0);

        std::env::remove_var("TEST_GRAPH_API_BASE");
    }
}
