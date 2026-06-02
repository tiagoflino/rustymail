use sqlx::SqlitePool;
use tauri::Manager;

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct ActionItem {
    pub id: i64,
    pub account_id: String,
    pub thread_id: String,
    pub message_id: Option<String>,
    pub description: String,
    pub assignee: Option<String>,
    pub deadline: Option<String>,
    pub confidence: f64,
    pub status: String,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub thread_subject: Option<String>,
    pub thread_sender: Option<String>,
}

#[tauri::command]
pub async fn get_action_items(
    app_handle: tauri::AppHandle,
    account_id: Option<String>,
    status: Option<String>,
    limit: Option<i32>,
) -> Result<Vec<ActionItem>, String> {
    let pool = app_handle.state::<SqlitePool>();

    let mut sql = String::from(
        "SELECT a.id, a.account_id, a.thread_id, a.message_id, a.description,
                a.assignee, a.deadline, a.confidence, a.status, a.created_at,
                a.completed_at, t.subject AS thread_subject, t.sender AS thread_sender
         FROM action_items a
         LEFT JOIN threads t ON a.thread_id = t.id
         WHERE 1=1"
    );

    let mut binds: Vec<String> = Vec::new();

    if let Some(ref acc) = account_id {
        sql.push_str(" AND a.account_id = ?");
        binds.push(acc.clone());
    }
    if let Some(ref st) = status {
        sql.push_str(" AND a.status = ?");
        binds.push(st.clone());
    }

    sql.push_str(" ORDER BY a.created_at DESC");

    if let Some(l) = limit {
        sql.push_str(&format!(" LIMIT {}", l));
    }

    let mut query = sqlx::query_as::<_, ActionItem>(&sql);
    for bind in &binds {
        query = query.bind(bind);
    }

    query.fetch_all(pool.inner()).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mark_action_complete(
    app_handle: tauri::AppHandle,
    action_item_id: i64,
) -> Result<(), String> {
    let pool = app_handle.state::<SqlitePool>();
    let now = chrono::Utc::now().timestamp();

    sqlx::query("UPDATE action_items SET status = 'completed', completed_at = ? WHERE id = ?")
        .bind(now)
        .bind(action_item_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn dismiss_action_item(
    app_handle: tauri::AppHandle,
    action_item_id: i64,
) -> Result<(), String> {
    let pool = app_handle.state::<SqlitePool>();

    sqlx::query("UPDATE action_items SET status = 'dismissed' WHERE id = ?")
        .bind(action_item_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn delete_action_items_by_thread(
    app_handle: tauri::AppHandle,
    thread_id: String,
) -> Result<i64, String> {
    let pool = app_handle.state::<SqlitePool>();

    let deleted = sqlx::query("DELETE FROM action_items WHERE thread_id = ?")
        .bind(&thread_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(deleted.rows_affected() as i64)
}

#[derive(serde::Serialize)]
pub struct ThreadActionCount {
    pub thread_id: String,
    pub count: i64,
}

#[tauri::command]
pub async fn get_action_counts(
    app_handle: tauri::AppHandle,
    thread_ids: Vec<String>,
) -> Result<Vec<ThreadActionCount>, String> {
    if thread_ids.is_empty() {
        return Ok(vec![]);
    }
    let pool = app_handle.state::<SqlitePool>();

    let placeholders: Vec<String> = thread_ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
    let sql = format!(
        "SELECT thread_id, COUNT(*) as count FROM action_items WHERE status = 'pending' AND thread_id IN ({}) GROUP BY thread_id",
        placeholders.join(",")
    );

    let mut query = sqlx::query_as::<_, (String, i64)>(&sql);
    for id in &thread_ids {
        query = query.bind(id);
    }

    let rows = query.fetch_all(pool.inner()).await.map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|(thread_id, count)| ThreadActionCount { thread_id, count }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_helpers::{insert_account, insert_message, insert_thread, setup_test_db};
    use crate::db;

    async fn setup_with_action_items_table() -> SqlitePool {
        let pool = setup_test_db().await;
        db::m022_create_action_items(&pool).await.unwrap();
        pool
    }

    async fn insert_action_item(
        pool: &SqlitePool,
        account_id: &str,
        thread_id: &str,
        message_id: Option<&str>,
        description: &str,
        assignee: Option<&str>,
        deadline: Option<&str>,
        confidence: f64,
        status: &str,
        created_at: i64,
    ) -> i64 {
        sqlx::query(
            "INSERT INTO action_items (account_id, thread_id, message_id, description, assignee, deadline, confidence, status, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(account_id)
        .bind(thread_id)
        .bind(message_id)
        .bind(description)
        .bind(assignee)
        .bind(deadline)
        .bind(confidence)
        .bind(status)
        .bind(created_at)
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid()
    }

    #[tokio::test]
    async fn test_get_action_items_empty() {
        let pool = setup_with_action_items_table().await;
        let items: Vec<ActionItem> = sqlx::query_as(
            "SELECT a.id, a.account_id, a.thread_id, a.message_id, a.description,
                    a.assignee, a.deadline, a.confidence, a.status, a.created_at,
                    a.completed_at, NULL AS thread_subject, NULL AS thread_sender
             FROM action_items a"
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn test_get_action_items_with_thread_join() {
        let pool = setup_with_action_items_table().await;
        insert_account(&pool, "acc1", "user@test.com", "User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "sender@test.com", "user@test.com", "Test Subject", 1000).await;

        let id = insert_action_item(&pool, "acc1", "t1", Some("m1"), "Review the proposal", Some("Alice"), Some("2026-06-01"), 0.95, "pending", 1000).await;

        let pool_ref = &pool;
        let items: Vec<ActionItem> = sqlx::query_as(
            "SELECT a.id, a.account_id, a.thread_id, a.message_id, a.description,
                    a.assignee, a.deadline, a.confidence, a.status, a.created_at,
                    a.completed_at, t.subject AS thread_subject, t.sender AS thread_sender
             FROM action_items a
             LEFT JOIN threads t ON a.thread_id = t.id
             WHERE a.id = ?"
        )
        .bind(id)
        .fetch_all(pool_ref)
        .await
        .unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].description, "Review the proposal");
        assert_eq!(items[0].assignee, Some("Alice".to_string()));
        assert_eq!(items[0].thread_subject, Some("Test Subject".to_string()));
        assert_eq!(items[0].thread_sender, Some("sender@test.com".to_string()));
        assert_eq!(items[0].status, "pending");
    }

    #[tokio::test]
    async fn test_mark_action_complete() {
        let pool = setup_with_action_items_table().await;
        insert_account(&pool, "acc1", "user@test.com", "User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "sender@test.com", "user@test.com", "Test", 1000).await;

        let id = insert_action_item(&pool, "acc1", "t1", Some("m1"), "Task", None, None, 0.8, "pending", 1000).await;

        mark_action_complete_inner(&pool, id).await.unwrap();

        let (status, completed_at): (String, Option<i64>) = sqlx::query_as(
            "SELECT status, completed_at FROM action_items WHERE id = ?"
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(status, "completed");
        assert!(completed_at.is_some());
    }

    #[tokio::test]
    async fn test_dismiss_action_item() {
        let pool = setup_with_action_items_table().await;
        insert_account(&pool, "acc1", "user@test.com", "User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;

        let id = insert_action_item(&pool, "acc1", "t1", None, "Task", None, None, 0.8, "pending", 1000).await;

        dismiss_action_item_inner(&pool, id).await.unwrap();

        let status: String = sqlx::query_scalar("SELECT status FROM action_items WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(status, "dismissed");
    }

    async fn mark_action_complete_inner(pool: &SqlitePool, action_item_id: i64) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query("UPDATE action_items SET status = 'completed', completed_at = ? WHERE id = ?")
            .bind(now)
            .bind(action_item_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn dismiss_action_item_inner(pool: &SqlitePool, action_item_id: i64) -> Result<(), String> {
        sqlx::query("UPDATE action_items SET status = 'dismissed' WHERE id = ?")
            .bind(action_item_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    #[tokio::test]
    async fn test_filter_by_status() {
        let pool = setup_with_action_items_table().await;
        insert_account(&pool, "acc1", "user@test.com", "User", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;

        insert_action_item(&pool, "acc1", "t1", None, "Pending task", None, None, 0.5, "pending", 1000).await;
        insert_action_item(&pool, "acc1", "t1", None, "Completed task", None, None, 0.5, "completed", 2000).await;

        let pending_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM action_items WHERE status = 'pending'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let completed_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM action_items WHERE status = 'completed'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(pending_count, 1);
        assert_eq!(completed_count, 1);
    }

    #[tokio::test]
    async fn test_mark_action_complete_nonexistent() {
        let pool = setup_with_action_items_table().await;
        let result = mark_action_complete_inner(&pool, 99999).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_action_item_serialization() {
        let item = ActionItem {
            id: 1,
            account_id: "acc1".to_string(),
            thread_id: "t1".to_string(),
            message_id: Some("m1".to_string()),
            description: "Review the proposal".to_string(),
            assignee: Some("Alice".to_string()),
            deadline: Some("2026-06-01".to_string()),
            confidence: 0.95,
            status: "pending".to_string(),
            created_at: 1000,
            completed_at: None,
            thread_subject: Some("Test".to_string()),
            thread_sender: Some("sender@test.com".to_string()),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("description"));
        assert!(json.contains("assignee"));
        assert!(json.contains("deadline"));
        assert!(json.contains("confidence"));
        assert!(json.contains("thread_subject"));
        assert!(json.contains("thread_sender"));
    }
}
