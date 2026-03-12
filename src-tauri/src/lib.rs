mod commands;
pub mod calendar_api;
mod credentials;
mod db;
mod gmail_api;
mod tray;

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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let pool = tauri::async_runtime::block_on(async { db::init_db(&handle).await })
                .expect("Failed to initialize database");
            handle.manage(pool);
            tray::setup_tray(app)?;
            println!("Database initialized successfully!");
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Hide window instead of closing — app stays in tray
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::accounts::authenticate_gmail,
            commands::accounts::check_auth_status,
            commands::accounts::get_accounts,
            commands::accounts::switch_account,
            commands::accounts::remove_account,
            commands::settings::get_settings,
            commands::settings::get_setting,
            commands::settings::update_setting,
            commands::sync::sync_gmail_data,
            commands::sync::ensure_threads_hydrated,
            commands::labels::get_labels,
            commands::threads::get_threads,
            commands::threads::fetch_label_threads,
            commands::threads::archive_thread,
            commands::threads::move_thread_to_trash,
            commands::threads::untrash_thread,
            commands::threads::mark_thread_read_status,
            commands::threads::toggle_thread_star,
            commands::messages::get_messages,
            commands::messages::sync_thread_messages,
            commands::messages::get_attachments,
            commands::messages::download_attachment,
            commands::messages::open_attachment,
            commands::search::search_messages,
            commands::search::get_hydration_progress,
            commands::search::get_search_suggestions,
            commands::search::save_recent_search,
            commands::compose::send_message,
            commands::compose::save_draft,
            commands::compose::delete_draft,
            commands::compose::delete_draft_by_thread,
            commands::compose::upload_to_drive,
            commands::compose::get_draft_id_by_message_id,
            commands::compose::search_contacts,
            commands::misc::open_external_url,
            commands::misc::get_upcoming_events,
            commands::misc::get_file_size,
            tray::update_tray_unread,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
