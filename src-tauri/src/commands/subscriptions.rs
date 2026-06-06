use crate::subscription_detector::{detect, DetectionInput};
use sqlx::SqlitePool;
use tauri::{Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

#[derive(serde::Serialize)]
pub struct SubscriptionInfo {
    pub id: i64,
    pub account_id: String,
    pub sender_email: String,
    pub sender_name: Option<String>,
    pub detection_method: String,
    pub detection_details: Option<String>,
    pub unsubscribe_url: Option<String>,
    pub unsubscribe_mailto: Option<String>,
    pub supports_one_click: bool,
    pub status: String,
    pub message_count: i32,
    pub read_count: i32,
    pub avg_frequency_days: Option<f64>,
    pub first_seen: i64,
    pub last_seen: i64,
    pub user_corrected: bool,
}

#[tauri::command]
pub async fn get_subscriptions(
    app_handle: tauri::AppHandle,
    account_id: Option<String>,
    status: Option<String>,
) -> Result<Vec<SubscriptionInfo>, String> {
    let pool = app_handle.state::<SqlitePool>();

    let (sql, binds) = match (&account_id, &status) {
        (Some(acc_id), Some(stat)) => (
            "SELECT id, account_id, sender_email, sender_name, detection_method, 
                    detection_details, unsubscribe_url, unsubscribe_mailto, 
                    supports_one_click, status, message_count, read_count, 
                    avg_frequency_days, first_seen, last_seen, user_corrected
             FROM subscriptions WHERE account_id = ? AND status = ?".to_string(),
            vec![acc_id.clone(), stat.clone()],
        ),
        (Some(acc_id), None) => (
            "SELECT id, account_id, sender_email, sender_name, detection_method, 
                    detection_details, unsubscribe_url, unsubscribe_mailto, 
                    supports_one_click, status, message_count, read_count, 
                    avg_frequency_days, first_seen, last_seen, user_corrected
             FROM subscriptions WHERE account_id = ?".to_string(),
            vec![acc_id.clone()],
        ),
        (None, Some(stat)) => (
            "SELECT id, account_id, sender_email, sender_name, detection_method, 
                    detection_details, unsubscribe_url, unsubscribe_mailto, 
                    supports_one_click, status, message_count, read_count, 
                    avg_frequency_days, first_seen, last_seen, user_corrected
             FROM subscriptions WHERE status = ?".to_string(),
            vec![stat.clone()],
        ),
        (None, None) => (
            "SELECT id, account_id, sender_email, sender_name, detection_method, 
                    detection_details, unsubscribe_url, unsubscribe_mailto, 
                    supports_one_click, status, message_count, read_count, 
                    avg_frequency_days, first_seen, last_seen, user_corrected
             FROM subscriptions".to_string(),
            vec![],
        ),
    };

    #[derive(sqlx::FromRow)]
    struct Row {
        id: i64,
        account_id: String,
        sender_email: String,
        sender_name: Option<String>,
        detection_method: String,
        detection_details: Option<String>,
        unsubscribe_url: Option<String>,
        unsubscribe_mailto: Option<String>,
        supports_one_click: i32,
        status: String,
        message_count: i32,
        read_count: i32,
        avg_frequency_days: Option<f64>,
        first_seen: i64,
        last_seen: i64,
        user_corrected: i32,
    }

    let mut query = sqlx::query_as::<_, Row>(&sql);
    for bind in &binds {
        query = query.bind(bind);
    }

    let rows = query.fetch_all(pool.inner()).await.map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| SubscriptionInfo {
            id: r.id,
            account_id: r.account_id,
            sender_email: r.sender_email,
            sender_name: r.sender_name,
            detection_method: r.detection_method,
            detection_details: r.detection_details,
            unsubscribe_url: r.unsubscribe_url,
            unsubscribe_mailto: r.unsubscribe_mailto,
            supports_one_click: r.supports_one_click == 1,
            status: r.status,
            message_count: r.message_count,
            read_count: r.read_count,
            avg_frequency_days: r.avg_frequency_days,
            first_seen: r.first_seen,
            last_seen: r.last_seen,
            user_corrected: r.user_corrected == 1,
        })
        .collect())
}

