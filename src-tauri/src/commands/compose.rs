use super::accounts::get_active_account;
use tauri::Manager;

#[derive(serde::Serialize)]
pub struct ContactSuggestion {
    pub name: String,
    pub email: String,
    pub raw: String,
}

#[tauri::command]
pub async fn send_message(
    app_handle: tauri::AppHandle,
    to: String,
    subject: String,
    body: String,
    thread_id: Option<String>,
    in_reply_to: Option<String>,
    references: Option<String>,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    #[derive(sqlx::FromRow)]
    struct EmailRow {
        email: String,
    }
    let row = sqlx::query_as::<_, EmailRow>("SELECT email FROM accounts WHERE id = ?")
        .bind(&account.id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    crate::gmail_api::send_message(
        &account.id,
        &row.email,
        &account.access_token,
        &to,
        &subject,
        &body,
        thread_id.as_deref(),
        in_reply_to.as_deref(),
        references.as_deref(),
    )
    .await
}

pub(crate) async fn search_contacts_inner(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    query: &str,
) -> Result<Vec<ContactSuggestion>, String> {
    let pattern = format!("%{}%", query);

    #[derive(sqlx::FromRow)]
    struct RawContact {
        contact: String,
    }

    let rows: Vec<RawContact> = sqlx::query_as(
        "SELECT DISTINCT sender as contact FROM messages WHERE account_id = ? AND sender LIKE ?
         UNION
         SELECT DISTINCT recipients as contact FROM messages WHERE account_id = ? AND recipients LIKE ?
         LIMIT 20"
    )
    .bind(account_id).bind(&pattern)
    .bind(account_id).bind(&pattern)
    .fetch_all(pool).await.unwrap_or_default();

    let mut seen = std::collections::HashSet::new();
    let mut suggestions = Vec::new();

    for row in rows {
        let parts: Vec<&str> = row.contact.split(',').collect();
        for p in parts {
            let p = p.trim();
            if p.is_empty() || !p.to_lowercase().contains(&query.to_lowercase()) {
                continue;
            }
            if !seen.insert(p.to_string()) {
                continue;
            }

            let (name, email) = if let Some(bracket_start) = p.find('<') {
                let name = p[..bracket_start].trim().trim_matches('"').to_string();
                let email = p[bracket_start + 1..].trim_matches('>').trim().to_string();
                (name, email)
            } else {
                ("".to_string(), p.to_string())
            };

            suggestions.push(ContactSuggestion {
                name,
                email,
                raw: p.to_string(),
            });
        }
    }

    suggestions.sort_by(|a, b| a.email.len().cmp(&b.email.len()));
    suggestions.truncate(10);
    Ok(suggestions)
}

#[tauri::command]
pub async fn search_contacts(
    app_handle: tauri::AppHandle,
    query: String,
) -> Result<Vec<ContactSuggestion>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    search_contacts_inner(pool.inner(), &account.id, &query).await
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn save_draft(
    app_handle: tauri::AppHandle,
    to: String,
    subject: String,
    body: String,
    thread_id: Option<String>,
    in_reply_to: Option<String>,
    references: Option<String>,
    draft_id: Option<String>,
) -> Result<String, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    #[derive(sqlx::FromRow)]
    struct EmailRow {
        email: String,
    }
    let row = sqlx::query_as::<_, EmailRow>("SELECT email FROM accounts WHERE id = ?")
        .bind(&account.id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    let new_draft_id = crate::gmail_api::save_draft(
        &account.id,
        &row.email,
        &account.access_token,
        &to,
        &subject,
        &body,
        thread_id.as_deref(),
        in_reply_to.as_deref(),
        references.as_deref(),
        draft_id.as_deref(),
    )
    .await?;

    // Clean up stale local draft messages for this thread.
    // Gmail changes the message ID on each draft update, leaving orphaned
    // records in the local DB. Delete local draft-labeled messages for the thread
    // so re-sync picks up fresh data without duplicates.
    if let Some(ref tid) = thread_id {
        let _ = sqlx::query(
            "DELETE FROM messages WHERE thread_id = ? AND id IN (
                SELECT message_id FROM message_labels WHERE label_id = 'DRAFT'
            )",
        )
        .bind(tid)
        .execute(pool.inner())
        .await;
    }

    Ok(new_draft_id)
}

#[tauri::command]
pub async fn delete_draft(app_handle: tauri::AppHandle, draft_id: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    #[derive(sqlx::FromRow)]
    struct EmailRow {
        email: String,
    }
    let row = sqlx::query_as::<_, EmailRow>("SELECT email FROM accounts WHERE id = ?")
        .bind(&account.id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    crate::gmail_api::delete_draft(
        pool.inner(),
        &account.id,
        &row.email,
        &account.access_token,
        &draft_id,
    )
    .await
}

#[tauri::command]
pub async fn delete_draft_by_thread(
    app_handle: tauri::AppHandle,
    thread_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    crate::gmail_api::delete_draft_by_thread(
        pool.inner(),
        &account.id,
        &account.access_token,
        &thread_id,
    )
    .await
}

#[tauri::command]
pub async fn get_draft_id_by_message_id(
    app_handle: tauri::AppHandle,
    message_id: String,
) -> Result<String, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    crate::gmail_api::get_draft_id_by_message_id(&account.id, &account.access_token, &message_id)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_helpers::*;

    #[tokio::test]
    async fn test_search_contacts_inner_empty() {
        let pool = setup_test_db().await;
        let contacts = search_contacts_inner(&pool, "acc1", "alice").await.unwrap();
        assert!(contacts.is_empty());
    }

    #[tokio::test]
    async fn test_search_contacts_inner_finds_senders() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice Doe <alice@example.com>", "bob@example.com", "Hi", 1000).await;

        let contacts = search_contacts_inner(&pool, "acc1", "alice").await.unwrap();
        assert!(!contacts.is_empty());
        assert_eq!(contacts[0].email, "alice@example.com");
        assert_eq!(contacts[0].name, "Alice Doe");
    }

    #[tokio::test]
    async fn test_search_contacts_inner_finds_recipients() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "someone@test.com", "Bob Smith <bob@example.com>", "Hi", 1000).await;

        let contacts = search_contacts_inner(&pool, "acc1", "bob").await.unwrap();
        assert!(!contacts.is_empty());
        assert_eq!(contacts[0].email, "bob@example.com");
    }

    #[tokio::test]
    async fn test_search_contacts_inner_deduplication() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@example.com", "", "Hi", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "alice@example.com", "", "Hi again", 2000).await;

        let contacts = search_contacts_inner(&pool, "acc1", "alice").await.unwrap();
        assert_eq!(contacts.len(), 1);
    }

    #[tokio::test]
    async fn test_search_contacts_inner_sorted_by_email_length() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "alice_longname@verylongdomain.com", "", "Hi", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "alice@short.com", "", "Hi", 2000).await;

        let contacts = search_contacts_inner(&pool, "acc1", "alice").await.unwrap();
        assert_eq!(contacts.len(), 2);
        assert!(contacts[0].email.len() <= contacts[1].email.len());
    }
}
