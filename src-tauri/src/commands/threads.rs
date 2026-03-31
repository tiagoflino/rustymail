use super::accounts::get_active_account;
use tauri::Manager;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct LocalThread {
    pub id: String,
    pub snippet: String,
    pub history_id: String,
    pub unread: i32,
    pub sender: String,
    pub subject: String,
    pub internal_date: i64,
    pub starred: bool,
    pub star_type: Option<String>,
    pub has_attachments: bool,
    pub important: bool,
    pub account_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThreadCategory {
    Primary,
    Social,
    Promotions,
    Important,
}

impl ThreadCategory {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "primary" => Some(ThreadCategory::Primary),
            "social" => Some(ThreadCategory::Social),
            "promotions" => Some(ThreadCategory::Promotions),
            "important" => Some(ThreadCategory::Important),
            _ => None,
        }
    }
}

pub(crate) fn clean_sender_name(raw: Option<String>) -> String {
    let mut s = raw.unwrap_or_else(|| "Unknown Sender".to_string());
    if let Some(idx) = s.find('<') {
        let name = s[..idx].trim();
        if !name.is_empty() {
            s = name.to_string();
        } else {
            s = s.replace("<", "").replace(">", "").trim().to_string();
        }
    }
    s.replace("\"", "")
}

pub(crate) async fn get_threads_inner(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    label_id: Option<&str>,
    category: Option<ThreadCategory>,
    offset: i32,
    limit: i32,
) -> Result<Vec<LocalThread>, String> {
    #[derive(sqlx::FromRow)]
    struct TR {
        id: String,
        snippet: Option<String>,
        history_id: Option<String>,
        unread: Option<i32>,
        sender: Option<String>,
        subject: Option<String>,
        msg_date: Option<i64>,
        starred: Option<i32>,
        star_type: Option<String>,
        has_attachments: Option<i32>,
        important: Option<i32>,
        account_id: String,
    }

    // Only return threads that have been hydrated (have at least one message)
    let base_select = r#"
        SELECT t.id, t.snippet, t.history_id, t.unread,
                t.sender as sender,
                t.subject as subject,
                t.latest_date as msg_date,
                EXISTS (SELECT 1 FROM thread_labels tl WHERE tl.thread_id = t.id AND tl.label_id = 'STARRED') as starred,
                (SELECT tls.label_id FROM thread_labels tls
                 WHERE tls.thread_id = t.id
                 AND tls.label_id IN ('YELLOW_STAR','ORANGE_STAR','RED_STAR','PURPLE_STAR','BLUE_STAR','GREEN_STAR','GREEN_CIRCLE','RED_CIRCLE','ORANGE_CIRCLE','YELLOW_CIRCLE','BLUE_CIRCLE','PURPLE_CIRCLE')
                 LIMIT 1) as star_type,
                EXISTS (SELECT 1 FROM messages m6 WHERE m6.thread_id = t.id AND m6.has_attachments = 1) as has_attachments,
                EXISTS (SELECT 1 FROM thread_labels tl2 WHERE tl2.thread_id = t.id AND tl2.label_id = 'IMPORTANT') as important,
                t.account_id
         FROM threads t
    "#;
    let hydrated_filter = "t.metadata_synced = 1";

    let hf = hydrated_filter;
    let (sql, binds): (String, Vec<String>) = match (label_id, category) {
        (Some(_lid), Some(cat)) => {
            match cat {
                ThreadCategory::Primary => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        LEFT JOIN thread_labels tl_social ON t.id = tl_social.thread_id AND tl_social.label_id = 'CATEGORY_SOCIAL'
                        LEFT JOIN thread_labels tl_promo ON t.id = tl_promo.thread_id AND tl_promo.label_id = 'CATEGORY_PROMOTIONS'
                        WHERE t.account_id = ? AND {hf}
                          AND tl_social.thread_id IS NULL AND tl_promo.thread_id IS NULL
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select);
                    (sql, vec![account_id.to_string()])
                },
                ThreadCategory::Social => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_SOCIAL'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id = ? AND {hf}
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select);
                    (sql, vec![account_id.to_string()])
                },
                ThreadCategory::Promotions => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_PROMOTIONS'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id = ? AND {hf}
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select);
                    (sql, vec![account_id.to_string()])
                },
                ThreadCategory::Important => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'IMPORTANT'
                        WHERE t.account_id = ? AND {hf}
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select);
                    (sql, vec![account_id.to_string()])
                },
            }
        },
        (Some(lid), None) => {
            let sql = format!(
                "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = ?
                WHERE t.account_id = ? AND {hf}
                ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                LIMIT ? OFFSET ?", base_select);
            (sql, vec![lid.to_string(), account_id.to_string()])
        },
        (None, Some(cat)) => {
            match cat {
                ThreadCategory::Primary => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        LEFT JOIN thread_labels tl_social ON t.id = tl_social.thread_id AND tl_social.label_id = 'CATEGORY_SOCIAL'
                        LEFT JOIN thread_labels tl_promo ON t.id = tl_promo.thread_id AND tl_promo.label_id = 'CATEGORY_PROMOTIONS'
                        WHERE t.account_id = ? AND {hf}
                          AND tl_social.thread_id IS NULL AND tl_promo.thread_id IS NULL
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select);
                    (sql, vec![account_id.to_string()])
                },
                ThreadCategory::Social => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_SOCIAL'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id = ? AND {hf}
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select);
                    (sql, vec![account_id.to_string()])
                },
                ThreadCategory::Promotions => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_PROMOTIONS'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id = ? AND {hf}
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select);
                    (sql, vec![account_id.to_string()])
                },
                ThreadCategory::Important => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'IMPORTANT'
                        WHERE t.account_id = ? AND {hf}
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select);
                    (sql, vec![account_id.to_string()])
                },
            }
        },
        (None, None) => {
            let sql = format!(
                "{} WHERE t.account_id = ? AND {hf}
                ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                LIMIT ? OFFSET ?", base_select);
            (sql, vec![account_id.to_string()])
        },
    };

    let mut query = sqlx::query_as::<_, TR>(&sql);
    for bind in &binds {
        query = query.bind(bind);
    }
    query = query.bind(limit).bind(offset);

    let rows: Vec<TR> = query.fetch_all(pool).await.map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| LocalThread {
            id: r.id,
            snippet: r.snippet.unwrap_or_default(),
            history_id: r.history_id.unwrap_or_default(),
            unread: r.unread.unwrap_or(0),
            sender: clean_sender_name(r.sender),
            subject: r.subject.unwrap_or_else(|| "No Subject".to_string()),
            internal_date: r.msg_date.unwrap_or(0),
            starred: r.starred.unwrap_or(0) == 1,
            star_type: r.star_type,
            has_attachments: r.has_attachments.unwrap_or(0) == 1,
            important: r.important.unwrap_or(0) == 1,
            account_id: r.account_id,
        })
        .collect())
}