#[tauri::command]
pub async fn correct_subscription(
    app_handle: tauri::AppHandle,
    subscription_id: i64,
    is_subscription: bool,
) -> Result<(), String> {
    let pool = app_handle.state::<SqlitePool>();
    
    let status = if is_subscription { "active" } else { "ignored" };
    
    sqlx::query("UPDATE subscriptions SET user_corrected = 1, status = ? WHERE id = ?")
        .bind(status)
        .bind(subscription_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[derive(serde::Serialize)]
pub struct UnsubscribeResult {
    pub method: String,
    pub success: bool,
    pub message: String,
    pub opened_browser: bool,
}

#[tauri::command]
pub async fn delete_subscription(
    app_handle: tauri::AppHandle,
    subscription_id: i64,
) -> Result<(), String> {
    let pool = app_handle.state::<SqlitePool>();

    sqlx::query("DELETE FROM subscriptions WHERE id = ?")
        .bind(subscription_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn unsubscribe(
    app_handle: tauri::AppHandle,
    subscription_id: i64,
) -> Result<UnsubscribeResult, String> {
    let pool = app_handle.state::<SqlitePool>();

    #[derive(sqlx::FromRow)]
    struct SubRow {
        unsubscribe_url: Option<String>,
        unsubscribe_mailto: Option<String>,
        supports_one_click: i32,
    }

    let row = sqlx::query_as::<_, SubRow>(
        "SELECT unsubscribe_url, unsubscribe_mailto, supports_one_click FROM subscriptions WHERE id = ?"
    )
    .bind(subscription_id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let sub = match row {
        Some(r) => r,
        None => return Ok(UnsubscribeResult {
            method: "none".to_string(),
            success: false,
            message: "Subscription not found".to_string(),
            opened_browser: false,
        }),
    };

    if sub.supports_one_click == 1 {
        if let Some(url) = &sub.unsubscribe_url {
            let client = reqwest::Client::new();
            let result = client.post(url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body("List-Unsubscribe=One-Click")
                .send()
                .await;

            if let Ok(resp) = result {
                if resp.status().is_success() {
                    let _ = sqlx::query("UPDATE subscriptions SET status = \"unsubscribed\" WHERE id = ?")
                        .bind(subscription_id)
                        .execute(pool.inner())
                        .await;

                    return Ok(UnsubscribeResult {
                        method: "one_click".to_string(),
                        success: true,
                        message: "Successfully unsubscribed via one-click".to_string(),
                        opened_browser: false,
                    });
                }
            }
        }
    }

    if let Some(url) = &sub.unsubscribe_url {
        if let Err(e) = app_handle.opener().open_url(url, None::<&str>) {
            return Ok(UnsubscribeResult {
                method: "https".to_string(),
                success: false,
                message: format!("Failed to open URL: {}", e),
                opened_browser: false,
            });
        }

        return Ok(UnsubscribeResult {
            method: "https".to_string(),
            success: true,
            message: "Opened unsubscribe URL in browser".to_string(),
            opened_browser: true,
        });
    }

    if sub.unsubscribe_mailto.is_some() {
        return Ok(UnsubscribeResult {
            method: "mailto".to_string(),
            success: false,
            message: "Mailto unsubscribe requires manual action".to_string(),
            opened_browser: false,
        });
    }

    tracing::info!("Unsubscribe: no method available for subscription_id={}", subscription_id);
    Ok(UnsubscribeResult {
        method: "none".to_string(),
        success: false,
        message: "No unsubscribe method available".to_string(),
        opened_browser: false,
    })
}

#[tauri::command]
pub async fn mark_unsubscribed(
    app_handle: tauri::AppHandle,
    subscription_id: i64,
) -> Result<(), String> {
    let pool = app_handle.state::<SqlitePool>();
    sqlx::query("UPDATE subscriptions SET status = 'unsubscribed' WHERE id = ?")
        .bind(subscription_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(serde::Serialize)]
pub struct ScanResult {
    pub messages_scanned: i64,
    pub subscriptions_found: i64,
    pub subscriptions_updated: i64,
    pub enriched: i64,
}

#[tauri::command]
pub async fn scan_subscriptions(
    app_handle: tauri::AppHandle,
    account_id: String,
) -> Result<ScanResult, String> {
    let pool = app_handle.state::<SqlitePool>();
    let account = super::accounts::get_account_by_id(pool.inner(), &account_id).await?;
    scan_subscriptions_inner(Some(&app_handle), pool.inner(), &account_id, &account.access_token).await
}

pub async fn scan_subscriptions_inner(
    app_handle: Option<&tauri::AppHandle>,
    pool: &SqlitePool,
    account_id: &str,
    access_token: &str,
) -> Result<ScanResult, String> {
    tracing::info!("Scanning subscriptions for {}", account_id);

    let depth_setting = crate::commands::settings::get_setting_inner(pool, "subscription_scan_depth")
        .await
        .unwrap_or_else(|_| "500".to_string());
    let scan_depth: i32 = depth_setting.parse().unwrap_or(500);

    let mut seen_senders = std::collections::HashSet::new();
    let mut api_messages_scanned: i64 = 0;
    let mut api_subscriptions_found = 0;

    if scan_depth > 0 {
        let provider_type = super::accounts::get_provider_type(pool, account_id).await;
        tracing::info!("Remote scan provider: {} depth: {}", provider_type, scan_depth);

        if let Some(h) = app_handle {
            let _ = h.emit("scan-progress", format!("Scanning {} messages from {} server...", scan_depth, provider_type));
        }

        match provider_type.as_str() {
            "gmail" => {
                let mut all_ids: Vec<String> = Vec::new();
                let mut page_token: Option<String> = None;

                loop {
                    let (ids, next_token) = match crate::gmail_api::list_recent_message_ids(
                        access_token,
                        scan_depth.min(500),
                        page_token.as_deref(),
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(e) => {
                            tracing::warn!("Gmail list messages failed: {}", e);
                            break;
                        }
                    };
                    all_ids.extend(ids);
                    if let Some(h) = app_handle {
                        let _ = h.emit("scan-progress", format!("Fetched {} message IDs from Gmail", all_ids.len()));
                    }
                    page_token = next_token;
                    if all_ids.len() >= scan_depth as usize || page_token.is_none() {
                        break;
                    }
                }

                all_ids.truncate(scan_depth as usize);
                tracing::info!("Gmail remote scan: {} message IDs to fetch", all_ids.len());

                let messages = crate::gmail_api::batch_fetch_message_metadata(access_token, &all_ids).await.unwrap_or_default();
                tracing::info!("Gmail batch fetch returned {} messages", messages.len());

                for gmail_msg in &messages {
                    api_messages_scanned += 1;
                    if api_messages_scanned % 100 == 0 {
                        if let Some(h) = app_handle {
                            let _ = h.emit("scan-progress", format!("Scanned {} messages from Gmail API", api_messages_scanned));
                        }
                    }

                    let from = gmail_msg.payload.as_ref()
                        .and_then(|p| p.headers.as_ref())
                        .and_then(|h| h.iter().find(|hdr| hdr.name.eq_ignore_ascii_case("From")))
                        .map(|h| h.value.as_str())
                        .unwrap_or("");
                    if from.is_empty() { continue; }

                    if let Some(payload) = &gmail_msg.payload {
                        if let Some(headers) = &payload.headers {
                            let header_refs: Vec<(&str, &str)> = headers.iter().map(|h| (h.name.as_str(), h.value.as_str())).collect();
                            let input = DetectionInput { headers: header_refs, body_plain: None, body_html: None, sender: from };
                            let result = detect(&input);
                            if result.is_subscription {
                                if seen_senders.contains(&result.sender_email) { continue; }
                                seen_senders.insert(result.sender_email.clone());
                                let internal_date: i64 = gmail_msg.internal_date.parse().unwrap_or(0);
                                let _ = sqlx::query(
                                    "INSERT INTO subscriptions (account_id, sender_email, sender_name, detection_method, detection_details, unsubscribe_url, unsubscribe_mailto, supports_one_click, first_seen, last_seen, message_count)
                                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)
                                     ON CONFLICT(account_id, sender_email) DO UPDATE SET
                                        detection_method = excluded.detection_method, detection_details = excluded.detection_details,
                                        unsubscribe_url = COALESCE(excluded.unsubscribe_url, subscriptions.unsubscribe_url),
                                        unsubscribe_mailto = COALESCE(excluded.unsubscribe_mailto, subscriptions.unsubscribe_mailto),
                                        supports_one_click = MAX(subscriptions.supports_one_click, excluded.supports_one_click),
                                        message_count = message_count + 1, last_seen = MAX(subscriptions.last_seen, excluded.last_seen)"
                                )
                                .bind(account_id).bind(&result.sender_email).bind(&result.sender_name)
                                .bind(result.methods.join(", ")).bind(&result.details)
                                .bind(&result.unsubscribe_url).bind(&result.unsubscribe_mailto)
                                .bind(if result.supports_one_click { 1 } else { 0 })
                                .bind(internal_date).bind(internal_date)
                                .execute(pool).await;
                                api_subscriptions_found += 1;
                            }
                        }
                    }
                }
            }
            "imap" => {
                match crate::provider::imap::operations::imap_remote_scan(pool, account_id, scan_depth).await {
                    Ok(headers) => {
                        tracing::info!("IMAP remote scan returned {} messages", headers.len());
                        for hdr in &headers {
                            api_messages_scanned += 1;
                            if api_messages_scanned % 100 == 0 {
                                if let Some(h) = app_handle {
                                    let _ = h.emit("scan-progress", format!("Scanned {} messages from IMAP server", api_messages_scanned));
                                }
                            }
                            let header_refs: Vec<(&str, &str)> = hdr.headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                            let input = DetectionInput { headers: header_refs, body_plain: None, body_html: None, sender: &hdr.sender };
                            let result = detect(&input);
                            if result.is_subscription {
                                if seen_senders.contains(&result.sender_email) { continue; }
                                seen_senders.insert(result.sender_email.clone());
                                let _ = sqlx::query(
                                    "INSERT INTO subscriptions (account_id, sender_email, sender_name, detection_method, detection_details, unsubscribe_url, unsubscribe_mailto, supports_one_click, first_seen, last_seen, message_count)
                                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)
                                     ON CONFLICT(account_id, sender_email) DO UPDATE SET
                                        detection_method = excluded.detection_method, detection_details = excluded.detection_details,
                                        unsubscribe_url = COALESCE(excluded.unsubscribe_url, subscriptions.unsubscribe_url),
                                        unsubscribe_mailto = COALESCE(excluded.unsubscribe_mailto, subscriptions.unsubscribe_mailto),
                                        supports_one_click = MAX(subscriptions.supports_one_click, excluded.supports_one_click),
                                        message_count = message_count + 1, last_seen = MAX(subscriptions.last_seen, excluded.last_seen)"
                                )
                                .bind(account_id).bind(&result.sender_email).bind(&result.sender_name)
                                .bind(result.methods.join(", ")).bind(&result.details)
                                .bind(&result.unsubscribe_url).bind(&result.unsubscribe_mailto)
                                .bind(if result.supports_one_click { 1 } else { 0 })
                                .bind(hdr.date).bind(hdr.date)
                                .execute(pool).await;
                                api_subscriptions_found += 1;
                            }
                        }
                    }
                    Err(e) => tracing::warn!("IMAP remote scan failed: {}", e),
                }
            }
            "outlook" => {
                let mut skip = 0;
                loop {
                    match crate::outlook_api::list_outlook_messages_with_headers(access_token, scan_depth.min(500), skip).await {
                        Ok((messages, next_skip)) => {
                            tracing::info!("Outlook list returned {} messages (skip={})", messages.len(), skip);
                            for msg in &messages {
                                api_messages_scanned += 1;
                                if api_messages_scanned % 100 == 0 {
                                    if let Some(h) = app_handle {
                                        let _ = h.emit("scan-progress", format!("Scanned {} messages from Outlook server", api_messages_scanned));
                                    }
                                }
                                let header_refs: Vec<(&str, &str)> = msg.headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                                let sender = format!("{} <{}>", msg.from_name.as_deref().unwrap_or(""), msg.from_address.as_deref().unwrap_or(""));
                                let input = DetectionInput { headers: header_refs, body_plain: None, body_html: None, sender: &sender };
                                let result = detect(&input);
                                if result.is_subscription {
                                    if seen_senders.contains(&result.sender_email) { continue; }
                                    seen_senders.insert(result.sender_email.clone());
                                    let internal_date: i64 = msg.received_date_time.parse().unwrap_or(0);
                                    let _ = sqlx::query(
                                        "INSERT INTO subscriptions (account_id, sender_email, sender_name, detection_method, detection_details, unsubscribe_url, unsubscribe_mailto, supports_one_click, first_seen, last_seen, message_count)
                                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)
                                         ON CONFLICT(account_id, sender_email) DO UPDATE SET
                                            detection_method = excluded.detection_method, detection_details = excluded.detection_details,
                                            unsubscribe_url = COALESCE(excluded.unsubscribe_url, subscriptions.unsubscribe_url),
                                            unsubscribe_mailto = COALESCE(excluded.unsubscribe_mailto, subscriptions.unsubscribe_mailto),
                                            supports_one_click = MAX(subscriptions.supports_one_click, excluded.supports_one_click),
                                            message_count = message_count + 1, last_seen = MAX(subscriptions.last_seen, excluded.last_seen)"
                                    )
                                    .bind(account_id).bind(&result.sender_email).bind(&result.sender_name)
                                    .bind(result.methods.join(", ")).bind(&result.details)
                                    .bind(&result.unsubscribe_url).bind(&result.unsubscribe_mailto)
                                    .bind(if result.supports_one_click { 1 } else { 0 })
                                    .bind(internal_date).bind(internal_date)
                                    .execute(pool).await;
                                    api_subscriptions_found += 1;
                                }
                            }
                            if next_skip.is_none() || api_messages_scanned >= scan_depth as i64 { break; }
                            skip = next_skip.unwrap();
                        }
                        Err(e) => {
                            tracing::warn!("Outlook remote scan failed: {}", e);
                            break;
                        }
                    }
                }
            }
            _ => {
                tracing::info!("No remote scan available for provider '{}'", provider_type);
            }
        }

        tracing::info!("Remote scan complete: {} scanned, {} subscriptions found", api_messages_scanned, api_subscriptions_found);
    }

    #[derive(sqlx::FromRow)]
    #[allow(dead_code)]
    struct MsgRow {
        id: String,
        sender: String,
        body_plain: Option<String>,
        body_html: Option<String>,
        internal_date: i64,
    }

    let messages = sqlx::query_as::<_, MsgRow>(
        "SELECT m.id, m.sender, m.body_plain, m.body_html, m.internal_date
         FROM messages m
         JOIN threads t ON m.thread_id = t.id
         WHERE t.account_id = ?"
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut messages_scanned: i64 = 0;
    let mut subscriptions_found = 0;
    let mut subscriptions_updated = 0;
    let total_messages = messages.len() as i64;

    for msg in messages {
        messages_scanned += 1;
        if messages_scanned % 50 == 0 || messages_scanned == total_messages {
            if let Some(h) = app_handle { let _ = h.emit("scan-progress", format!("Scanning messages ({}/{})", messages_scanned, total_messages)); }
        }

        let input = DetectionInput {
            headers: vec![],
            body_plain: msg.body_plain.as_deref(),
            body_html: msg.body_html.as_deref(),
            sender: &msg.sender,
        };

        let result = detect(&input);

        if result.is_subscription {
            if seen_senders.contains(&result.sender_email) {
                continue;
            }
            seen_senders.insert(result.sender_email.clone());

            let insert_result = sqlx::query(
                "INSERT INTO subscriptions (account_id, sender_email, sender_name, detection_method, detection_details, unsubscribe_url, unsubscribe_mailto, supports_one_click, first_seen, last_seen, message_count)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)
                 ON CONFLICT(account_id, sender_email) DO UPDATE SET
                    detection_method = excluded.detection_method,
                    detection_details = excluded.detection_details,
                    unsubscribe_url = COALESCE(excluded.unsubscribe_url, subscriptions.unsubscribe_url),
                    unsubscribe_mailto = COALESCE(excluded.unsubscribe_mailto, subscriptions.unsubscribe_mailto),
                    supports_one_click = MAX(subscriptions.supports_one_click, excluded.supports_one_click),
                    message_count = message_count + 1,
                    last_seen = MAX(subscriptions.last_seen, excluded.last_seen)"
            )
            .bind(account_id)
            .bind(&result.sender_email)
            .bind(&result.sender_name)
            .bind(result.methods.join(", "))
            .bind(&result.details)
            .bind(&result.unsubscribe_url)
            .bind(&result.unsubscribe_mailto)
            .bind(if result.supports_one_click { 1 } else { 0 })
            .bind(msg.internal_date)
            .bind(msg.internal_date)
            .execute(pool)
            .await;

            if let Ok(res) = insert_result {
                if res.rows_affected() > 0 {
                    subscriptions_found += 1;
                } else {
                    subscriptions_updated += 1;
                }
            }
        }
    }

    if let Some(h) = app_handle { let _ = h.emit("scan-progress", "Recomputing statistics...".to_string()); }

    // Recompute statistics from actual message data
    let _ = sqlx::query(
        "UPDATE subscriptions SET
            first_seen = sub_stats.min_date,
            last_seen = sub_stats.max_date,
            message_count = sub_stats.msg_count,
            avg_frequency_days = CASE
                WHEN sub_stats.msg_count > 1 THEN
                    (sub_stats.max_date - sub_stats.min_date) / ((sub_stats.msg_count - 1) * 86400000.0)
                ELSE NULL
            END
         FROM (
            SELECT
                s.id as sub_id,
                MIN(m.internal_date) as min_date,
                MAX(m.internal_date) as max_date,
                COUNT(m.id) as msg_count
            FROM subscriptions s
            JOIN messages m ON m.sender LIKE '%' || s.sender_email || '%'
            JOIN threads t ON m.thread_id = t.id AND t.account_id = s.account_id
            WHERE s.account_id = ?
            GROUP BY s.id
         ) sub_stats
         WHERE subscriptions.id = sub_stats.sub_id"
    )
    .bind(account_id)
    .execute(pool)
    .await;

    // Enrich subscriptions missing unsubscribe data by fetching headers from the provider
    let mut enriched: i64 = 0;

    // Check provider type — remote header enrichment only works for Gmail
    let enrich_provider_type = sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(provider_type, 'gmail') FROM accounts WHERE id = ?"
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| "gmail".to_string());

    if enrich_provider_type == "gmail" {
        #[derive(sqlx::FromRow)]
        struct SubMissingUnsub {
            id: i64,
            sender_email: String,
        }

        let subs_to_enrich = sqlx::query_as::<_, SubMissingUnsub>(
            "SELECT id, sender_email FROM subscriptions
             WHERE account_id = ? AND unsubscribe_url IS NULL AND unsubscribe_mailto IS NULL"
        )
        .bind(account_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let total_to_enrich = subs_to_enrich.len();
        if total_to_enrich > 0 {
            if let Some(h) = app_handle { let _ = h.emit("scan-progress", format!("Enriching subscriptions (0/{})", total_to_enrich)); }
        }

        for (i, sub) in subs_to_enrich.iter().enumerate() {
            #[derive(sqlx::FromRow)]
            struct MsgRef {
                id: String,
                sender: String,
            }

            let msg_row = sqlx::query_as::<_, MsgRef>(
                "SELECT m.id, m.sender FROM messages m
                 JOIN threads t ON m.thread_id = t.id
                 WHERE t.account_id = ? AND m.sender LIKE '%' || ? || '%'
                 ORDER BY m.internal_date DESC LIMIT 1"
            )
            .bind(account_id)
            .bind(&sub.sender_email)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

            if let Some(h) = app_handle { let _ = h.emit("scan-progress", format!("Enriching subscriptions ({}/{})", i + 1, total_to_enrich)); }

            if let Some(msg_ref) = msg_row {
                if let Ok(gmail_msg) = crate::gmail_api::fetch_message_metadata(access_token, &msg_ref.id).await {
                    if let Some(payload) = &gmail_msg.payload {
                        if let Some(headers) = &payload.headers {
                            let header_vec: Vec<(&str, &str)> = headers
                                .iter()
                                .map(|h| (h.name.as_str(), h.value.as_str()))
                                .collect();

                            let input = DetectionInput {
                                headers: header_vec,
                                body_plain: None,
                                body_html: None,
                                sender: &msg_ref.sender,
                            };

                            let result = detect(&input);

                            if result.unsubscribe_url.is_some() || result.unsubscribe_mailto.is_some() {
                                let _ = sqlx::query(
                                    "UPDATE subscriptions SET
                                        unsubscribe_url = COALESCE(?, unsubscribe_url),
                                        unsubscribe_mailto = COALESCE(?, unsubscribe_mailto),
                                        supports_one_click = MAX(supports_one_click, ?)
                                     WHERE id = ?"
                                )
                                .bind(&result.unsubscribe_url)
                                .bind(&result.unsubscribe_mailto)
                                .bind(if result.supports_one_click { 1 } else { 0 })
                                .bind(sub.id)
                                .execute(pool)
                                .await;

                                enriched += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(ScanResult {
        messages_scanned: api_messages_scanned + messages_scanned,
        subscriptions_found: api_subscriptions_found + subscriptions_found,
        subscriptions_updated,
        enriched,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_helpers::{insert_account, insert_message, insert_thread, setup_test_db};

    #[tokio::test]
    async fn test_get_subscriptions_empty() {
        let pool = setup_test_db().await;
        
        let result = get_subscriptions_inner(&pool, None, None).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_get_subscriptions_returns_data() {
        let pool = setup_test_db().await;
        sqlx::query(
            "INSERT INTO subscriptions (account_id, sender_email, sender_name, detection_method, 
             first_seen, last_seen, status) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind("acc1")
        .bind("newsletter@example.com")
        .bind("Newsletter")
        .bind("List-Unsubscribe header")
        .bind(1000)
        .bind(2000)
        .bind("active")
        .execute(&pool)
        .await
        .unwrap();

        let result = get_subscriptions_inner(&pool, Some("acc1"), None).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].sender_email, "newsletter@example.com");
        assert_eq!(result[0].sender_name, Some("Newsletter".to_string()));
    }

    #[tokio::test]
    async fn test_correct_subscription() {
        let pool = setup_test_db().await;
        let id = sqlx::query(
            "INSERT INTO subscriptions (account_id, sender_email, sender_name, detection_method, 
             first_seen, last_seen, status) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind("acc1")
        .bind("newsletter@example.com")
        .bind("Newsletter")
        .bind("List-Unsubscribe header")
        .bind(1000)
        .bind(2000)
        .bind("active")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

        correct_subscription_inner(&pool, id, false).await.unwrap();

        let result: (i32, String) = sqlx::query_as(
            "SELECT user_corrected, status FROM subscriptions WHERE id = ?"
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(result.0, 1);
        assert_eq!(result.1, "ignored");
    }

    async fn get_subscriptions_inner(
        pool: &SqlitePool,
        account_id: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<SubscriptionInfo>, String> {
        let (sql, binds): (String, Vec<String>) = match (account_id, status) {
            (Some(acc_id), Some(stat)) => (
                "SELECT id, account_id, sender_email, sender_name, detection_method, 
                        detection_details, unsubscribe_url, unsubscribe_mailto, 
                        supports_one_click, status, message_count, read_count, 
                        avg_frequency_days, first_seen, last_seen, user_corrected
                 FROM subscriptions WHERE account_id = ? AND status = ?".to_string(),
                vec![acc_id.to_string(), stat.to_string()],
            ),
            (Some(acc_id), None) => (
                "SELECT id, account_id, sender_email, sender_name, detection_method, 
                        detection_details, unsubscribe_url, unsubscribe_mailto, 
                        supports_one_click, status, message_count, read_count, 
                        avg_frequency_days, first_seen, last_seen, user_corrected
                 FROM subscriptions WHERE account_id = ?".to_string(),
                vec![acc_id.to_string()],
            ),
            (None, Some(stat)) => (
                "SELECT id, account_id, sender_email, sender_name, detection_method, 
                        detection_details, unsubscribe_url, unsubscribe_mailto, 
                        supports_one_click, status, message_count, read_count, 
                        avg_frequency_days, first_seen, last_seen, user_corrected
                 FROM subscriptions WHERE status = ?".to_string(),
                vec![stat.to_string()],
            ),
            (None, None) => (
                "SELECT id, account_id, sender_email, sender_name, detection_method, 
                        detection_details, unsubscribe_url, unsubscribe_mailto, 
                        supports_one_click, status, message_count, read_count, 
                        avg_frequency_days, first_seen, last_seen, user_corrected
                 FROM subscriptions".to_string(),
                vec![],
            ),
        };

        #[derive(sqlx::FromRow)]
        struct Row {
            id: i64,
            account_id: String,
            sender_email: String,
            sender_name: Option<String>,
            detection_method: String,
            detection_details: Option<String>,
            unsubscribe_url: Option<String>,
            unsubscribe_mailto: Option<String>,
            supports_one_click: i32,
            status: String,
            message_count: i32,
            read_count: i32,
            avg_frequency_days: Option<f64>,
            first_seen: i64,
            last_seen: i64,
            user_corrected: i32,
        }

        let mut query = sqlx::query_as::<_, Row>(&sql);
        for bind in &binds {
            query = query.bind(bind);
        }

        let rows = query.fetch_all(pool).await.map_err(|e| e.to_string())?;

        Ok(rows
            .into_iter()
            .map(|r| SubscriptionInfo {
                id: r.id,
                account_id: r.account_id,
                sender_email: r.sender_email,
                sender_name: r.sender_name,
                detection_method: r.detection_method,
                detection_details: r.detection_details,
                unsubscribe_url: r.unsubscribe_url,
                unsubscribe_mailto: r.unsubscribe_mailto,
                supports_one_click: r.supports_one_click == 1,
                status: r.status,
                message_count: r.message_count,
                read_count: r.read_count,
                avg_frequency_days: r.avg_frequency_days,
                first_seen: r.first_seen,
                last_seen: r.last_seen,
                user_corrected: r.user_corrected == 1,
            })
            .collect())
    }

    async fn correct_subscription_inner(
        pool: &SqlitePool,
        subscription_id: i64,
        is_subscription: bool,
    ) -> Result<(), String> {
        let status = if is_subscription { "active" } else { "ignored" };
        
        sqlx::query("UPDATE subscriptions SET user_corrected = 1, status = ? WHERE id = ?")
            .bind(status)
            .bind(subscription_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_subscription() {
        let pool = setup_test_db().await;
        
        let id = sqlx::query(
            "INSERT INTO subscriptions (account_id, sender_email, sender_name, detection_method, 
             first_seen, last_seen, status) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind("acc1")
        .bind("newsletter@example.com")
        .bind("Newsletter")
        .bind("List-Unsubscribe header")
        .bind(1000)
        .bind(2000)
        .bind("active")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

        delete_subscription_inner(&pool, id).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM subscriptions WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn test_delete_subscription_nonexistent() {
        let pool = setup_test_db().await;
        
        let result = delete_subscription_inner(&pool, 99999).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_unsubscribe_result_serialization() {
        let result = UnsubscribeResult {
            method: "one_click".to_string(),
            success: true,
            message: "Success".to_string(),
            opened_browser: false,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("one_click"));
        assert!(json.contains("success"));

        let scan_result = ScanResult {
            messages_scanned: 10,
            subscriptions_found: 5,
            subscriptions_updated: 2,
            enriched: 0,
        };
        let json = serde_json::to_string(&scan_result).unwrap();
        assert!(json.contains("messages_scanned"));
    }

    #[tokio::test]
    async fn test_scan_respects_depth_setting() {
        let pool = setup_test_db().await;

        crate::commands::settings::update_setting_inner(&pool, "subscription_scan_depth", "0").await.unwrap();

        insert_account(&pool, "acc1", "user@gmail.com", "User", 1, 1000).await;
        insert_thread(&pool, "thread1", "acc1").await;
        insert_message(&pool, "msg1", "thread1", "acc1", "newsletter@example.com", "user@gmail.com", "Newsletter", 1000).await;

        sqlx::query("UPDATE messages SET body_html = '<html><body><a href=\"https://example.com/unsubscribe\">unsubscribe</a></body></html>' WHERE id = 'msg1'")
            .execute(&pool)
            .await
            .unwrap();

        let result = scan_subscriptions_inner(None, &pool, "acc1", "fake_token").await.unwrap();

        assert_eq!(result.messages_scanned, 1);
        assert_eq!(result.subscriptions_found, 1);
    }

    #[tokio::test]
    async fn test_scan_skips_already_detected_senders() {
        let pool = setup_test_db().await;

        crate::commands::settings::update_setting_inner(&pool, "subscription_scan_depth", "0").await.unwrap();

        insert_account(&pool, "acc1", "user@gmail.com", "User", 1, 1000).await;
        insert_thread(&pool, "thread1", "acc1").await;
        insert_message(&pool, "msg1", "thread1", "acc1", "newsletter@example.com", "user@gmail.com", "Newsletter", 1000).await;

        sqlx::query("UPDATE messages SET body_html = '<html><body><a href=\"https://example.com/unsubscribe\">unsubscribe</a></body></html>' WHERE id = 'msg1'")
            .execute(&pool)
            .await
            .unwrap();

        // Pre-insert an existing subscription so the UPSERT updates it rather than inserts
        sqlx::query(
            "INSERT INTO subscriptions (account_id, sender_email, sender_name, detection_method, first_seen, last_seen, status) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind("acc1")
        .bind("newsletter@example.com")
        .bind("Newsletter")
        .bind("user_corrected")
        .bind(1000)
        .bind(2000)
        .bind("active")
        .execute(&pool)
        .await
        .unwrap();

        let result = scan_subscriptions_inner(None, &pool, "acc1", "fake_token").await.unwrap();

        assert_eq!(result.messages_scanned, 1);

        // Verify no duplicate subscription was created
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM subscriptions WHERE sender_email = 'newsletter@example.com' AND account_id = 'acc1'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn test_scan_subscriptions_basic() {
        let pool = setup_test_db().await;
        
        insert_account(&pool, "acc1", "user@gmail.com", "User", 1, 1000).await;
        insert_thread(&pool, "thread1", "acc1").await;
        insert_message(&pool, "msg1", "thread1", "acc1", "newsletter@example.com", "user@gmail.com", "Newsletter", 1000).await;
        
        sqlx::query("UPDATE messages SET body_html = '<html><body><a href=\"https://example.com/unsubscribe\">unsubscribe</a></body></html>' WHERE id = 'msg1'")
            .execute(&pool)
            .await
            .unwrap();

        let result = scan_subscriptions_inner(None, &pool, "acc1", "fake_token").await.unwrap();
        
        assert_eq!(result.messages_scanned, 1);
        assert_eq!(result.subscriptions_found, 1);
        assert_eq!(result.subscriptions_updated, 0);

        let sub: (String, String) = sqlx::query_as(
            "SELECT sender_email, detection_method FROM subscriptions WHERE account_id = 'acc1'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        
        assert_eq!(sub.0, "newsletter@example.com");
        assert!(sub.1.contains("Unsubscribe link"));
    }

    async fn delete_subscription_inner(
        pool: &SqlitePool,
        subscription_id: i64,
    ) -> Result<(), String> {
        sqlx::query("DELETE FROM subscriptions WHERE id = ?")
            .bind(subscription_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }
}