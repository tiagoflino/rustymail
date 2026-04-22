use tauri::Manager;

#[derive(serde::Serialize, serde::Deserialize, sqlx::FromRow, Debug)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub subject: String,
    pub body_html: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[tauri::command]
pub async fn create_template(
    app_handle: tauri::AppHandle,
    name: String,
    subject: String,
    body_html: String,
) -> Result<Template, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let now = chrono::Utc::now().timestamp();
    let id = format!("tmpl_{}_{}", now, std::process::id());

    sqlx::query(
        "INSERT INTO templates (id, name, subject, body_html, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(&id).bind(&name).bind(&subject).bind(&body_html).bind(now).bind(now)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    tracing::info!("Template created: '{}' ({})", name, id);
    Ok(Template { id, name, subject, body_html, created_at: now, updated_at: now })
}

#[tauri::command]
pub async fn update_template(
    app_handle: tauri::AppHandle,
    id: String,
    name: String,
    subject: String,
    body_html: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let now = chrono::Utc::now().timestamp();

    let result = sqlx::query(
        "UPDATE templates SET name = ?, subject = ?, body_html = ?, updated_at = ? WHERE id = ?"
    )
    .bind(&name).bind(&subject).bind(&body_html).bind(now).bind(&id)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    if result.rows_affected() == 0 {
        return Err("Template not found".into());
    }
    tracing::info!("Template updated: '{}' ({})", name, id);
    Ok(())
}

#[tauri::command]
pub async fn delete_template(
    app_handle: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    sqlx::query("DELETE FROM templates WHERE id = ?")
        .bind(&id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!("Template deleted: {}", id);
    Ok(())
}

#[tauri::command]
pub async fn get_templates(
    app_handle: tauri::AppHandle,
) -> Result<Vec<Template>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let templates: Vec<Template> = sqlx::query_as(
        "SELECT id, name, subject, body_html, created_at, updated_at FROM templates ORDER BY name ASC"
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;
    Ok(templates)
}

#[tauri::command]
pub async fn get_template(
    app_handle: tauri::AppHandle,
    id: String,
) -> Result<Template, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let template: Template = sqlx::query_as(
        "SELECT id, name, subject, body_html, created_at, updated_at FROM templates WHERE id = ?"
    )
    .bind(&id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;
    Ok(template)
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::setup_test_db;

    #[tokio::test]
    async fn test_create_template() {
        let pool = setup_test_db().await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO templates (id, name, subject, body_html, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("t1").bind("Test").bind("Subject").bind("<p>Body</p>").bind(now).bind(now)
        .execute(&pool).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM templates")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn test_update_template() {
        let pool = setup_test_db().await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO templates (id, name, subject, body_html, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("t1").bind("Old Name").bind("Old Subject").bind("<p>Old</p>").bind(now).bind(now)
        .execute(&pool).await.unwrap();

        sqlx::query("UPDATE templates SET name = ?, updated_at = ? WHERE id = ?")
            .bind("New Name").bind(now + 1).bind("t1")
            .execute(&pool).await.unwrap();

        let name: (String,) = sqlx::query_as("SELECT name FROM templates WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(name.0, "New Name");
    }

    #[tokio::test]
    async fn test_delete_template() {
        let pool = setup_test_db().await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO templates (id, name, subject, body_html, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("t1").bind("Test").bind("").bind("<p>Body</p>").bind(now).bind(now)
        .execute(&pool).await.unwrap();

        sqlx::query("DELETE FROM templates WHERE id = 't1'").execute(&pool).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM templates")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn test_get_templates_ordered() {
        let pool = setup_test_db().await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO templates (id, name, subject, body_html, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("t2").bind("Zebra").bind("").bind("").bind(now).bind(now).execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO templates (id, name, subject, body_html, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("t1").bind("Alpha").bind("").bind("").bind(now).bind(now).execute(&pool).await.unwrap();

        let rows: Vec<(String,)> = sqlx::query_as("SELECT name FROM templates ORDER BY name ASC")
            .fetch_all(&pool).await.unwrap();
        assert_eq!(rows[0].0, "Alpha");
        assert_eq!(rows[1].0, "Zebra");
    }

    #[tokio::test]
    async fn test_duplicate_names_allowed() {
        let pool = setup_test_db().await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO templates (id, name, subject, body_html, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("t1").bind("Same Name").bind("").bind("").bind(now).bind(now).execute(&pool).await.unwrap();

        let result = sqlx::query(
            "INSERT INTO templates (id, name, subject, body_html, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
        ).bind("t2").bind("Same Name").bind("").bind("").bind(now).bind(now).execute(&pool).await;

        assert!(result.is_ok());
    }
}