#[derive(serde::Serialize)]
pub struct ThreadCountResult {
    pub count: i64,
    pub has_more_remote: bool,
}

pub(crate) async fn get_thread_count_inner(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    label_id: Option<&str>,
    category: Option<ThreadCategory>,
    token_store: &crate::page_token_store::PageTokenStore,
) -> Result<ThreadCountResult, String> {
    let base_select = "SELECT COUNT(DISTINCT t.id) FROM threads t";
    let hydrated_filter = "t.metadata_synced = 1";

    let hf = hydrated_filter;
    let (sql, binds): (String, Vec<String>) = match (label_id, category) {
        (Some(_lid), Some(cat)) => {
            match cat {
                ThreadCategory::Primary => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        LEFT JOIN thread_labels tl_social ON t.id = tl_social.thread_id AND tl_social.label_id = 'CATEGORY_SOCIAL'
                        LEFT JOIN thread_labels tl_promo ON t.id = tl_promo.thread_id AND tl_promo.label_id = 'CATEGORY_PROMOTIONS'
                        WHERE t.account_id = ? AND {hf}
                          AND tl_social.thread_id IS NULL AND tl_promo.thread_id IS NULL", base_select);
                    (sql, vec![account_id.to_string()])
                },
                ThreadCategory::Social => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_SOCIAL'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id = ? AND {hf}", base_select);
                    (sql, vec![account_id.to_string()])
                },
                ThreadCategory::Promotions => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_PROMOTIONS'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id = ? AND {hf}", base_select);
                    (sql, vec![account_id.to_string()])
                },
                ThreadCategory::Important => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'IMPORTANT'
                        WHERE t.account_id = ? AND {hf}", base_select);
                    (sql, vec![account_id.to_string()])
                },
            }
        },
        (Some(lid), None) => {
            let sql = format!(
                "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = ?
                WHERE t.account_id = ? AND {hf}", base_select);
            (sql, vec![lid.to_string(), account_id.to_string()])
        },
        (None, Some(cat)) => {
            match cat {
                ThreadCategory::Primary => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        LEFT JOIN thread_labels tl_social ON t.id = tl_social.thread_id AND tl_social.label_id = 'CATEGORY_SOCIAL'
                        LEFT JOIN thread_labels tl_promo ON t.id = tl_promo.thread_id AND tl_promo.label_id = 'CATEGORY_PROMOTIONS'
                        WHERE t.account_id = ? AND {hf}
                          AND tl_social.thread_id IS NULL AND tl_promo.thread_id IS NULL", base_select);
                    (sql, vec![account_id.to_string()])
                },
                ThreadCategory::Social => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_SOCIAL'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id = ? AND {hf}", base_select);
                    (sql, vec![account_id.to_string()])
                },
                ThreadCategory::Promotions => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_PROMOTIONS'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id = ? AND {hf}", base_select);
                    (sql, vec![account_id.to_string()])
                },
                ThreadCategory::Important => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'IMPORTANT'
                        WHERE t.account_id = ? AND {hf}", base_select);
                    (sql, vec![account_id.to_string()])
                },
            }
        },
        (None, None) => {
            let sql = format!(
                "{} WHERE t.account_id = ? AND {hf}", base_select);
            (sql, vec![account_id.to_string()])
        },
    };

    let mut query = sqlx::query_scalar::<_, i64>(&sql);
    for bind in &binds {
        query = query.bind(bind);
    }

    let count = query.fetch_one(pool).await.map_err(|e| e.to_string())?;

    let token_key = match category {
        Some(cat) => {
            let cat_name = match cat {
                ThreadCategory::Primary => "primary",
                ThreadCategory::Social => "social",
                ThreadCategory::Promotions => "promotions",
                ThreadCategory::Important => "important",
            };
            format!("{}:category:{}", account_id, cat_name)
        },
        None => match label_id {
            Some(lid) => format!("{}:{}", account_id, lid),
            None => format!("{}:INBOX", account_id),
        },
    };
    let has_more_remote = token_store.get(&token_key).is_some();

    Ok(ThreadCountResult { count, has_more_remote })
}

#[tauri::command]
pub async fn get_thread_count(
    app_handle: tauri::AppHandle,
    label_id: Option<String>,
    category: Option<String>,
) -> Result<ThreadCountResult, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let cat = category.and_then(|c| ThreadCategory::from_str(&c));
    let token_store = app_handle.state::<crate::page_token_store::PageTokenStore>();
    get_thread_count_inner(pool.inner(), &account.id, label_id.as_deref(), cat, token_store.inner()).await
}

#[tauri::command]
pub async fn get_threads(
    app_handle: tauri::AppHandle,
    label_id: Option<String>,
    category: Option<String>,
    offset: Option<i32>,
    limit: Option<i32>,
) -> Result<Vec<LocalThread>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let lim = limit.unwrap_or(50);
    let off = offset.unwrap_or(0);
    let cat = category.and_then(|c| ThreadCategory::from_str(&c));
    get_threads_inner(pool.inner(), &account.id, label_id.as_deref(), cat, off, lim).await
}

