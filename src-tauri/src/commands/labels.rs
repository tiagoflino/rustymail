use super::accounts::get_active_account;
use tauri::Manager;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct LocalLabel {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub unread_count: i32,
}

pub(crate) async fn get_labels_inner(pool: &sqlx::SqlitePool, account_id: &str) -> Result<Vec<LocalLabel>, String> {
    #[derive(sqlx::FromRow)]
    struct LabelRow {
        id: String,
        name: Option<String>,
        r#type: Option<String>,
        unread_count: Option<i32>,
    }

    let rows: Vec<LabelRow> = sqlx::query_as(
        "SELECT id, name, type, unread_count FROM labels
         WHERE account_id = ?
         AND UPPER(id) NOT IN ('YELLOW_STAR', 'CHAT', 'VOICEMAIL')
         AND UPPER(name) NOT IN ('YELLOW_STAR', 'YELLOW STAR', 'CHAT', 'VOICEMAIL')
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
}
