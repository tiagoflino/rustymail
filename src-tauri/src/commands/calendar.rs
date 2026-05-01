use crate::calendar_api::{CalendarEvent, NewCalendarEvent};
use super::accounts::get_active_account;
use tauri::Manager;

async fn get_caldav_url(pool: &sqlx::SqlitePool, account_id: &str) -> Result<String, String> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT caldav_url FROM imap_config WHERE account_id = ?",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .flatten()
    .ok_or_else(|| {
        "CalDAV not configured. Calendar requires CalDAV support from your provider.".to_string()
    })
}

#[tauri::command]
pub async fn get_events(
    app_handle: tauri::AppHandle,
    time_min: String,
    time_max: String,
) -> Result<Vec<CalendarEvent>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let provider_type = super::accounts::get_provider_type(pool.inner(), &account.id).await;
    if provider_type == "imap" {
        let caldav_url = get_caldav_url(pool.inner(), &account.id).await?;
        let config = crate::provider::imap::connection::ImapConfig::from_db(pool.inner(), &account.id).await?;
        let password = crate::credentials::get_imap_password(&account.id)?;
        return crate::caldav_api::caldav_get_events(&caldav_url, &config.username, &password, &time_min, &time_max).await;
    }
    if provider_type == "outlook" {
        return crate::outlook_api::outlook_get_events(&account.access_token, &time_min, &time_max).await;
    }
    crate::calendar_api::get_events(&account.access_token, &time_min, &time_max).await
}

#[tauri::command]
pub async fn create_event(
    app_handle: tauri::AppHandle,
    event: NewCalendarEvent,
) -> Result<CalendarEvent, String> {
    tracing::info!("Calendar event created");
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let provider_type = super::accounts::get_provider_type(pool.inner(), &account.id).await;
    if provider_type == "imap" {
        let caldav_url = get_caldav_url(pool.inner(), &account.id).await?;
        let config = crate::provider::imap::connection::ImapConfig::from_db(pool.inner(), &account.id).await?;
        let password = crate::credentials::get_imap_password(&account.id)?;
        return crate::caldav_api::caldav_create_event(&caldav_url, &config.username, &password, &event).await
            .map_err(|e| { tracing::error!("CalDAV API error: {}", e); e });
    }
    if provider_type == "outlook" {
        return crate::outlook_api::outlook_create_event(&account.access_token, &event).await
            .map_err(|e| { tracing::error!("Outlook Calendar API error: {}", e); e });
    }
    crate::calendar_api::create_event(&account.access_token, &event).await
        .map_err(|e| { tracing::error!("Calendar API error: {}", e); e })
}

#[tauri::command]
pub async fn update_event(
    app_handle: tauri::AppHandle,
    event_id: String,
    event: NewCalendarEvent,
) -> Result<CalendarEvent, String> {
    tracing::info!("Calendar event updated: {}", event_id);
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let provider_type = super::accounts::get_provider_type(pool.inner(), &account.id).await;
    if provider_type == "imap" {
        let caldav_url = get_caldav_url(pool.inner(), &account.id).await?;
        let config = crate::provider::imap::connection::ImapConfig::from_db(pool.inner(), &account.id).await?;
        let password = crate::credentials::get_imap_password(&account.id)?;
        return crate::caldav_api::caldav_update_event(&caldav_url, &config.username, &password, &event_id, &event).await
            .map_err(|e| { tracing::error!("CalDAV API error: {}", e); e });
    }
    if provider_type == "outlook" {
        return crate::outlook_api::outlook_update_event(&account.access_token, &event_id, &event).await
            .map_err(|e| { tracing::error!("Outlook Calendar API error: {}", e); e });
    }
    crate::calendar_api::update_event(&account.access_token, &event_id, &event).await
        .map_err(|e| { tracing::error!("Calendar API error: {}", e); e })
}

#[tauri::command]
pub async fn delete_event(
    app_handle: tauri::AppHandle,
    event_id: String,
) -> Result<(), String> {
    tracing::info!("Calendar event deleted: {}", event_id);
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let provider_type = super::accounts::get_provider_type(pool.inner(), &account.id).await;
    if provider_type == "imap" {
        let caldav_url = get_caldav_url(pool.inner(), &account.id).await?;
        let config = crate::provider::imap::connection::ImapConfig::from_db(pool.inner(), &account.id).await?;
        let password = crate::credentials::get_imap_password(&account.id)?;
        return crate::caldav_api::caldav_delete_event(&caldav_url, &config.username, &password, &event_id).await
            .map_err(|e| { tracing::error!("CalDAV API error: {}", e); e });
    }
    if provider_type == "outlook" {
        return crate::outlook_api::outlook_delete_event(&account.access_token, &event_id).await
            .map_err(|e| { tracing::error!("Outlook Calendar API error: {}", e); e });
    }
    crate::calendar_api::delete_event(&account.access_token, &event_id).await
        .map_err(|e| { tracing::error!("Calendar API error: {}", e); e })
}
