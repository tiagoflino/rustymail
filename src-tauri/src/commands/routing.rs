use super::accounts::get_active_account;
use crate::sender_routing::{Routing, SenderRoutingInfo};
use tauri::Manager;

#[tauri::command]
pub async fn check_new_sender(
    app_handle: tauri::AppHandle,
    sender_email: String,
) -> Result<bool, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::sender_routing::is_new_sender(pool.inner(), &account.id, &sender_email).await
}

#[derive(serde::Serialize)]
pub struct NewSenderCheck {
    pub sender_email: String,
    pub is_new: bool,
}

/// Batched new-sender check — one IPC round-trip for many senders, replacing the
/// per-sender N+1 calls on the sync hot path.
#[tauri::command]
pub async fn check_new_senders(
    app_handle: tauri::AppHandle,
    sender_emails: Vec<String>,
) -> Result<Vec<NewSenderCheck>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    let mut out = Vec::with_capacity(sender_emails.len());
    for sender_email in sender_emails {
        let is_new =
            crate::sender_routing::is_new_sender(pool.inner(), &account.id, &sender_email).await?;
        out.push(NewSenderCheck { sender_email, is_new });
    }
    Ok(out)
}

#[tauri::command]
pub async fn set_sender_routing(
    app_handle: tauri::AppHandle,
    sender_email: String,
    sender_name: Option<String>,
    routing: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let now = chrono::Utc::now().timestamp();

    // Normalize the routing string (unknown -> inbox) and store the canonical
    // value, and normalize the email so reads/lookups match exactly.
    let routing = Routing::parse(&routing).as_str().to_string();
    let sender_email = crate::sender_routing::normalize_email(&sender_email);

    sqlx::query(
        "INSERT OR REPLACE INTO sender_routing (sender_email, account_id, sender_name, routing, created_at, updated_at)
         VALUES (?, ?, ?, ?, COALESCE((SELECT created_at FROM sender_routing WHERE sender_email = ? AND account_id = ?), ?), ?)"
    )
    .bind(&sender_email)
    .bind(&account.id)
    .bind(&sender_name)
    .bind(&routing)
    .bind(&sender_email)
    .bind(&account.id)
    .bind(now)
    .bind(now)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    tracing::info!("Sender routing set: {} -> {} ({})", sender_email, routing,
        sender_name.as_deref().unwrap_or("unknown"));
    Ok(())
}

#[tauri::command]
pub async fn get_sender_routing(
    app_handle: tauri::AppHandle,
    sender_email: String,
) -> Result<Option<SenderRoutingInfo>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    #[derive(sqlx::FromRow)]
    struct RoutingRow {
        sender_email: String,
        sender_name: Option<String>,
        routing: String,
        created_at: i64,
        updated_at: i64,
    }

    let sender_email = crate::sender_routing::normalize_email(&sender_email);
    let row: Option<RoutingRow> = sqlx::query_as(
        "SELECT sender_email, sender_name, routing, created_at, updated_at FROM sender_routing WHERE sender_email = ? AND account_id = ?"
    )
    .bind(&sender_email)
    .bind(&account.id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(row.map(|r| SenderRoutingInfo {
        sender_email: r.sender_email,
        sender_name: r.sender_name,
        routing: r.routing,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }))
}

#[tauri::command]
pub async fn get_all_sender_routings(
    app_handle: tauri::AppHandle,
) -> Result<Vec<SenderRoutingInfo>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    #[derive(sqlx::FromRow)]
    struct RoutingRow {
        sender_email: String,
        sender_name: Option<String>,
        routing: String,
        created_at: i64,
        updated_at: i64,
    }

    let rows: Vec<RoutingRow> = sqlx::query_as(
        "SELECT sender_email, sender_name, routing, created_at, updated_at FROM sender_routing WHERE account_id = ? ORDER BY updated_at DESC"
    )
    .bind(&account.id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows.into_iter().map(|r| SenderRoutingInfo {
        sender_email: r.sender_email,
        sender_name: r.sender_name,
        routing: r.routing,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }).collect())
}