pub(crate) async fn get_unified_threads_inner(
    pool: &sqlx::SqlitePool,
    account_ids: &[String],
    label_id: Option<&str>,
    category: Option<ThreadCategory>,
    offset: i32,
    limit: i32,
) -> Result<Vec<LocalThread>, String> {
    if account_ids.is_empty() {
        return Ok(vec![]);
    }

    #[derive(sqlx::FromRow)]
    struct TR {
        id: String,
        snippet: Option<String>,
        history_id: Option<String>,
        unread: Option<i32>,
        sender: Option<String>,
        subject: Option<String>,
        msg_date: Option<i64>,
        starred: Option<i32>,
        star_type: Option<String>,
        has_attachments: Option<i32>,
        important: Option<i32>,
        account_id: String,
    }

    let base_select = r#"
        SELECT t.id, t.snippet, t.history_id, t.unread,
                t.sender as sender,
                t.subject as subject,
                t.latest_date as msg_date,
                EXISTS (SELECT 1 FROM thread_labels tl WHERE tl.thread_id = t.id AND tl.label_id = 'STARRED') as starred,
                (SELECT tls.label_id FROM thread_labels tls
                 WHERE tls.thread_id = t.id
                 AND tls.label_id IN ('YELLOW_STAR','ORANGE_STAR','RED_STAR','PURPLE_STAR','BLUE_STAR','GREEN_STAR','GREEN_CIRCLE','RED_CIRCLE','ORANGE_CIRCLE','YELLOW_CIRCLE','BLUE_CIRCLE','PURPLE_CIRCLE')
                 LIMIT 1) as star_type,
                EXISTS (SELECT 1 FROM messages m6 WHERE m6.thread_id = t.id AND m6.has_attachments = 1) as has_attachments,
                EXISTS (SELECT 1 FROM thread_labels tl2 WHERE tl2.thread_id = t.id AND tl2.label_id = 'IMPORTANT') as important,
                t.account_id
         FROM threads t
    "#;
    let hydrated_filter = "t.metadata_synced = 1";

    let hf = hydrated_filter;
    let placeholders = vec!["?"; account_ids.len()].join(", ");
    
    let (sql, binds): (String, Vec<String>) = match (label_id, category) {
        (Some(_lid), Some(cat)) => {
            match cat {
                ThreadCategory::Primary => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        LEFT JOIN thread_labels tl_social ON t.id = tl_social.thread_id AND tl_social.label_id = 'CATEGORY_SOCIAL'
                        LEFT JOIN thread_labels tl_promo ON t.id = tl_promo.thread_id AND tl_promo.label_id = 'CATEGORY_PROMOTIONS'
                        WHERE t.account_id IN ({}) AND {}
                          AND tl_social.thread_id IS NULL AND tl_promo.thread_id IS NULL
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
                ThreadCategory::Social => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_SOCIAL'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id IN ({}) AND {}
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
                ThreadCategory::Promotions => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_PROMOTIONS'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id IN ({}) AND {}
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
                ThreadCategory::Important => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'IMPORTANT'
                        WHERE t.account_id IN ({}) AND {}
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
            }
        },
        (Some(lid), None) => {
            let sql = format!(
                "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = ?
                WHERE t.account_id IN ({}) AND {}
                ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                LIMIT ? OFFSET ?", base_select, placeholders, hf);
            let mut b = vec![lid.to_string()];
            b.extend(account_ids.to_vec());
            (sql, b)
        },
        (None, Some(cat)) => {
            match cat {
                ThreadCategory::Primary => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        LEFT JOIN thread_labels tl_social ON t.id = tl_social.thread_id AND tl_social.label_id = 'CATEGORY_SOCIAL'
                        LEFT JOIN thread_labels tl_promo ON t.id = tl_promo.thread_id AND tl_promo.label_id = 'CATEGORY_PROMOTIONS'
                        WHERE t.account_id IN ({}) AND {}
                          AND tl_social.thread_id IS NULL AND tl_promo.thread_id IS NULL
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
                ThreadCategory::Social => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_SOCIAL'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id IN ({}) AND {}
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
                ThreadCategory::Promotions => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_PROMOTIONS'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id IN ({}) AND {}
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
                ThreadCategory::Important => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'IMPORTANT'
                        WHERE t.account_id IN ({}) AND {}
                        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                        LIMIT ? OFFSET ?", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
            }
        },
        (None, None) => {
            let sql = format!(
                "{} WHERE t.account_id IN ({}) AND {}
                ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
                LIMIT ? OFFSET ?", base_select, placeholders, hf);
            (sql, account_ids.to_vec())
        },
    };

    let mut query = sqlx::query_as::<_, TR>(&sql);
    for bind in &binds {
        query = query.bind(bind);
    }
    query = query.bind(limit).bind(offset);

    let rows: Vec<TR> = query.fetch_all(pool).await.map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| LocalThread {
            id: r.id,
            snippet: r.snippet.unwrap_or_default(),
            history_id: r.history_id.unwrap_or_default(),
            unread: r.unread.unwrap_or(0),
            sender: clean_sender_name(r.sender),
            subject: r.subject.unwrap_or_else(|| "No Subject".to_string()),
            internal_date: r.msg_date.unwrap_or(0),
            starred: r.starred.unwrap_or(0) == 1,
            star_type: r.star_type,
            has_attachments: r.has_attachments.unwrap_or(0) == 1,
            important: r.important.unwrap_or(0) == 1,
            account_id: r.account_id,
        })
        .collect())
}

#[tauri::command]
pub async fn get_unified_threads(
    app_handle: tauri::AppHandle,
    account_ids: Vec<String>,
    label_id: Option<String>,
    category: Option<String>,
    offset: Option<i32>,
    limit: Option<i32>,
) -> Result<Vec<LocalThread>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let lim = limit.unwrap_or(50);
    let off = offset.unwrap_or(0);
    let cat = category.and_then(|c| ThreadCategory::from_str(&c));
    get_unified_threads_inner(pool.inner(), &account_ids, label_id.as_deref(), cat, off, lim).await
}

