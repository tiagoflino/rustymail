use super::accounts::get_active_account;
use super::threads::fetch_threads_by_ids;
use super::threads::LocalThread;
use tauri::Manager;

struct ParsedQuery {
    from: Option<String>,
    to: Option<String>,
    subject: Option<String>,
    has_attachment: bool,
    is_unread: Option<bool>,
    free_text: String,
}

fn parse_query_operators(query: &str) -> ParsedQuery {
    let mut from = None;
    let mut to = None;
    let mut subject = None;
    let mut has_attachment = false;
    let mut is_unread = None;
    let mut free_parts = Vec::new();

    let mut chars = query.chars().peekable();
    let mut tokens = Vec::new();
    let mut current = String::new();

    // Tokenize: split on spaces but respect quoted strings
    while let Some(c) = chars.next() {
        if c == '"' {
            current.push(c);
            for c2 in chars.by_ref() {
                current.push(c2);
                if c2 == '"' { break; }
            }
        } else if c == ' ' {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    for token in tokens {
        if let Some(val) = token.strip_prefix("from:") {
            if !val.is_empty() { from = Some(val.trim_matches('"').to_string()); }
        } else if let Some(val) = token.strip_prefix("to:") {
            if !val.is_empty() { to = Some(val.trim_matches('"').to_string()); }
        } else if let Some(val) = token.strip_prefix("subject:") {
            if !val.is_empty() { subject = Some(val.trim_matches('"').to_string()); }
        } else if token == "has:attachment" {
            has_attachment = true;
        } else if token == "has:link" {
            // Gmail handles this, not stored locally
            free_parts.push(token);
        } else if token == "is:unread" {
            is_unread = Some(true);
        } else if token == "is:read" {
            is_unread = Some(false);
        } else if token.starts_with("is:") || token.starts_with("before:") || token.starts_with("after:") {
            // Pass through to Gmail API as free text
            free_parts.push(token);
        } else {
            free_parts.push(token);
        }
    }

    ParsedQuery {
        from,
        to,
        subject,
        has_attachment,
        is_unread,
        free_text: free_parts.join(" "),
    }
}

/// Performs local-only search: parse operators, FTS5 match, LIKE fallback.
/// Returns a deduplicated list of thread IDs found locally.
pub(crate) async fn search_messages_local(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    query: &str,
) -> Result<Vec<String>, String> {
    let mut all_thread_ids: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let parsed = parse_query_operators(query);

    let has_operators = parsed.from.is_some() || parsed.to.is_some() || parsed.subject.is_some() || parsed.has_attachment || parsed.is_unread.is_some();

    if has_operators {
        let mut conditions = vec!["m.account_id = ?".to_string()];
        let mut binds: Vec<String> = vec![account_id.to_string()];

        if let Some(ref f) = parsed.from {
            conditions.push("m.sender LIKE ?".to_string());
            binds.push(format!("%{}%", f));
        }
        if let Some(ref t) = parsed.to {
            conditions.push("m.recipients LIKE ?".to_string());
            binds.push(format!("%{}%", t));
        }
        if let Some(ref s) = parsed.subject {
            conditions.push("m.subject LIKE ?".to_string());
            binds.push(format!("%{}%", s));
        }
        if parsed.has_attachment {
            conditions.push("m.has_attachments = 1".to_string());
        }
        if let Some(unread) = parsed.is_unread {
            conditions.push("m.is_read = ?".to_string());
            binds.push(if unread { "0".to_string() } else { "1".to_string() });
        }

        let where_clause = conditions.join(" AND ");
        let sql = format!(
            "SELECT DISTINCT m.thread_id FROM messages m WHERE {} LIMIT 50",
            where_clause
        );

        #[derive(sqlx::FromRow)]
        struct TidRow { thread_id: Option<String> }

        let mut q = sqlx::query_as::<_, TidRow>(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        let rows: Vec<TidRow> = q.fetch_all(pool).await.unwrap_or_default();
        for r in rows {
            if let Some(tid) = r.thread_id {
                if seen.insert(tid.clone()) {
                    all_thread_ids.push(tid);
                }
            }
        }
    }

    // FTS5 for free text remainder
    if !parsed.free_text.is_empty() {
        #[derive(sqlx::FromRow)]
        struct FtsRow { thread_id: Option<String> }
        let fts_query = format!("{}*", parsed.free_text.replace('"', ""));
        let local: Vec<FtsRow> = sqlx::query_as(
            "SELECT DISTINCT m.thread_id FROM messages m
             INNER JOIN messages_fts ON messages_fts.rowid = m.rowid
             WHERE messages_fts MATCH ? AND m.account_id = ?
             LIMIT 50",
        )
        .bind(&fts_query)
        .bind(account_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for r in local {
            if let Some(tid) = r.thread_id {
                if seen.insert(tid.clone()) {
                    all_thread_ids.push(tid);
                }
            }
        }

        // LIKE fallback for free text
        #[derive(sqlx::FromRow)]
        struct LikeRow { thread_id: Option<String> }
        let pattern = format!("%{}%", parsed.free_text);
        let like_results: Vec<LikeRow> = sqlx::query_as(
            "SELECT DISTINCT thread_id FROM messages WHERE account_id = ? AND (sender LIKE ? OR subject LIKE ?) LIMIT 30"
        ).bind(account_id).bind(&pattern).bind(&pattern)
        .fetch_all(pool).await.unwrap_or_default();
        for r in like_results {
            if let Some(tid) = r.thread_id {
                if seen.insert(tid.clone()) {
                    all_thread_ids.push(tid);
                }
            }
        }
    }

    Ok(all_thread_ids)
}

#[tauri::command]
pub async fn search_messages(
    app_handle: tauri::AppHandle,
    query: String,
) -> Result<Vec<LocalThread>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    // Local search (operators + FTS5 + LIKE fallback)
    let mut all_thread_ids = search_messages_local(pool.inner(), &account.id, &query).await?;
    let mut seen: std::collections::HashSet<String> = all_thread_ids.iter().cloned().collect();

    // Gmail API search with the full original query (already supports operators)
    let api_ids = search_gmail_api(&account.access_token, &query).await;
    for tid in api_ids {
        if seen.insert(tid.clone()) {
            all_thread_ids.push(tid);
        }
    }

    let mut need_hydrate: Vec<String> = Vec::new();
    for tid in &all_thread_ids {
        #[derive(sqlx::FromRow)]
        struct C { cnt: i32 }
        let cnt =
            sqlx::query_as::<_, C>("SELECT COUNT(*) as cnt FROM messages WHERE thread_id = ?")
                .bind(tid)
                .fetch_one(pool.inner())
                .await
                .map(|r| r.cnt)
                .unwrap_or(0);
        if cnt == 0 {
            let _ = sqlx::query("INSERT OR IGNORE INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0)")
                .bind(tid).bind(&account.id)
                .execute(pool.inner()).await;
            need_hydrate.push(tid.clone());
        }
    }
    if !need_hydrate.is_empty() {
        crate::gmail_api::batch_hydrate_threads(
            pool.inner(),
            &account.id,
            &account.access_token,
            need_hydrate,
        )
        .await;
    }

    fetch_threads_by_ids(pool.inner(), &all_thread_ids, &account.id).await
}

fn auth_gmail_api_url(path: &str) -> String {
    #[cfg(test)]
    {
        let base = std::env::var("TEST_AUTH_GMAIL_API_BASE")
            .unwrap_or_else(|_| "https://gmail.googleapis.com".to_string());
        format!("{}{}", base, path)
    }
    #[cfg(not(test))]
    {
        format!("https://gmail.googleapis.com{}", path)
    }
}

pub(crate) async fn search_gmail_api(access_token: &str, query: &str) -> Vec<String> {
    let client = reqwest::Client::new();
    let res = match client
        .get(auth_gmail_api_url("/gmail/v1/users/me/messages"))
        .query(&[("q", query), ("maxResults", "30")])
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    if !res.status().is_success() {
        return vec![];
    }

    #[derive(serde::Deserialize)]
    struct MsgRef {
        #[allow(dead_code)]
        id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    }
    #[derive(serde::Deserialize)]
    struct MsgsResponse {
        messages: Option<Vec<MsgRef>>,
    }

    match res.json::<MsgsResponse>().await {
        Ok(api_res) => {
            if let Some(msgs) = api_res.messages {
                let mut seen = std::collections::HashSet::new();
                msgs.into_iter()
                    .filter(|m| seen.insert(m.thread_id.clone()))
                    .map(|m| m.thread_id)
                    .collect()
            } else {
                vec![]
            }
        }
        Err(_) => vec![],
    }
}

#[derive(serde::Serialize)]
pub struct HydrationProgress {
    pub total: usize,
    pub hydrated: usize,
}

pub(crate) async fn get_hydration_progress_inner(
    pool: &sqlx::SqlitePool,
    account_id: &str,
) -> Result<HydrationProgress, String> {
    #[derive(sqlx::FromRow)]
    struct Count {
        cnt: i32,
    }

    let total =
        sqlx::query_as::<_, Count>("SELECT COUNT(*) as cnt FROM threads WHERE account_id = ?")
            .bind(account_id)
            .fetch_one(pool)
            .await
            .map(|r| r.cnt)
            .unwrap_or(0) as usize;

    let hydrated = sqlx::query_as::<_, Count>(
        "SELECT COUNT(DISTINCT t.id) as cnt FROM threads t INNER JOIN messages m ON t.id = m.thread_id WHERE t.account_id = ?"
    ).bind(account_id).fetch_one(pool).await.map(|r| r.cnt).unwrap_or(0) as usize;

    Ok(HydrationProgress { total, hydrated })
}

#[tauri::command]
pub async fn get_hydration_progress(
    app_handle: tauri::AppHandle,
) -> Result<HydrationProgress, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    get_hydration_progress_inner(pool.inner(), &account.id).await
}

#[derive(serde::Serialize)]
pub struct SearchSuggestion {
    pub kind: String,
    pub text: String,
    pub detail: String,
}

pub(crate) async fn get_search_suggestions_inner(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    operator: Option<&str>,
    value: &str,
    full_query: &str,
) -> Result<Vec<SearchSuggestion>, String> {
    let mut suggestions = Vec::new();

    match operator {
        Some("from") => {
            if !value.is_empty() {
                #[derive(sqlx::FromRow)]
                struct SenderRow { sender: String }
                let pattern = format!("%{}%", value);
                let contacts: Vec<SenderRow> = sqlx::query_as(
                    "SELECT DISTINCT sender FROM messages WHERE account_id = ? AND sender LIKE ? LIMIT 8",
                )
                .bind(account_id)
                .bind(&pattern)
                .fetch_all(pool)
                .await
                .unwrap_or_default();

                for c in contacts {
                    let display = c.sender.split('<').next().unwrap_or(&c.sender).trim().to_string();
                    suggestions.push(SearchSuggestion {
                        kind: "contact".to_string(),
                        text: display.clone(),
                        detail: c.sender.clone(),
                    });
                }
            }
        }
        Some("to") => {
            if !value.is_empty() {
                #[derive(sqlx::FromRow)]
                struct RecipRow { recipients: String }
                let pattern = format!("%{}%", value);
                let rows: Vec<RecipRow> = sqlx::query_as(
                    "SELECT DISTINCT recipients FROM messages WHERE account_id = ? AND recipients LIKE ? LIMIT 20",
                )
                .bind(account_id)
                .bind(&pattern)
                .fetch_all(pool)
                .await
                .unwrap_or_default();

                let mut seen = std::collections::HashSet::new();
                for row in rows {
                    for p in row.recipients.split(',') {
                        let p = p.trim();
                        if p.is_empty() || !p.to_lowercase().contains(&value.to_lowercase()) {
                            continue;
                        }
                        if !seen.insert(p.to_string()) {
                            continue;
                        }
                        let display = if let Some(bracket_start) = p.find('<') {
                            p[..bracket_start].trim().trim_matches('"').to_string()
                        } else {
                            p.to_string()
                        };
                        suggestions.push(SearchSuggestion {
                            kind: "contact".to_string(),
                            text: display,
                            detail: p.to_string(),
                        });
                        if suggestions.len() >= 8 { break; }
                    }
                    if suggestions.len() >= 8 { break; }
                }
            }
        }
        Some("subject") => {
            if !value.is_empty() {
                #[derive(sqlx::FromRow)]
                struct SubjectRow { subject: String }
                let pattern = format!("%{}%", value);
                let subjects: Vec<SubjectRow> = sqlx::query_as(
                    "SELECT DISTINCT subject FROM messages WHERE account_id = ? AND subject LIKE ? LIMIT 8",
                )
                .bind(account_id)
                .bind(&pattern)
                .fetch_all(pool)
                .await
                .unwrap_or_default();

                for s in subjects {
                    suggestions.push(SearchSuggestion {
                        kind: "subject".to_string(),
                        text: s.subject.clone(),
                        detail: String::new(),
                    });
                }
            }
        }
        _ => {
            #[derive(sqlx::FromRow)]
            struct SettingRow { value: String }
            if let Ok(Some(row)) =
                sqlx::query_as::<_, SettingRow>("SELECT value FROM settings WHERE key = 'recent_searches'")
                    .fetch_optional(pool)
                    .await
            {
                if let Ok(recents) = serde_json::from_str::<Vec<String>>(&row.value) {
                    for r in recents.iter().take(5) {
                        if full_query.is_empty() || r.to_lowercase().contains(&full_query.to_lowercase()) {
                            suggestions.push(SearchSuggestion {
                                kind: "recent".to_string(),
                                text: r.clone(),
                                detail: "Recent search".to_string(),
                            });
                        }
                    }
                }
            }

            if full_query.len() >= 2 {
                #[derive(sqlx::FromRow)]
                struct SenderRow { sender: String }
                let pattern = format!("%{}%", full_query);
                let contacts: Vec<SenderRow> = sqlx::query_as(
                    "SELECT DISTINCT sender FROM messages WHERE account_id = ? AND sender LIKE ? LIMIT 5",
                )
                .bind(account_id)
                .bind(&pattern)
                .fetch_all(pool)
                .await
                .unwrap_or_default();

                for c in contacts {
                    suggestions.push(SearchSuggestion {
                        kind: "contact".to_string(),
                        text: format!("from:{}", c.sender.split('<').next().unwrap_or(&c.sender).trim()),
                        detail: c.sender.clone(),
                    });
                }

                #[derive(sqlx::FromRow)]
                struct SubjectRow { subject: String }
                let subjects: Vec<SubjectRow> = sqlx::query_as(
                    "SELECT DISTINCT subject FROM messages WHERE account_id = ? AND subject LIKE ? LIMIT 3",
                )
                .bind(account_id)
                .bind(&pattern)
                .fetch_all(pool)
                .await
                .unwrap_or_default();

                for s in subjects {
                    suggestions.push(SearchSuggestion {
                        kind: "subject".to_string(),
                        text: format!("subject:{}", s.subject),
                        detail: s.subject.clone(),
                    });
                }
            }
        }
    }

    Ok(suggestions)
}

#[tauri::command]
pub async fn get_search_suggestions(
    app_handle: tauri::AppHandle,
    operator: Option<String>,
    value: String,
    full_query: String,
) -> Result<Vec<SearchSuggestion>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    get_search_suggestions_inner(pool.inner(), &account.id, operator.as_deref(), &value, &full_query).await
}

pub(crate) async fn save_recent_search_inner(pool: &sqlx::SqlitePool, query: &str) -> Result<(), String> {
    #[derive(sqlx::FromRow)]
    struct SettingRow {
        value: String,
    }
    let mut recents: Vec<String> =
        sqlx::query_as::<_, SettingRow>("SELECT value FROM settings WHERE key = 'recent_searches'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None)
            .and_then(|r| serde_json::from_str(&r.value).ok())
            .unwrap_or_default();

    recents.retain(|r| r != query);
    recents.insert(0, query.to_string());
    recents.truncate(10);

    let json = serde_json::to_string(&recents).unwrap_or_default();
    sqlx::query("INSERT INTO settings (key, value) VALUES ('recent_searches', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(&json).execute(pool).await.map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn save_recent_search(app_handle: tauri::AppHandle, query: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    save_recent_search_inner(pool.inner(), &query).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_helpers::*;
    use super::super::settings::get_setting_inner;

    #[tokio::test]
    async fn test_search_messages_fts5() {
        let pool = setup_test_db().await;
        sqlx::query("INSERT INTO accounts (id) VALUES (?)")
            .bind("acc1").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind("msg1").bind("thread1").bind("acc1").bind("sender@example.com").bind("recipient@example.com")
            .bind("Test Subject").bind("snippet").bind(0i64).bind("This is a searchterm inside the body").bind("").bind(0)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT OR REPLACE INTO messages_fts(rowid, sender, subject, body_plain) SELECT rowid, sender, subject, body_plain FROM messages WHERE id = ?")
            .bind("msg1").execute(&pool).await.unwrap();
        let fts_query = "searchterm*".to_string();
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT m.thread_id FROM messages m INNER JOIN messages_fts ON messages_fts.rowid = m.rowid WHERE messages_fts MATCH ? AND m.account_id = ?"
        ).bind(&fts_query).bind("acc1").fetch_all(&pool).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "thread1");
    }

    // ===== parse_query_operators tests =====

    #[test]
    fn test_parse_query_simple_free_text() {
        let p = parse_query_operators("hello world");
        assert_eq!(p.free_text, "hello world");
        assert!(p.from.is_none());
        assert!(p.to.is_none());
        assert!(p.subject.is_none());
        assert!(!p.has_attachment);
        assert!(p.is_unread.is_none());
    }

    #[test]
    fn test_parse_query_from_operator() {
        let p = parse_query_operators("from:alice@example.com");
        assert_eq!(p.from, Some("alice@example.com".to_string()));
        assert!(p.free_text.is_empty());
    }

    #[test]
    fn test_parse_query_to_operator() {
        let p = parse_query_operators("to:bob@example.com");
        assert_eq!(p.to, Some("bob@example.com".to_string()));
        assert!(p.free_text.is_empty());
    }

    #[test]
    fn test_parse_query_subject_operator() {
        let p = parse_query_operators("subject:meeting");
        assert_eq!(p.subject, Some("meeting".to_string()));
        assert!(p.free_text.is_empty());
    }

    #[test]
    fn test_parse_query_quoted_values() {
        let p = parse_query_operators("from:\"Alice Doe\"");
        assert_eq!(p.from, Some("Alice Doe".to_string()));
    }

    #[test]
    fn test_parse_query_has_attachment() {
        let p = parse_query_operators("has:attachment");
        assert!(p.has_attachment);
        assert!(p.free_text.is_empty());
    }

    #[test]
    fn test_parse_query_is_unread() {
        let p = parse_query_operators("is:unread");
        assert_eq!(p.is_unread, Some(true));
    }

    #[test]
    fn test_parse_query_is_read() {
        let p = parse_query_operators("is:read");
        assert_eq!(p.is_unread, Some(false));
    }

    #[test]
    fn test_parse_query_mixed_operators() {
        let p = parse_query_operators("from:alice subject:meeting hello");
        assert_eq!(p.from, Some("alice".to_string()));
        assert_eq!(p.subject, Some("meeting".to_string()));
        assert_eq!(p.free_text, "hello");
    }

    #[test]
    fn test_parse_query_passthrough_operators() {
        let p = parse_query_operators("before:2024/01/01");
        assert!(p.free_text.contains("before:2024/01/01"));
        assert!(p.from.is_none());
    }

    #[test]
    fn test_parse_query_has_link_passthrough() {
        let p = parse_query_operators("has:link");
        assert!(p.free_text.contains("has:link"));
    }

    #[test]
    fn test_parse_query_empty_operator_value() {
        let p = parse_query_operators("from: something");
        assert!(p.from.is_none());
        assert_eq!(p.free_text, "something");
    }

    // ===== get_hydration_progress_inner tests =====

    #[tokio::test]
    async fn test_get_hydration_progress_inner_empty() {
        let pool = setup_test_db().await;
        let progress = get_hydration_progress_inner(&pool, "acc1").await.unwrap();
        assert_eq!(progress.total, 0);
        assert_eq!(progress.hydrated, 0);
    }

    #[tokio::test]
    async fn test_get_hydration_progress_inner_partial() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_thread(&pool, "t3", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;

        let progress = get_hydration_progress_inner(&pool, "acc1").await.unwrap();
        assert_eq!(progress.total, 3);
        assert_eq!(progress.hydrated, 1);
    }

    #[tokio::test]
    async fn test_get_hydration_progress_inner_all_hydrated() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "s@t.com", "", "Sub", 2000).await;

        let progress = get_hydration_progress_inner(&pool, "acc1").await.unwrap();
        assert_eq!(progress.total, 2);
        assert_eq!(progress.hydrated, 2);
    }

    #[tokio::test]
    async fn test_get_hydration_progress_inner_account_isolation() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc2").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;

        let progress = get_hydration_progress_inner(&pool, "acc1").await.unwrap();
        assert_eq!(progress.total, 1);
        assert_eq!(progress.hydrated, 1);
    }

    // ===== get_search_suggestions_inner tests =====

    #[tokio::test]
    async fn test_get_search_suggestions_inner_from_operator() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <alice@example.com>", "", "Subject", 1000).await;

        let suggestions = get_search_suggestions_inner(&pool, "acc1", Some("from"), "alice", "").await.unwrap();
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].kind, "contact");
    }

    #[tokio::test]
    async fn test_get_search_suggestions_inner_subject_operator() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Weekly Meeting Notes", 1000).await;

        let suggestions = get_search_suggestions_inner(&pool, "acc1", Some("subject"), "meeting", "").await.unwrap();
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].kind, "subject");
        assert!(suggestions[0].text.contains("Weekly Meeting Notes"));
    }

    #[tokio::test]
    async fn test_get_search_suggestions_inner_free_text_with_recents() {
        let pool = setup_test_db().await;
        save_recent_search_inner(&pool, "project alpha").await.unwrap();
        save_recent_search_inner(&pool, "budget report").await.unwrap();

        let suggestions = get_search_suggestions_inner(&pool, "acc1", None, "", "").await.unwrap();
        let recent_suggestions: Vec<&SearchSuggestion> = suggestions.iter().filter(|s| s.kind == "recent").collect();
        assert_eq!(recent_suggestions.len(), 2);
    }

    #[tokio::test]
    async fn test_get_search_suggestions_inner_free_text_filters_recents() {
        let pool = setup_test_db().await;
        save_recent_search_inner(&pool, "project alpha").await.unwrap();
        save_recent_search_inner(&pool, "budget report").await.unwrap();

        let suggestions = get_search_suggestions_inner(&pool, "acc1", None, "", "project").await.unwrap();
        let recent_suggestions: Vec<&SearchSuggestion> = suggestions.iter().filter(|s| s.kind == "recent").collect();
        assert_eq!(recent_suggestions.len(), 1);
        assert!(recent_suggestions[0].text.contains("project"));
    }

    #[tokio::test]
    async fn test_get_search_suggestions_inner_free_text_contacts_and_subjects() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <alice@example.com>", "", "Budget Report Q4", 1000).await;

        let suggestions = get_search_suggestions_inner(&pool, "acc1", None, "", "budget").await.unwrap();
        let contact_count = suggestions.iter().filter(|s| s.kind == "contact").count();
        let subject_count = suggestions.iter().filter(|s| s.kind == "subject").count();
        assert!(contact_count > 0 || subject_count > 0);
    }

    #[tokio::test]
    async fn test_get_search_suggestions_inner_to_operator() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "Bob <bob@example.com>", "Hi", 1000).await;

        let suggestions = get_search_suggestions_inner(&pool, "acc1", Some("to"), "bob", "").await.unwrap();
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].kind, "contact");
    }

    #[tokio::test]
    async fn test_get_search_suggestions_inner_empty_value() {
        let pool = setup_test_db().await;
        let suggestions = get_search_suggestions_inner(&pool, "acc1", Some("from"), "", "").await.unwrap();
        assert!(suggestions.is_empty());
    }

    #[tokio::test]
    async fn test_get_search_suggestions_inner_to_operator_dedup() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, recipients, sender, subject, internal_date) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind("m1").bind("t1").bind("acc1")
            .bind("alice@test.com, bob@test.com, alice@test.com")
            .bind("sender@test.com").bind("Test").bind(1000i64)
            .execute(&pool).await.unwrap();

        let suggestions = get_search_suggestions_inner(&pool, "acc1", Some("to"), "alice", "").await.unwrap();
        let alice_count = suggestions.iter().filter(|s| s.detail.contains("alice")).count();
        assert_eq!(alice_count, 1);
    }

    // ===== save_recent_search_inner tests =====

    #[tokio::test]
    async fn test_save_recent_search_inner_stores_search() {
        let pool = setup_test_db().await;
        save_recent_search_inner(&pool, "test query").await.unwrap();

        let val = get_setting_inner(&pool, "recent_searches").await.unwrap();
        let recents: Vec<String> = serde_json::from_str(&val).unwrap();
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0], "test query");
    }

    #[tokio::test]
    async fn test_save_recent_search_inner_ordering() {
        let pool = setup_test_db().await;
        save_recent_search_inner(&pool, "first").await.unwrap();
        save_recent_search_inner(&pool, "second").await.unwrap();

        let val = get_setting_inner(&pool, "recent_searches").await.unwrap();
        let recents: Vec<String> = serde_json::from_str(&val).unwrap();
        assert_eq!(recents[0], "second");
        assert_eq!(recents[1], "first");
    }

    #[tokio::test]
    async fn test_save_recent_search_inner_deduplication() {
        let pool = setup_test_db().await;
        save_recent_search_inner(&pool, "query").await.unwrap();
        save_recent_search_inner(&pool, "other").await.unwrap();
        save_recent_search_inner(&pool, "query").await.unwrap();

        let val = get_setting_inner(&pool, "recent_searches").await.unwrap();
        let recents: Vec<String> = serde_json::from_str(&val).unwrap();
        assert_eq!(recents.len(), 2);
        assert_eq!(recents[0], "query");
        assert_eq!(recents[1], "other");
    }

    #[tokio::test]
    async fn test_save_recent_search_inner_truncation() {
        let pool = setup_test_db().await;
        for i in 0..15 {
            save_recent_search_inner(&pool, &format!("query_{}", i)).await.unwrap();
        }

        let val = get_setting_inner(&pool, "recent_searches").await.unwrap();
        let recents: Vec<String> = serde_json::from_str(&val).unwrap();
        assert_eq!(recents.len(), 10);
        assert_eq!(recents[0], "query_14");
    }

    // ===== search_messages_local tests =====

    #[tokio::test]
    async fn test_search_messages_local_empty_db() {
        let pool = setup_test_db().await;
        let result = search_messages_local(&pool, "acc1", "hello").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_search_messages_local_like_fallback_sender() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <alice@example.com>", "bob@test.com", "Hello", 1000).await;

        let result = search_messages_local(&pool, "acc1", "alice").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "t1");
    }

    #[tokio::test]
    async fn test_search_messages_local_like_fallback_subject() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Important Meeting Notes", 1000).await;

        let result = search_messages_local(&pool, "acc1", "meeting").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "t1");
    }

    #[tokio::test]
    async fn test_search_messages_local_fts5() {
        let pool = setup_test_db().await;
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind("m1").bind("t1").bind("acc1").bind("s@t.com").bind("")
            .bind("Subject").bind("").bind(1000i64).bind("unique_searchterm_xyz in body").bind("").bind(0)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT OR REPLACE INTO messages_fts(rowid, sender, subject, body_plain) SELECT rowid, sender, subject, body_plain FROM messages WHERE id = ?")
            .bind("m1").execute(&pool).await.unwrap();

        let result = search_messages_local(&pool, "acc1", "unique_searchterm_xyz").await.unwrap();
        assert!(result.contains(&"t1".to_string()));
    }

    #[tokio::test]
    async fn test_search_messages_local_from_operator() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <alice@example.com>", "", "Sub", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "Bob <bob@example.com>", "", "Sub", 2000).await;

        let result = search_messages_local(&pool, "acc1", "from:alice").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "t1");
    }

    #[tokio::test]
    async fn test_search_messages_local_subject_operator() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Budget Review", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "s@t.com", "", "Project Update", 2000).await;

        let result = search_messages_local(&pool, "acc1", "subject:budget").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "t1");
    }

    #[tokio::test]
    async fn test_search_messages_local_deduplicates() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@test.com", "", "Hello Alice", 1000).await;
        insert_message(&pool, "m2", "t1", "acc1", "alice@test.com", "", "Reply Alice", 2000).await;

        let result = search_messages_local(&pool, "acc1", "alice").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "t1");
    }

    #[tokio::test]
    async fn test_search_messages_local_account_isolation() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@test.com", "", "Hello", 1000).await;
        insert_message(&pool, "m2", "t2", "acc2", "alice@test.com", "", "Hello", 2000).await;

        let result = search_messages_local(&pool, "acc1", "alice").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "t1");
    }

    // ===== search_gmail_api tests =====

    use std::sync::Mutex;
    static AUTH_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn test_search_gmail_api_success() {
        let _lock = AUTH_ENV_LOCK.lock().unwrap();
        let server = httpmock::MockServer::start();
        std::env::set_var("TEST_AUTH_GMAIL_API_BASE", server.base_url());

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/messages");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "messages": [
                        {"id": "m1", "threadId": "t1"},
                        {"id": "m2", "threadId": "t2"},
                        {"id": "m3", "threadId": "t1"}
                    ]
                }));
        });

        let result = search_gmail_api("fake_token", "test query").await;
        mock.assert();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"t1".to_string()));
        assert!(result.contains(&"t2".to_string()));

        std::env::remove_var("TEST_AUTH_GMAIL_API_BASE");
    }

    #[tokio::test]
    async fn test_search_gmail_api_empty_response() {
        let _lock = AUTH_ENV_LOCK.lock().unwrap();
        let server = httpmock::MockServer::start();
        std::env::set_var("TEST_AUTH_GMAIL_API_BASE", server.base_url());

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/messages");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({}));
        });

        let result = search_gmail_api("fake_token", "no results").await;
        mock.assert();
        assert!(result.is_empty());

        std::env::remove_var("TEST_AUTH_GMAIL_API_BASE");
    }

    #[tokio::test]
    async fn test_search_gmail_api_http_error() {
        let _lock = AUTH_ENV_LOCK.lock().unwrap();
        let server = httpmock::MockServer::start();
        std::env::set_var("TEST_AUTH_GMAIL_API_BASE", server.base_url());

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/messages");
            then.status(401);
        });

        let result = search_gmail_api("bad_token", "query").await;
        mock.assert();
        assert!(result.is_empty());

        std::env::remove_var("TEST_AUTH_GMAIL_API_BASE");
    }

    #[tokio::test]
    async fn test_search_gmail_api_deduplicates_threads() {
        let _lock = AUTH_ENV_LOCK.lock().unwrap();
        let server = httpmock::MockServer::start();
        std::env::set_var("TEST_AUTH_GMAIL_API_BASE", server.base_url());

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/messages");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "messages": [
                        {"id": "m1", "threadId": "t1"},
                        {"id": "m2", "threadId": "t1"},
                        {"id": "m3", "threadId": "t1"}
                    ]
                }));
        });

        let result = search_gmail_api("fake_token", "query").await;
        mock.assert();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "t1");

        std::env::remove_var("TEST_AUTH_GMAIL_API_BASE");
    }
}
