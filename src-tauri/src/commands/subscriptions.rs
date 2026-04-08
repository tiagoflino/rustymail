use sqlx::SqlitePool;
use tauri::Manager;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_helpers::setup_test_db;

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
}