pub(crate) async fn get_unified_thread_count_inner(
    pool: &sqlx::SqlitePool,
    account_ids: &[String],
    label_id: Option<&str>,
    category: Option<ThreadCategory>,
) -> Result<ThreadCountResult, String> {
    if account_ids.is_empty() {
        return Ok(ThreadCountResult { count: 0, has_more_remote: false });
    }

    let base_select = "SELECT COUNT(DISTINCT t.id) FROM threads t";
    let hydrated_filter = "t.metadata_synced = 1";

    let hf = hydrated_filter;
    let placeholders = vec!["?"; account_ids.len()].join(", ");

    let (sql, binds): (String, Vec<String>) = match (label_id, category) {
        (Some(_lid), Some(cat)) => {
            match cat {
                ThreadCategory::Primary => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        LEFT JOIN thread_labels tl_social ON t.id = tl_social.thread_id AND tl_social.label_id = 'CATEGORY_SOCIAL'
                        LEFT JOIN thread_labels tl_promo ON t.id = tl_promo.thread_id AND tl_promo.label_id = 'CATEGORY_PROMOTIONS'
                        WHERE t.account_id IN ({}) AND {}
                          AND tl_social.thread_id IS NULL AND tl_promo.thread_id IS NULL", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
                ThreadCategory::Social => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_SOCIAL'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id IN ({}) AND {}", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
                ThreadCategory::Promotions => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_PROMOTIONS'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id IN ({}) AND {}", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
                ThreadCategory::Important => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'IMPORTANT'
                        WHERE t.account_id IN ({}) AND {}", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
            }
        },
        (Some(lid), None) => {
            let sql = format!(
                "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = ?
                WHERE t.account_id IN ({}) AND {}", base_select, placeholders, hf);
            let mut b = vec![lid.to_string()];
            b.extend(account_ids.to_vec());
            (sql, b)
        },
        (None, Some(cat)) => {
            match cat {
                ThreadCategory::Primary => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        LEFT JOIN thread_labels tl_social ON t.id = tl_social.thread_id AND tl_social.label_id = 'CATEGORY_SOCIAL'
                        LEFT JOIN thread_labels tl_promo ON t.id = tl_promo.thread_id AND tl_promo.label_id = 'CATEGORY_PROMOTIONS'
                        WHERE t.account_id IN ({}) AND {}
                          AND tl_social.thread_id IS NULL AND tl_promo.thread_id IS NULL", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
                ThreadCategory::Social => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_SOCIAL'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id IN ({}) AND {}", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
                ThreadCategory::Promotions => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'CATEGORY_PROMOTIONS'
                        INNER JOIN thread_labels tl_inbox ON t.id = tl_inbox.thread_id AND tl_inbox.label_id = 'INBOX'
                        WHERE t.account_id IN ({}) AND {}", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
                ThreadCategory::Important => {
                    let sql = format!(
                        "{} INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = 'IMPORTANT'
                        WHERE t.account_id IN ({}) AND {}", base_select, placeholders, hf);
                    (sql, account_ids.to_vec())
                },
            }
        },
        (None, None) => {
            let sql = format!(
                "{} WHERE t.account_id IN ({}) AND {}", base_select, placeholders, hf);
            (sql, account_ids.to_vec())
        },
    };

    let mut query = sqlx::query_scalar::<_, i64>(&sql);
    for bind in &binds {
        query = query.bind(bind);
    }

    let count = query.fetch_one(pool).await.map_err(|e| e.to_string())?;

    Ok(ThreadCountResult { count, has_more_remote: false })
}

#[tauri::command]
pub async fn get_unified_thread_count(
    app_handle: tauri::AppHandle,
    account_ids: Vec<String>,
    label_id: Option<String>,
    category: Option<String>,
) -> Result<ThreadCountResult, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let cat = category.and_then(|c| ThreadCategory::from_str(&c));
    get_unified_thread_count_inner(pool.inner(), &account_ids, label_id.as_deref(), cat).await
}

const ON_DEMAND_BATCH_SIZE: i32 = 100;

#[tauri::command]
pub async fn fetch_label_threads(
    app_handle: tauri::AppHandle,
    label_id: String,
    account_id: Option<String>,
) -> Result<bool, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = match account_id {
        Some(id) => super::accounts::get_account_by_id(pool.inner(), &id).await?,
        None => get_active_account(pool.inner()).await?,
    };
    let token_store = app_handle.state::<crate::page_token_store::PageTokenStore>();
    let store_key = format!("{}:{}", account.id, label_id);
    let current_token = token_store.get(&store_key);

    println!("[OnDemand] Fetching threads for label: {} (has_token: {})", label_id, current_token.is_some());
    let result = crate::gmail_api::fetch_and_store_threads(
        pool.inner(),
        &account.id,
        &account.access_token,
        Some(&[label_id.as_str()]),
        ON_DEMAND_BATCH_SIZE,
        current_token.as_deref(),
        None,
    )
    .await?;

    let has_more = result.next_page_token.is_some();
    match result.next_page_token {
        Some(token) => token_store.set(&store_key, token),
        None => token_store.remove(&store_key),
    }

    if !result.thread_ids.is_empty() {
        let to_hydrate = filter_no_metadata(pool.inner(), &result.thread_ids).await;
        if !to_hydrate.is_empty() {
            crate::gmail_api::batch_metadata_hydrate(
                pool.inner(),
                &account.id,
                &account.access_token,
                to_hydrate,
            )
            .await;
        }
    }
    Ok(has_more)
}

async fn filter_no_metadata(pool: &sqlx::SqlitePool, thread_ids: &[String]) -> Vec<String> {
    if thread_ids.is_empty() {
        return vec![];
    }
    let placeholders: Vec<&str> = thread_ids.iter().map(|_| "?").collect();
    let sql = format!(
        "SELECT id FROM threads WHERE id IN ({}) AND (metadata_synced IS NULL OR metadata_synced = 0)",
        placeholders.join(",")
    );
    #[derive(sqlx::FromRow)]
    struct IdRow { id: String }
    let mut query = sqlx::query_as::<_, IdRow>(&sql);
    for tid in thread_ids {
        query = query.bind(tid);
    }
    query.fetch_all(pool).await.unwrap_or_default().into_iter().map(|r| r.id).collect()
}

#[tauri::command]
pub async fn fetch_category_threads(
    app_handle: tauri::AppHandle,
    category: String,
    account_id: Option<String>,
) -> Result<bool, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = match account_id {
        Some(id) => super::accounts::get_account_by_id(pool.inner(), &id).await?,
        None => get_active_account(pool.inner()).await?,
    };
    let token_store = app_handle.state::<crate::page_token_store::PageTokenStore>();
    let store_key = format!("{}:category:{}", account.id, category.to_lowercase());
    let current_token = token_store.get(&store_key);

    let (label_id, query) = match category.to_lowercase().as_str() {
        "primary" => ("INBOX".to_string(), Some("category:primary".to_string())),
        "social" => ("INBOX".to_string(), Some("category:social".to_string())),
        "promotions" => ("INBOX".to_string(), Some("category:promotions".to_string())),
        "important" => ("IMPORTANT".to_string(), None),
        _ => return Err(format!("Unknown category: {}", category)),
    };

    println!("[OnDemand] Fetching threads for category {} (has_token: {}, query: {:?})", category, current_token.is_some(), query);
    let result = crate::gmail_api::fetch_and_store_threads(
        pool.inner(),
        &account.id,
        &account.access_token,
        Some(&[label_id.as_str()]),
        ON_DEMAND_BATCH_SIZE,
        current_token.as_deref(),
        query.as_deref(),
    )
    .await?;

    let has_more = result.next_page_token.is_some();
    match result.next_page_token {
        Some(token) => token_store.set(&store_key, token),
        None => token_store.remove(&store_key),
    }

    if !result.thread_ids.is_empty() {
        let to_hydrate = filter_no_metadata(pool.inner(), &result.thread_ids).await;
        if !to_hydrate.is_empty() {
            crate::gmail_api::batch_metadata_hydrate(
                pool.inner(),
                &account.id,
                &account.access_token,
                to_hydrate,
            )
            .await;
        }
    }
    Ok(has_more)
}