/// Remove a routing decision so the sender returns to default (inbox) handling.
#[tauri::command]
pub async fn delete_sender_routing(
    app_handle: tauri::AppHandle,
    sender_email: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let sender_email = crate::sender_routing::normalize_email(&sender_email);

    sqlx::query("DELETE FROM sender_routing WHERE sender_email = ? AND account_id = ?")
        .bind(&sender_email)
        .bind(&account.id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::{setup_test_db, insert_account};

    #[tokio::test]
    async fn test_set_and_get_sender_routing() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO sender_routing (sender_email, account_id, routing, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind("sender@example.com")
        .bind("acc1")
        .bind("feed")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let routing: (String,) = sqlx::query_as(
            "SELECT routing FROM sender_routing WHERE sender_email = ? AND account_id = ?"
        )
        .bind("sender@example.com")
        .bind("acc1")
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(routing.0, "feed");
    }

    #[tokio::test]
    async fn test_set_sender_routing_updates_existing() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO sender_routing (sender_email, account_id, routing, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind("s@example.com")
        .bind("acc1")
        .bind("inbox")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT OR REPLACE INTO sender_routing (sender_email, account_id, routing, created_at, updated_at) VALUES (?, ?, ?, (SELECT created_at FROM sender_routing WHERE sender_email = ? AND account_id = ?), ?)"
        )
        .bind("s@example.com")
        .bind("acc1")
        .bind("blocked")
        .bind("s@example.com")
        .bind("acc1")
        .bind(now + 100)
        .execute(&pool)
        .await
        .unwrap();

        let routing: (String,) = sqlx::query_as(
            "SELECT routing FROM sender_routing WHERE sender_email = ? AND account_id = ?"
        )
        .bind("s@example.com")
        .bind("acc1")
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(routing.0, "blocked");
    }

    #[tokio::test]
    async fn test_get_sender_routing_nonexistent() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let row: Option<(String,)> = sqlx::query_as(
            "SELECT routing FROM sender_routing WHERE sender_email = ? AND account_id = ?"
        )
        .bind("nonexistent@example.com")
        .bind("acc1")
        .fetch_optional(&pool)
        .await
        .unwrap();

        assert!(row.is_none());
    }

    #[tokio::test]
    async fn test_get_all_sender_routings_ordered() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = chrono::Utc::now().timestamp();

        for (i, (email, routing)) in [
            ("a@test.com", "inbox"),
            ("b@test.com", "feed"),
            ("c@test.com", "blocked"),
        ].iter().enumerate() {
            sqlx::query(
                "INSERT INTO sender_routing (sender_email, account_id, routing, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
            )
            .bind(email)
            .bind("acc1")
            .bind(routing)
            .bind(now + i as i64)
            .bind(now + i as i64)
            .execute(&pool)
            .await
            .unwrap();
        }

        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT sender_email FROM sender_routing WHERE account_id = ? ORDER BY updated_at DESC"
        )
        .bind("acc1")
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(rows.len(), 3);
        // Most recently updated first
        assert_eq!(rows[0].0, "c@test.com");
    }

    #[tokio::test]
    async fn test_sender_routing_persists_sender_name() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO sender_routing (sender_email, account_id, sender_name, routing, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind("named@example.com")
        .bind("acc1")
        .bind("Named Sender")
        .bind("feed")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let row: (Option<String>, String) = sqlx::query_as(
            "SELECT sender_name, routing FROM sender_routing WHERE sender_email = ? AND account_id = ?"
        )
        .bind("named@example.com")
        .bind("acc1")
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(row.0.as_deref(), Some("Named Sender"));
        assert_eq!(row.1, "feed");
    }

    #[tokio::test]
    async fn test_sender_routing_unique_per_account() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test User", 1, 1000).await;

        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO sender_routing (sender_email, account_id, routing, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind("dup@test.com")
        .bind("acc1")
        .bind("inbox")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let dup = sqlx::query(
            "INSERT INTO sender_routing (sender_email, account_id, routing, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind("dup@test.com")
        .bind("acc1")
        .bind("feed")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await;

        assert!(dup.is_err(), "Duplicate sender_email+account_id should fail");
    }
}
