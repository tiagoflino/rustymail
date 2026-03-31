use super::accounts::get_active_account;
use tauri::Manager;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct LocalLabel {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub unread_count: i32,
    pub threads_total: i32,
    pub threads_unread: i32,
    #[serde(rename = "bgColor")]
    pub bg_color: Option<String>,
    #[serde(rename = "textColor")]
    pub text_color: Option<String>,
}

pub(crate) async fn get_labels_inner(pool: &sqlx::SqlitePool, account_id: &str) -> Result<Vec<LocalLabel>, String> {
    #[derive(sqlx::FromRow)]
    struct LabelRow {
        id: String,
        name: Option<String>,
        r#type: Option<String>,
        unread_count: Option<i32>,
        threads_total: Option<i32>,
        threads_unread: Option<i32>,
        bg_color: Option<String>,
        text_color: Option<String>,
    }

    let rows: Vec<LabelRow> = sqlx::query_as(
        "SELECT id, name, type, unread_count, COALESCE(threads_total, 0) as threads_total, COALESCE(threads_unread, 0) as threads_unread, bg_color, text_color FROM labels
         WHERE account_id = ?
         AND (
           type = 'user'
           OR (type = 'system' AND UPPER(id) IN ('INBOX', 'SENT', 'DRAFT', 'TRASH', 'SPAM', 'STARRED', 'IMPORTANT'))
         )
         ORDER BY CASE WHEN type = 'system' THEN 0 ELSE 1 END, name ASC",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| LocalLabel {
            id: r.id,
            name: r.name.unwrap_or_default(),
            r#type: r.r#type.unwrap_or_default(),
            unread_count: r.unread_count.unwrap_or(0),
            threads_total: r.threads_total.unwrap_or(0),
            threads_unread: r.threads_unread.unwrap_or(0),
            bg_color: r.bg_color,
            text_color: r.text_color,
        })
        .collect())
}