pub(crate) async fn fetch_threads_by_ids(
    pool: &sqlx::SqlitePool,
    ids: &[String],
    account_id: &str,
) -> Result<Vec<LocalThread>, String> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
    let sql = format!(
        "SELECT t.id, t.snippet, t.history_id, t.unread,
                t.sender as sender,
                t.subject as subject,
                t.latest_date as msg_date,
                EXISTS (SELECT 1 FROM thread_labels tl WHERE tl.thread_id = t.id AND tl.label_id = 'STARRED') as starred,
                (SELECT tls.label_id FROM thread_labels tls
                 WHERE tls.thread_id = t.id
                 AND tls.label_id IN ('YELLOW_STAR','ORANGE_STAR','RED_STAR','PURPLE_STAR','BLUE_STAR','GREEN_STAR','GREEN_CIRCLE','RED_CIRCLE','ORANGE_CIRCLE','YELLOW_CIRCLE','BLUE_CIRCLE','PURPLE_CIRCLE')
                 LIMIT 1) as star_type,
                EXISTS (SELECT 1 FROM messages m6 WHERE m6.thread_id = t.id AND m6.has_attachments = 1) as has_attachments,
                EXISTS (SELECT 1 FROM thread_labels tl2 WHERE tl2.thread_id = t.id AND tl2.label_id = 'IMPORTANT') as important,
                t.account_id
         FROM threads t
         WHERE t.id IN ({}) AND t.account_id = ?
         ORDER BY COALESCE(t.latest_date, 0) DESC",
        placeholders.join(",")
    );

    #[derive(sqlx::FromRow)]
    struct TR {
        id: String,
        snippet: Option<String>,
        history_id: Option<String>,
        unread: Option<i32>,
        sender: Option<String>,
        subject: Option<String>,
        msg_date: Option<i64>,
        starred: Option<i32>,
        star_type: Option<String>,
        has_attachments: Option<i32>,
        important: Option<i32>,
        account_id: String,
    }

    let mut q = sqlx::query_as::<_, TR>(&sql);
    for tid in ids {
        q = q.bind(tid);
    }
    q = q.bind(account_id);

    let rows = q.fetch_all(pool).await.unwrap_or_default();
    Ok(rows
        .into_iter()
        .map(|r| LocalThread {
            id: r.id,
            snippet: r.snippet.unwrap_or_default(),
            history_id: r.history_id.unwrap_or_default(),
            unread: r.unread.unwrap_or(0),
            sender: clean_sender_name(r.sender),
            subject: r.subject.unwrap_or_else(|| "No Subject".to_string()),
            internal_date: r.msg_date.unwrap_or(0),
            starred: r.starred.unwrap_or(0) == 1,
            star_type: r.star_type,
            has_attachments: r.has_attachments.unwrap_or(0) == 1,
            important: r.important.unwrap_or(0) == 1,
            account_id: r.account_id,
        })
        .collect())
}

#[tauri::command]
pub async fn archive_thread(app_handle: tauri::AppHandle, thread_id: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::gmail_api::modify_thread(
        pool.inner(),
        &account.id,
        &account.access_token,
        &thread_id,
        vec![],
        vec!["INBOX".to_string()],
    )
    .await
}

#[tauri::command]
pub async fn move_thread_to_trash(
    app_handle: tauri::AppHandle,
    thread_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::gmail_api::trash_thread(pool.inner(), &account.id, &account.access_token, &thread_id)
        .await
}

