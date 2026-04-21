use crate::calendar_api::{CalendarEvent, NewCalendarEvent};
use super::accounts::get_active_account;
use tauri::Manager;

#[tauri::command]
pub async fn get_events(
    app_handle: tauri::AppHandle,
    time_min: String,
    time_max: String,
) -> Result<Vec<CalendarEvent>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
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
    crate::calendar_api::delete_event(&account.access_token, &event_id).await
        .map_err(|e| { tracing::error!("Calendar API error: {}", e); e })
}
