mod auth;
pub mod calendar_api;
mod credentials;
mod db;
mod gmail_api;

use tauri::Manager;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(debug_assertions)]
    dotenvy::dotenv().ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let pool = tauri::async_runtime::block_on(async { db::init_db(&handle).await })
                .expect("Failed to initialize database");
            handle.manage(pool);
            println!("Database initialized successfully!");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            auth::authenticate_gmail,
            auth::check_auth_status,
            auth::get_accounts,
            auth::switch_account,
            auth::remove_account,
            auth::get_settings,
            auth::update_setting,
            auth::sync_gmail_data,
            auth::get_threads,
            auth::sync_thread_messages,
            auth::get_messages,
            auth::get_labels,
            auth::archive_thread,
            auth::move_thread_to_trash,
            auth::untrash_thread,
            auth::mark_thread_read_status,
            crate::auth::toggle_thread_star,
            auth::search_messages,
            auth::get_hydration_progress,
            auth::ensure_threads_hydrated,
            auth::get_search_suggestions,
            auth::save_recent_search,
            auth::fetch_label_threads,
            auth::get_setting,
            auth::send_message,
            auth::save_draft,
            auth::delete_draft,
            auth::delete_draft_by_thread,
            auth::get_draft_id_by_message_id,
            auth::get_upcoming_events,
            auth::search_contacts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}