#[tauri::command]
pub async fn untrash_thread(
    app_handle: tauri::AppHandle,
    thread_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::gmail_api::untrash_thread(&account.access_token, &thread_id).await?;
    sqlx::query(
        "INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0) ON CONFLICT(id) DO NOTHING"
    )
    .bind(&thread_id)
    .bind(&account.id)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM thread_labels WHERE thread_id = ?")
        .bind(&thread_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
    crate::gmail_api::fetch_messages_for_thread(
        pool.inner(),
        &account.id,
        &account.access_token,
        &thread_id,
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn mark_thread_read_status(
    app_handle: tauri::AppHandle,
    thread_id: String,
    is_read: bool,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let (add, remove) = if is_read {
        (vec![], vec!["UNREAD".to_string()])
    } else {
        (vec!["UNREAD".to_string()], vec![])
    };
    crate::gmail_api::modify_thread(
        pool.inner(),
        &account.id,
        &account.access_token,
        &thread_id,
        add,
        remove,
    )
    .await
}

const SUPERSTAR_ORDER: &[&str] = &[
    "YELLOW_STAR", "ORANGE_STAR", "RED_STAR", "PURPLE_STAR", "BLUE_STAR", "GREEN_STAR",
    "GREEN_CIRCLE", "RED_CIRCLE", "ORANGE_CIRCLE", "YELLOW_CIRCLE", "BLUE_CIRCLE", "PURPLE_CIRCLE",
];

#[tauri::command]
pub async fn set_thread_star(
    app_handle: tauri::AppHandle,
    thread_id: String,
    star_label_id: Option<String>,
    account_id: Option<String>,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = match account_id {
        Some(id) => super::accounts::get_account_by_id(pool.inner(), &id).await?,
        None => get_active_account(pool.inner()).await?,
    };

    // Only remove superstars that actually exist in the user's account
    let existing_stars = sqlx::query_scalar::<_, String>(
        "SELECT id FROM labels WHERE account_id = ? AND id IN ('YELLOW_STAR','ORANGE_STAR','RED_STAR','PURPLE_STAR','BLUE_STAR','GREEN_STAR','GREEN_CIRCLE','RED_CIRCLE','ORANGE_CIRCLE','YELLOW_CIRCLE','BLUE_CIRCLE','PURPLE_CIRCLE')"
    )
    .bind(&account.id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let add = match star_label_id {
        Some(ref label) => vec!["STARRED".to_string(), label.clone()],
        None => vec![],
    };

    let mut remove: Vec<String> = existing_stars
        .into_iter()
        .filter(|s| !add.contains(s))
        .collect();
    if !add.contains(&"STARRED".to_string()) {
        remove.push("STARRED".to_string());
    }

    crate::gmail_api::modify_thread(
        pool.inner(),
        &account.id,
        &account.access_token,
        &thread_id,
        add,
        remove,
    )
    .await
}

#[tauri::command]
pub async fn get_available_superstars(
    app_handle: tauri::AppHandle,
    account_id: Option<String>,
) -> Result<Vec<String>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = match account_id {
        Some(id) => super::accounts::get_account_by_id(pool.inner(), &id).await?,
        None => get_active_account(pool.inner()).await?,
    };

    let rows = sqlx::query_scalar::<_, String>(
        "SELECT id FROM labels WHERE account_id = ? AND id IN ('YELLOW_STAR','ORANGE_STAR','RED_STAR','PURPLE_STAR','BLUE_STAR','GREEN_STAR','GREEN_CIRCLE','RED_CIRCLE','ORANGE_CIRCLE','YELLOW_CIRCLE','BLUE_CIRCLE','PURPLE_CIRCLE')"
    )
    .bind(&account.id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(SUPERSTAR_ORDER.iter()
        .filter(|s| rows.contains(&s.to_string()))
        .map(|s| s.to_string())
        .collect())
}

#[tauri::command]
pub async fn toggle_thread_important(
    app_handle: tauri::AppHandle,
    thread_id: String,
    important: bool,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let (add, remove) = if important {
        (vec!["IMPORTANT".to_string()], vec![])
    } else {
        (vec![], vec!["IMPORTANT".to_string()])
    };
    crate::gmail_api::modify_thread(
        pool.inner(),
        &account.id,
        &account.access_token,
        &thread_id,
        add,
        remove,
    )
    .await?;

    toggle_important_local(pool.inner(), &thread_id, important).await?;

    Ok(())
}

/// Toggle the STARRED label on a thread locally (insert or delete from thread_labels).
#[allow(dead_code)] // used in tests
pub(crate) async fn toggle_star_local(pool: &sqlx::SqlitePool, thread_id: &str, starred: bool) -> Result<(), String> {
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

/// Toggle the IMPORTANT label on a thread locally (insert or delete from thread_labels).
#[allow(dead_code)] // used in tests
pub(crate) async fn toggle_important_local(pool: &sqlx::SqlitePool, thread_id: &str, important: bool) -> Result<(), String> {
    if important {
        sqlx::query("INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, 'IMPORTANT')")
            .bind(thread_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        sqlx::query("DELETE FROM thread_labels WHERE thread_id = ? AND label_id = 'IMPORTANT'")
            .bind(thread_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Mark a thread as read or unread locally (update threads.unread column).
#[allow(dead_code)] // used in tests
pub(crate) async fn mark_read_status_local(pool: &sqlx::SqlitePool, thread_id: &str, unread: bool) -> Result<(), String> {
    let val = if unread { 1 } else { 0 };
    sqlx::query("UPDATE threads SET unread = ? WHERE id = ?")
        .bind(val)
        .bind(thread_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_helpers::*;

    #[test]
    fn test_clean_sender_name() {
        assert_eq!(
            clean_sender_name(Some("John Doe <john@example.com>".to_string())),
            "John Doe"
        );
        assert_eq!(
            clean_sender_name(Some("<only-email@example.com>".to_string())),
            "only-email@example.com"
        );
        assert_eq!(
            clean_sender_name(Some("\"John Doe\" <john@example.com>".to_string())),
            "John Doe"
        );
        assert_eq!(clean_sender_name(None), "Unknown Sender");
    }

    #[test]
    fn test_clean_sender_name_empty_string() {
        assert_eq!(clean_sender_name(Some("".to_string())), "");
    }

    #[test]
    fn test_clean_sender_name_whitespace_only() {
        assert_eq!(clean_sender_name(Some("   ".to_string())), "   ");
    }

    #[test]
    fn test_clean_sender_name_multiple_brackets() {
        assert_eq!(
            clean_sender_name(Some("Name <email> <extra>".to_string())),
            "Name"
        );
    }

    #[test]
    fn test_clean_sender_name_no_brackets() {
        assert_eq!(
            clean_sender_name(Some("just-a-name".to_string())),
            "just-a-name"
        );
    }

    #[tokio::test]
    async fn test_get_threads_inner_empty() {
        let pool = setup_test_db().await;
        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert!(threads.is_empty());
    }

    #[tokio::test]
    async fn test_get_threads_inner_with_messages() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <alice@test.com>", "bob@test.com", "Hello", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "Bob <bob@test.com>", "alice@test.com", "World", 2000).await;

        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].id, "t2");
        assert_eq!(threads[0].sender, "Bob");
        assert_eq!(threads[0].subject, "World");
        assert_eq!(threads[1].id, "t1");
        assert_eq!(threads[1].sender, "Alice");
    }

    #[tokio::test]
    async fn test_get_threads_inner_label_filtering() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "sender@test.com", "", "Sub1", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "sender@test.com", "", "Sub2", 2000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'INBOX')")
            .execute(&pool).await.unwrap();

        let inbox_threads = get_threads_inner(&pool, "acc1", Some("INBOX"), None, 0, 50).await.unwrap();
        assert_eq!(inbox_threads.len(), 1);
        assert_eq!(inbox_threads[0].id, "t1");

        let all_threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert_eq!(all_threads.len(), 2);
    }

    #[tokio::test]
    async fn test_get_threads_inner_starred_flag() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'STARRED')")
            .execute(&pool).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert_eq!(threads.len(), 1);
        assert!(threads[0].starred);
    }

    #[tokio::test]
    async fn test_get_threads_inner_pagination() {
        let pool = setup_test_db().await;
        for i in 0..5 {
            let tid = format!("t{}", i);
            let mid = format!("m{}", i);
            insert_thread(&pool, &tid, "acc1").await;
            insert_message(&pool, &mid, &tid, "acc1", "s@t.com", "", "Sub", (i * 1000) as i64).await;
        }

        let page1 = get_threads_inner(&pool, "acc1", None, None, 0, 2).await.unwrap();
        assert_eq!(page1.len(), 2);
        let page2 = get_threads_inner(&pool, "acc1", None, None, 2, 2).await.unwrap();
        assert_eq!(page2.len(), 2);
        assert_ne!(page1[0].id, page2[0].id);
    }

    #[tokio::test]
    async fn test_get_threads_inner_filters_unhydrated() {
        let pool = setup_test_db().await;
        // Thread with no messages (unhydrated stub) should not appear
        insert_thread(&pool, "t1", "acc1").await;
        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert!(threads.is_empty(), "unhydrated threads should be filtered out");

        // Once a message is added (hydrated), it should appear
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "", 1000).await;
        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert_eq!(threads.len(), 1);
    }

    #[tokio::test]
    async fn test_get_threads_inner_clean_sender_name_integration() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "\"John Doe\" <john@example.com>", "", "Test Subject", 1000).await;

        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].sender, "John Doe");
    }

    #[tokio::test]
    async fn test_fetch_threads_by_ids_empty_list() {
        let pool = setup_test_db().await;
        let result = fetch_threads_by_ids(&pool, &[], "acc1").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_threads_by_ids_returns_matching() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_thread(&pool, "t3", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <alice@test.com>", "", "Hello", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "Bob <bob@test.com>", "", "World", 2000).await;
        insert_message(&pool, "m3", "t3", "acc1", "Carol <carol@test.com>", "", "Test", 3000).await;

        let ids = vec!["t1".to_string(), "t3".to_string()];
        let result = fetch_threads_by_ids(&pool, &ids, "acc1").await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "t3");
        assert_eq!(result[1].id, "t1");
    }

    #[tokio::test]
    async fn test_fetch_threads_by_ids_account_isolation() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc2").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;
        insert_message(&pool, "m2", "t2", "acc2", "s@t.com", "", "Sub", 2000).await;

        let ids = vec!["t1".to_string(), "t2".to_string()];
        let result = fetch_threads_by_ids(&pool, &ids, "acc1").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "t1");
    }

    #[tokio::test]
    async fn test_fetch_threads_by_ids_with_starred() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'STARRED')")
            .execute(&pool).await.unwrap();

        let ids = vec!["t1".to_string()];
        let result = fetch_threads_by_ids(&pool, &ids, "acc1").await.unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].starred);
    }

    #[tokio::test]
    async fn test_fetch_threads_by_ids_nonexistent() {
        let pool = setup_test_db().await;
        let ids = vec!["nonexistent".to_string()];
        let result = fetch_threads_by_ids(&pool, &ids, "acc1").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_threads_by_ids_with_messages_and_sender() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <a@test.com>", "", "Subject 1", 2000).await;
        insert_message(&pool, "m2", "t2", "acc1", "Bob <b@test.com>", "", "Subject 2", 1000).await;

        let ids = vec!["t1".to_string(), "t2".to_string()];
        let threads = fetch_threads_by_ids(&pool, &ids, "acc1").await.unwrap();
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].id, "t1");
        assert_eq!(threads[1].id, "t2");
    }

    #[tokio::test]
    async fn test_toggle_star_local_add_star() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;

        toggle_star_local(&pool, "t1", true).await.unwrap();

        #[derive(sqlx::FromRow)]
        struct LabelRow { label_id: String }
        let labels: Vec<LabelRow> = sqlx::query_as("SELECT label_id FROM thread_labels WHERE thread_id = 't1'")
            .fetch_all(&pool).await.unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].label_id, "STARRED");
    }

    #[tokio::test]
    async fn test_toggle_star_local_remove_star() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'STARRED')")
            .execute(&pool).await.unwrap();

        toggle_star_local(&pool, "t1", false).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM thread_labels WHERE thread_id = 't1' AND label_id = 'STARRED'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn test_toggle_star_local_idempotent_add() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;

        toggle_star_local(&pool, "t1", true).await.unwrap();
        toggle_star_local(&pool, "t1", true).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM thread_labels WHERE thread_id = 't1' AND label_id = 'STARRED'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn test_toggle_star_local_idempotent_remove() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;

        toggle_star_local(&pool, "t1", false).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM thread_labels WHERE thread_id = 't1' AND label_id = 'STARRED'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn test_toggle_star_local_verified_via_threads() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;

        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert!(!threads[0].starred);

        toggle_star_local(&pool, "t1", true).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert!(threads[0].starred);

        toggle_star_local(&pool, "t1", false).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert!(!threads[0].starred);
    }

    #[tokio::test]
    async fn test_mark_read_status_local_mark_unread() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;

        mark_read_status_local(&pool, "t1", true).await.unwrap();

        #[derive(sqlx::FromRow)]
        struct UnreadRow { unread: Option<i32> }
        let row: UnreadRow = sqlx::query_as("SELECT unread FROM threads WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(row.unread, Some(1));
    }

    #[tokio::test]
    async fn test_mark_read_status_local_mark_read() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        mark_read_status_local(&pool, "t1", true).await.unwrap();

        mark_read_status_local(&pool, "t1", false).await.unwrap();

        #[derive(sqlx::FromRow)]
        struct UnreadRow { unread: Option<i32> }
        let row: UnreadRow = sqlx::query_as("SELECT unread FROM threads WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(row.unread, Some(0));
    }

    #[tokio::test]
    async fn test_mark_read_status_local_verified_via_threads() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;

        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert_eq!(threads[0].unread, 0);

        mark_read_status_local(&pool, "t1", true).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert_eq!(threads[0].unread, 1);

        mark_read_status_local(&pool, "t1", false).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert_eq!(threads[0].unread, 0);
    }

    #[tokio::test]
    async fn test_mark_read_status_local_nonexistent_thread() {
        let pool = setup_test_db().await;
        let result = mark_read_status_local(&pool, "nonexistent", true).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_threads_inner_has_attachments_flag() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, ?, ?, ?, '', ?, '', '', 1)")
            .bind("m1").bind("t1").bind("acc1").bind("sender@test.com").bind("").bind("Subject").bind(1000i64)
            .execute(&pool).await.unwrap();
        sqlx::query("UPDATE threads SET sender = 'sender@test.com', subject = 'Subject', latest_date = 1000, metadata_synced = 1 WHERE id = 't1'")
            .execute(&pool).await.unwrap();
        insert_message(&pool, "m2", "t2", "acc1", "sender@test.com", "", "Subject2", 2000).await;

        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert_eq!(threads.len(), 2);
        let t1 = threads.iter().find(|t| t.id == "t1").unwrap();
        let t2 = threads.iter().find(|t| t.id == "t2").unwrap();
        assert!(t1.has_attachments);
        assert!(!t2.has_attachments);
    }

    #[tokio::test]
    async fn test_fetch_threads_by_ids_has_attachments() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, ?, ?, ?, '', ?, '', '', 1)")
            .bind("m1").bind("t1").bind("acc1").bind("sender@test.com").bind("").bind("Subject").bind(1000i64)
            .execute(&pool).await.unwrap();
        sqlx::query("UPDATE threads SET sender = 'sender@test.com', subject = 'Subject', latest_date = 1000, metadata_synced = 1 WHERE id = 't1'")
            .execute(&pool).await.unwrap();
        insert_message(&pool, "m2", "t2", "acc1", "sender@test.com", "", "Subject2", 2000).await;

        let ids = vec!["t1".to_string(), "t2".to_string()];
        let threads = fetch_threads_by_ids(&pool, &ids, "acc1").await.unwrap();
        assert_eq!(threads.len(), 2);
        let t1 = threads.iter().find(|t| t.id == "t1").unwrap();
        let t2 = threads.iter().find(|t| t.id == "t2").unwrap();
        assert!(t1.has_attachments);
        assert!(!t2.has_attachments);
    }

    #[tokio::test]
    async fn test_get_threads_inner_has_attachments_multiple_messages() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "sender@test.com", "", "Subject", 1000).await;
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, ?, ?, ?, '', ?, '', '', 1)")
            .bind("m2").bind("t1").bind("acc1").bind("sender@test.com").bind("").bind("Subject").bind(2000i64)
            .execute(&pool).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert_eq!(threads.len(), 1);
        assert!(threads[0].has_attachments);
    }

    #[tokio::test]
    async fn test_get_threads_inner_category_filtering() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_thread(&pool, "t3", "acc1").await;
        insert_thread(&pool, "t4", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub1", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "s@t.com", "", "Sub2", 2000).await;
        insert_message(&pool, "m3", "t3", "acc1", "s@t.com", "", "Sub3", 3000).await;
        insert_message(&pool, "m4", "t4", "acc1", "s@t.com", "", "Sub4", 4000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'INBOX')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t2', 'INBOX')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t2', 'CATEGORY_SOCIAL')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t3', 'INBOX')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t3', 'CATEGORY_PROMOTIONS')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t4', 'IMPORTANT')")
            .execute(&pool).await.unwrap();

        let primary = get_threads_inner(&pool, "acc1", Some("INBOX"), Some(ThreadCategory::Primary), 0, 50).await.unwrap();
        assert_eq!(primary.len(), 1);
        assert_eq!(primary[0].id, "t1");

        let social = get_threads_inner(&pool, "acc1", Some("INBOX"), Some(ThreadCategory::Social), 0, 50).await.unwrap();
        assert_eq!(social.len(), 1);
        assert_eq!(social[0].id, "t2");

        let promotions = get_threads_inner(&pool, "acc1", Some("INBOX"), Some(ThreadCategory::Promotions), 0, 50).await.unwrap();
        assert_eq!(promotions.len(), 1);
        assert_eq!(promotions[0].id, "t3");

        let important = get_threads_inner(&pool, "acc1", None, Some(ThreadCategory::Important), 0, 50).await.unwrap();
        assert_eq!(important.len(), 1);
        assert_eq!(important[0].id, "t4");
    }

    #[tokio::test]
    async fn test_get_threads_inner_important_flag() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "s@t.com", "", "Sub", 2000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'INBOX')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'IMPORTANT')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t2', 'INBOX')")
            .execute(&pool).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", Some("INBOX"), None, 0, 50).await.unwrap();
        assert_eq!(threads.len(), 2);
        let important_thread = threads.iter().find(|t| t.id == "t1").unwrap();
        let normal_thread = threads.iter().find(|t| t.id == "t2").unwrap();
        assert!(important_thread.important);
        assert!(!normal_thread.important);
    }

    #[tokio::test]
    async fn test_get_thread_count_inner_empty() {
        let pool = setup_test_db().await;
        let store = crate::page_token_store::PageTokenStore::new();
        let result = get_thread_count_inner(&pool, "acc1", None, None, &store).await.unwrap();
        assert_eq!(result.count, 0);
        assert!(!result.has_more_remote);
    }

    #[tokio::test]
    async fn test_get_thread_count_inner_with_threads() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub1", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "s@t.com", "", "Sub2", 2000).await;

        let store = crate::page_token_store::PageTokenStore::new();
        let result = get_thread_count_inner(&pool, "acc1", None, None, &store).await.unwrap();
        assert_eq!(result.count, 2);
    }

    #[tokio::test]
    async fn test_get_thread_count_inner_category_filtering() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub1", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "s@t.com", "", "Sub2", 2000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'INBOX')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t2', 'INBOX')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t2', 'CATEGORY_SOCIAL')").execute(&pool).await.unwrap();

        let store = crate::page_token_store::PageTokenStore::new();
        let primary = get_thread_count_inner(&pool, "acc1", Some("INBOX"), Some(ThreadCategory::Primary), &store).await.unwrap();
        assert_eq!(primary.count, 1);

        let social = get_thread_count_inner(&pool, "acc1", Some("INBOX"), Some(ThreadCategory::Social), &store).await.unwrap();
        assert_eq!(social.count, 1);
    }

    #[tokio::test]
    async fn test_get_thread_count_inner_has_more_remote() {
        let pool = setup_test_db().await;
        let store = crate::page_token_store::PageTokenStore::new();
        store.set("acc1:INBOX", "some-token".to_string());

        let result = get_thread_count_inner(&pool, "acc1", Some("INBOX"), None, &store).await.unwrap();
        assert!(result.has_more_remote);
    }

    #[tokio::test]
    async fn test_filter_no_metadata() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;

        let ids = vec!["t1".to_string(), "t2".to_string()];
        let no_meta = filter_no_metadata(&pool, &ids).await;
        assert_eq!(no_meta, vec!["t2".to_string()]);
    }

    #[tokio::test]
    async fn test_get_threads_inner_excludes_no_metadata() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        let threads = get_threads_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert!(threads.is_empty());
    }
}
