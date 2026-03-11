#[derive(serde::Serialize)]
pub struct SettingEntry {
    pub key: String,
    pub value: String,
}

pub(crate) async fn get_settings_inner(pool: &sqlx::SqlitePool) -> Result<Vec<SettingEntry>, String> {
    #[derive(sqlx::FromRow)]
    struct Row {
        key: String,
        value: String,
    }

    let rows: Vec<Row> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| SettingEntry {
            key: r.key,
            value: r.value,
        })
        .collect())
}

#[tauri::command]
pub async fn get_settings(app_handle: tauri::AppHandle) -> Result<Vec<SettingEntry>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    get_settings_inner(pool.inner()).await
}

pub(crate) async fn get_setting_inner(pool: &sqlx::SqlitePool, key: &str) -> Result<String, String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(row.map(|r| r.0).unwrap_or_default())
}

#[tauri::command]
pub async fn get_setting(app_handle: tauri::AppHandle, key: String) -> Result<String, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    get_setting_inner(pool.inner(), &key).await
}

pub(crate) async fn update_setting_inner(pool: &sqlx::SqlitePool, key: &str, value: &str) -> Result<(), String> {
    sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn update_setting(
    app_handle: tauri::AppHandle,
    key: String,
    value: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    update_setting_inner(pool.inner(), &key, &value).await
}

use tauri::Manager;

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_helpers::setup_test_db;

    #[tokio::test]
    async fn test_get_setting_inner_default_values() {
        let pool = setup_test_db().await;
        let theme = get_setting_inner(&pool, "theme").await.unwrap();
        assert_eq!(theme, "system");
        let density = get_setting_inner(&pool, "density").await.unwrap();
        assert_eq!(density, "default");
    }

    #[tokio::test]
    async fn test_get_setting_inner_nonexistent() {
        let pool = setup_test_db().await;
        let val = get_setting_inner(&pool, "nonexistent_key").await.unwrap();
        assert_eq!(val, "");
    }

    #[tokio::test]
    async fn test_update_setting_inner() {
        let pool = setup_test_db().await;
        let original = get_setting_inner(&pool, "theme").await.unwrap();
        assert_eq!(original, "system");

        update_setting_inner(&pool, "theme", "dark").await.unwrap();
        let updated = get_setting_inner(&pool, "theme").await.unwrap();
        assert_eq!(updated, "dark");
    }

    #[tokio::test]
    async fn test_update_setting_inner_creates_new_key() {
        let pool = setup_test_db().await;
        update_setting_inner(&pool, "custom_key", "custom_value").await.unwrap();
        let val = get_setting_inner(&pool, "custom_key").await.unwrap();
        assert_eq!(val, "custom_value");
    }

    #[tokio::test]
    async fn test_get_settings_inner_returns_defaults() {
        let pool = setup_test_db().await;
        let settings = get_settings_inner(&pool).await.unwrap();
        assert!(settings.len() >= 13, "Expected at least 13 settings, got {}", settings.len());

        let theme = settings.iter().find(|s| s.key == "theme");
        assert!(theme.is_some());
        assert_eq!(theme.unwrap().value, "system");

        let density = settings.iter().find(|s| s.key == "density");
        assert!(density.is_some());
        assert_eq!(density.unwrap().value, "default");
    }

    #[tokio::test]
    async fn test_get_settings_inner_reflects_updates() {
        let pool = setup_test_db().await;
        update_setting_inner(&pool, "theme", "dark").await.unwrap();

        let settings = get_settings_inner(&pool).await.unwrap();
        let theme = settings.iter().find(|s| s.key == "theme").unwrap();
        assert_eq!(theme.value, "dark");
    }

    #[tokio::test]
    async fn test_get_settings_inner_includes_custom_keys() {
        let pool = setup_test_db().await;
        update_setting_inner(&pool, "my_custom", "val123").await.unwrap();

        let settings = get_settings_inner(&pool).await.unwrap();
        let custom = settings.iter().find(|s| s.key == "my_custom");
        assert!(custom.is_some());
        assert_eq!(custom.unwrap().value, "val123");
    }
}