#[tauri::command]
pub async fn get_labels(app_handle: tauri::AppHandle) -> Result<Vec<LocalLabel>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    get_labels_inner(pool.inner(), &account.id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_helpers::setup_test_db;

    #[tokio::test]
    async fn test_get_labels_inner_empty() {
        let pool = setup_test_db().await;
        let labels = get_labels_inner(&pool, "acc1").await.unwrap();
        assert!(labels.is_empty());
    }

    #[tokio::test]
    async fn test_get_labels_inner_filters_hidden_labels() {
        let pool = setup_test_db().await;
        for (id, name, ltype) in &[
            ("INBOX", "INBOX", "system"),
            ("SENT", "SENT", "system"),
            ("CHAT", "CHAT", "system"),
            ("VOICEMAIL", "VOICEMAIL", "system"),
            ("YELLOW_STAR", "YELLOW_STAR", "system"),
            ("Label_1", "Work", "user"),
        ] {
            sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES (?, 'acc1', ?, ?, 0)")
                .bind(id).bind(name).bind(ltype)
                .execute(&pool).await.unwrap();
        }

        let labels = get_labels_inner(&pool, "acc1").await.unwrap();
        let label_ids: Vec<&str> = labels.iter().map(|l| l.id.as_str()).collect();
        assert!(label_ids.contains(&"INBOX"));
        assert!(label_ids.contains(&"SENT"));
        assert!(label_ids.contains(&"Label_1"));
        assert!(!label_ids.contains(&"CHAT"));
        assert!(!label_ids.contains(&"VOICEMAIL"));
        assert!(!label_ids.contains(&"YELLOW_STAR"));
    }

    #[tokio::test]
    async fn test_get_labels_inner_ordering() {
        let pool = setup_test_db().await;
        sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES ('Label_Z', 'acc1', 'Zebra', 'user', 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES ('INBOX', 'acc1', 'INBOX', 'system', 5)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES ('Label_A', 'acc1', 'Alpha', 'user', 0)")
            .execute(&pool).await.unwrap();

        let labels = get_labels_inner(&pool, "acc1").await.unwrap();
        assert_eq!(labels[0].id, "INBOX");
        assert_eq!(labels[1].id, "Label_A");
        assert_eq!(labels[2].id, "Label_Z");
    }

    #[tokio::test]
    async fn test_get_labels_inner_account_isolation() {
        let pool = setup_test_db().await;
        sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES ('INBOX', 'acc1', 'INBOX', 'system', 3)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES ('INBOX2', 'acc2', 'INBOX', 'system', 1)")
            .execute(&pool).await.unwrap();

        let labels = get_labels_inner(&pool, "acc1").await.unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].unread_count, 3);
    }

    #[tokio::test]
    async fn test_get_labels_inner_includes_thread_counts() {
        let pool = setup_test_db().await;
        sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count, threads_total, threads_unread, bg_color, text_color) VALUES ('INBOX', 'acc1', 'INBOX', 'system', 5, 1000, 50, '#ff0000', '#ffffff')")
            .execute(&pool).await.unwrap();

        let labels = get_labels_inner(&pool, "acc1").await.unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].threads_total, 1000);
        assert_eq!(labels[0].threads_unread, 50);
        assert_eq!(labels[0].bg_color.as_deref(), Some("#ff0000"));
        assert_eq!(labels[0].text_color.as_deref(), Some("#ffffff"));
    }

    #[tokio::test]
    async fn test_get_labels_inner_filters_superstar_labels() {
        let pool = setup_test_db().await;
        // Insert the whitelisted system labels
        for (id, name) in &[
            ("INBOX", "INBOX"),
            ("SENT", "SENT"),
            ("DRAFT", "DRAFT"),
            ("STARRED", "STARRED"),
            ("IMPORTANT", "IMPORTANT"),
        ] {
            sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES (?, 'acc1', ?, 'system', 0)")
                .bind(id).bind(name)
                .execute(&pool).await.unwrap();
        }
        // Insert superstar/decorative system labels that should be filtered
        for (id, name) in &[
            ("YELLOW_STAR", "Starred"),
            ("GREEN_STAR", "Green star"),
            ("RED_STAR", "Red star"),
            ("ORANGE_STAR", "Orange star"),
            ("BLUE_STAR", "Blue star"),
            ("PURPLE_STAR", "Purple star"),
            ("GREEN_CIRCLE", "Green circle"),
            ("RED_CIRCLE", "Red circle"),
            ("ORANGE_CIRCLE", "Orange circle"),
            ("YELLOW_CIRCLE", "Yellow circle"),
            ("BLUE_CIRCLE", "Blue circle"),
            ("PURPLE_CIRCLE", "Purple circle"),
        ] {
            sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES (?, 'acc1', ?, 'system', 0)")
                .bind(id).bind(name)
                .execute(&pool).await.unwrap();
        }
        // Insert other internal system labels that should be filtered
        for (id, name) in &[
            ("CHAT", "CHAT"),
            ("VOICEMAIL", "VOICEMAIL"),
            ("OPENED", "OPENED"),
            ("UNREAD", "UNREAD"),
            ("CATEGORY_PERSONAL", "CATEGORY_PERSONAL"),
            ("CATEGORY_SOCIAL", "CATEGORY_SOCIAL"),
            ("CATEGORY_PROMOTIONS", "CATEGORY_PROMOTIONS"),
            ("CATEGORY_UPDATES", "CATEGORY_UPDATES"),
            ("CATEGORY_FORUMS", "CATEGORY_FORUMS"),
        ] {
            sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES (?, 'acc1', ?, 'system', 0)")
                .bind(id).bind(name)
                .execute(&pool).await.unwrap();
        }

        let labels = get_labels_inner(&pool, "acc1").await.unwrap();
        let label_ids: Vec<&str> = labels.iter().map(|l| l.id.as_str()).collect();

        // Whitelisted system labels should be present
        assert!(label_ids.contains(&"INBOX"), "INBOX should be present");
        assert!(label_ids.contains(&"SENT"), "SENT should be present");
        assert!(label_ids.contains(&"DRAFT"), "DRAFT should be present");
        assert!(label_ids.contains(&"STARRED"), "STARRED should be present");
        assert!(label_ids.contains(&"IMPORTANT"), "IMPORTANT should be present");

        // Only the 5 whitelisted system labels should be returned
        assert_eq!(labels.len(), 5, "Only whitelisted system labels should pass through, got: {:?}", label_ids);
    }

    #[tokio::test]
    async fn test_get_labels_inner_user_labels_not_affected_by_system_filter() {
        let pool = setup_test_db().await;
        // User labels that look like system labels should still pass through
        for (id, name) in &[
            ("Label_1", "Gold Star"),
            ("Label_2", "Red Circle Project"),
            ("Label_3", "Work/Projects/Active"),
            ("Label_4", "Newsletters"),
        ] {
            sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES (?, 'acc1', ?, 'user', 0)")
                .bind(id).bind(name)
                .execute(&pool).await.unwrap();
        }

        let labels = get_labels_inner(&pool, "acc1").await.unwrap();
        assert_eq!(labels.len(), 4);
        let names: Vec<&str> = labels.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"Gold Star"));
        assert!(names.contains(&"Red Circle Project"));
        assert!(names.contains(&"Work/Projects/Active"));
        assert!(names.contains(&"Newsletters"));
    }

    #[tokio::test]
    async fn test_get_labels_inner_system_whitelist_is_complete() {
        let pool = setup_test_db().await;
        // Insert all 7 whitelisted system labels
        for (id, name) in &[
            ("INBOX", "INBOX"),
            ("SENT", "SENT"),
            ("DRAFT", "DRAFT"),
            ("TRASH", "TRASH"),
            ("SPAM", "SPAM"),
            ("STARRED", "STARRED"),
            ("IMPORTANT", "IMPORTANT"),
        ] {
            sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES (?, 'acc1', ?, 'system', 0)")
                .bind(id).bind(name)
                .execute(&pool).await.unwrap();
        }

        let labels = get_labels_inner(&pool, "acc1").await.unwrap();
        let label_ids: Vec<&str> = labels.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(labels.len(), 7);
        for expected in &["INBOX", "SENT", "DRAFT", "TRASH", "SPAM", "STARRED", "IMPORTANT"] {
            assert!(label_ids.contains(expected), "{} should be present", expected);
        }
    }
}
