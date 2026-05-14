use tauri::Manager;

#[tauri::command]
pub async fn get_feed_threads(
    app_handle: tauri::AppHandle,
    offset: Option<i32>,
    limit: Option<i32>,
) -> Result<Vec<super::threads::LocalThread>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = super::accounts::get_active_account(pool.inner()).await?;
    let off = offset.unwrap_or(0);
    let lim = limit.unwrap_or(50);
    get_feed_threads_inner(pool.inner(), &account.id, off, lim).await
}

pub(crate) async fn get_feed_threads_inner(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    offset: i32,
    limit: i32,
) -> Result<Vec<super::threads::LocalThread>, String> {
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

    let sql = r#"
        SELECT DISTINCT t.id, t.snippet, t.history_id, t.unread,
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
        INNER JOIN subscriptions s ON t.account_id = s.account_id AND INSTR(t.sender, s.sender_email) > 0
        WHERE s.status = 'active'
          AND t.account_id = ?
          AND t.metadata_synced = 1
          AND EXISTS (SELECT 1 FROM thread_labels tl3 WHERE tl3.thread_id = t.id AND tl3.label_id = 'INBOX')
        ORDER BY COALESCE(t.latest_date, 0) DESC, t.rowid DESC
        LIMIT ? OFFSET ?
    "#;

    let rows: Vec<TR> = sqlx::query_as::<_, TR>(sql)
        .bind(account_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| super::threads::LocalThread {
            id: r.id,
            snippet: r.snippet.unwrap_or_default(),
            history_id: r.history_id.unwrap_or_default(),
            unread: r.unread.unwrap_or(0),
            sender: super::threads::clean_sender_name(r.sender),
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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_helpers::*;

    #[tokio::test]
    async fn test_get_feed_threads_returns_subscription_threads() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;

        sqlx::query(
            "INSERT INTO subscriptions (account_id, sender_email, sender_name, detection_method, first_seen, last_seen, status) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind("acc1").bind("newsletter@example.com").bind("Newsletter Co").bind("header").bind(1000).bind(2000).bind("active")
        .execute(&pool).await.unwrap();

        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "newsletter@example.com", "", "Newsletter Subject", 3000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'INBOX')")
            .execute(&pool).await.unwrap();

        let result = get_feed_threads_inner(&pool, "acc1", 0, 50).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "t1");
        assert_eq!(result[0].subject, "Newsletter Subject");
    }

    #[tokio::test]
    async fn test_get_feed_threads_excludes_non_subscription_senders() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;

        sqlx::query(
            "INSERT INTO subscriptions (account_id, sender_email, sender_name, detection_method, first_seen, last_seen, status) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind("acc1").bind("newsletter@example.com").bind("Newsletter Co").bind("header").bind(1000).bind(2000).bind("active")
        .execute(&pool).await.unwrap();

        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "newsletter@example.com", "", "Sub", 1000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'INBOX')")
            .execute(&pool).await.unwrap();

        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m2", "t2", "acc1", "random@other.com", "", "Not a sub", 2000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t2', 'INBOX')")
            .execute(&pool).await.unwrap();

        let result = get_feed_threads_inner(&pool, "acc1", 0, 50).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "t1");
    }

    #[tokio::test]
    async fn test_get_feed_threads_respects_pagination() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;

        for (email, name, _date) in [
            ("newsletter@example.com", "Newsletter", 1000),
            ("updates@example.com", "Updates", 2000),
        ] {
            sqlx::query(
                "INSERT INTO subscriptions (account_id, sender_email, sender_name, detection_method, first_seen, last_seen, status) VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind("acc1").bind(email).bind(name).bind("header").bind(1000).bind(2000).bind("active")
            .execute(&pool).await.unwrap();
        }

        for (tid, mid, email, subject, date) in [
            ("t1", "m1", "newsletter@example.com", "Sub1", 1000),
            ("t2", "m2", "updates@example.com", "Sub2", 2000),
        ] {
            insert_thread(&pool, tid, "acc1").await;
            insert_message(&pool, mid, tid, "acc1", email, "", subject, date).await;
            sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES (?, 'INBOX')")
                .bind(tid)
                .execute(&pool).await.unwrap();
        }

        let page1 = get_feed_threads_inner(&pool, "acc1", 0, 1).await.unwrap();
        assert_eq!(page1.len(), 1);
        assert_eq!(page1[0].id, "t2");

        let page2 = get_feed_threads_inner(&pool, "acc1", 1, 1).await.unwrap();
        assert_eq!(page2.len(), 1);
        assert_eq!(page2[0].id, "t1");
    }

    #[tokio::test]
    async fn test_get_feed_threads_excludes_inactive_subscriptions() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;

        sqlx::query(
            "INSERT INTO subscriptions (account_id, sender_email, sender_name, detection_method, first_seen, last_seen, status) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind("acc1").bind("newsletter@example.com").bind("Newsletter").bind("header").bind(1000).bind(2000).bind("active")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO subscriptions (account_id, sender_email, sender_name, detection_method, first_seen, last_seen, status) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind("acc1").bind("unsubscribed@example.com").bind("Gone").bind("header").bind(1000).bind(2000).bind("unsubscribed")
        .execute(&pool).await.unwrap();

        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "newsletter@example.com", "", "Active", 1000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'INBOX')")
            .execute(&pool).await.unwrap();

        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m2", "t2", "acc1", "unsubscribed@example.com", "", "Inactive", 2000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t2', 'INBOX')")
            .execute(&pool).await.unwrap();

        let result = get_feed_threads_inner(&pool, "acc1", 0, 50).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "t1");
    }
}
