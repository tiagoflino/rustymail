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
    }

    let rows: Vec<TR> = if let Some(lid) = label_id {
        sqlx::query_as(
            "SELECT t.id, t.snippet, t.history_id, t.unread,
                    (SELECT m2.sender FROM messages m2 WHERE m2.thread_id = t.id ORDER BY m2.internal_date DESC LIMIT 1) as sender,
                    (SELECT m3.subject FROM messages m3 WHERE m3.thread_id = t.id ORDER BY m3.internal_date DESC LIMIT 1) as subject,
                    (SELECT MAX(m4.internal_date) FROM messages m4 WHERE m4.thread_id = t.id) as msg_date,
                    EXISTS (SELECT 1 FROM thread_labels tl WHERE tl.thread_id = t.id AND tl.label_id = 'STARRED') as starred
             FROM threads t
             INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = ?
             WHERE t.account_id = ?
             ORDER BY COALESCE((SELECT MAX(m5.internal_date) FROM messages m5 WHERE m5.thread_id = t.id), 0) DESC, t.rowid DESC
             LIMIT ? OFFSET ?"
        ).bind(lid).bind(account_id).bind(limit).bind(offset)
        .fetch_all(pool).await.map_err(|e| e.to_string())?
    } else {
        sqlx::query_as(
            "SELECT t.id, t.snippet, t.history_id, t.unread,
                    (SELECT m2.sender FROM messages m2 WHERE m2.thread_id = t.id ORDER BY m2.internal_date DESC LIMIT 1) as sender,
                    (SELECT m3.subject FROM messages m3 WHERE m3.thread_id = t.id ORDER BY m3.internal_date DESC LIMIT 1) as subject,
                    (SELECT MAX(m4.internal_date) FROM messages m4 WHERE m4.thread_id = t.id) as msg_date,
                    EXISTS (SELECT 1 FROM thread_labels tl WHERE tl.thread_id = t.id AND tl.label_id = 'STARRED') as starred
             FROM threads t
             WHERE t.account_id = ?
             ORDER BY COALESCE((SELECT MAX(m5.internal_date) FROM messages m5 WHERE m5.thread_id = t.id), 0) DESC, t.rowid DESC
             LIMIT ? OFFSET ?"
        ).bind(account_id).bind(limit).bind(offset)
        .fetch_all(pool).await.map_err(|e| e.to_string())?
    };

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
        })
        .collect())
}

#[tauri::command]
pub async fn get_threads(
    app_handle: tauri::AppHandle,
    label_id: Option<String>,
    offset: Option<i32>,
    limit: Option<i32>,
) -> Result<Vec<LocalThread>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let lim = limit.unwrap_or(50);
    let off = offset.unwrap_or(0);
    get_threads_inner(pool.inner(), &account.id, label_id.as_deref(), off, lim).await
}

#[tauri::command]
pub async fn fetch_label_threads(
    app_handle: tauri::AppHandle,
    label_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    println!("[OnDemand] Fetching threads for label: {}", label_id);
    crate::gmail_api::fetch_and_store_threads(
        pool.inner(),
        &account.id,
        &account.access_token,
        Some(&[label_id.as_str()]),
        50,
    )
    .await?;

    #[derive(sqlx::FromRow)]
    struct PrefetchVal { value: String }
    let prefetch = sqlx::query_as::<_, PrefetchVal>("SELECT value FROM settings WHERE key = 'prefetch_bodies'")
        .fetch_optional(pool.inner())
        .await
        .unwrap_or(None)
        .map(|r| r.value == "true")
        .unwrap_or(false);

    let unhydrated = crate::gmail_api::get_unhydrated_thread_ids(pool.inner(), &account.id).await;
    if !unhydrated.is_empty() {
        let limit = if prefetch { unhydrated.len() } else { 50 };
        let batch: Vec<String> = unhydrated.into_iter().take(limit).collect();
        crate::gmail_api::batch_hydrate_threads(
            pool.inner(),
            &account.id,
            &account.access_token,
            batch,
        )
        .await;
    }
    Ok(())
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
                (SELECT m2.sender FROM messages m2 WHERE m2.thread_id = t.id ORDER BY m2.internal_date DESC LIMIT 1) as sender,
                (SELECT m3.subject FROM messages m3 WHERE m3.thread_id = t.id ORDER BY m3.internal_date DESC LIMIT 1) as subject,
                (SELECT MAX(m4.internal_date) FROM messages m4 WHERE m4.thread_id = t.id) as msg_date,
                EXISTS (SELECT 1 FROM thread_labels tl WHERE tl.thread_id = t.id AND tl.label_id = 'STARRED') as starred
         FROM threads t
         WHERE t.id IN ({}) AND t.account_id = ?
         ORDER BY COALESCE((SELECT MAX(m5.internal_date) FROM messages m5 WHERE m5.thread_id = t.id), 0) DESC",
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

#[tauri::command]
pub async fn toggle_thread_star(
    app_handle: tauri::AppHandle,
    thread_id: String,
    starred: bool,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let (add, remove) = if starred {
        (vec!["STARRED".to_string()], vec![])
    } else {
        (vec![], vec!["STARRED".to_string()])
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
        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert!(threads.is_empty());
    }

    #[tokio::test]
    async fn test_get_threads_inner_with_messages() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <alice@test.com>", "bob@test.com", "Hello", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "Bob <bob@test.com>", "alice@test.com", "World", 2000).await;

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
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

        let inbox_threads = get_threads_inner(&pool, "acc1", Some("INBOX"), 0, 50).await.unwrap();
        assert_eq!(inbox_threads.len(), 1);
        assert_eq!(inbox_threads[0].id, "t1");

        let all_threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert_eq!(all_threads.len(), 2);
    }

    #[tokio::test]
    async fn test_get_threads_inner_starred_flag() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'STARRED')")
            .execute(&pool).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
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

        let page1 = get_threads_inner(&pool, "acc1", None, 0, 2).await.unwrap();
        assert_eq!(page1.len(), 2);
        let page2 = get_threads_inner(&pool, "acc1", None, 2, 2).await.unwrap();
        assert_eq!(page2.len(), 2);
        assert_ne!(page1[0].id, page2[0].id);
    }

    #[tokio::test]
    async fn test_get_threads_inner_no_subject_fallback() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert_eq!(threads[0].subject, "No Subject");
    }

    #[tokio::test]
    async fn test_get_threads_inner_clean_sender_name_integration() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "\"John Doe\" <john@example.com>", "", "Test Subject", 1000).await;

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
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

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert!(!threads[0].starred);

        toggle_star_local(&pool, "t1", true).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert!(threads[0].starred);

        toggle_star_local(&pool, "t1", false).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
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

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert_eq!(threads[0].unread, 0);

        mark_read_status_local(&pool, "t1", true).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert_eq!(threads[0].unread, 1);

        mark_read_status_local(&pool, "t1", false).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert_eq!(threads[0].unread, 0);
    }

    #[tokio::test]
    async fn test_mark_read_status_local_nonexistent_thread() {
        let pool = setup_test_db().await;
        let result = mark_read_status_local(&pool, "nonexistent", true).await;
        assert!(result.is_ok());
    }
}
