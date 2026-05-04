mod commands;
pub mod caldav_api;
pub mod calendar_api;
mod credentials;
mod db;
pub mod email_utils;
mod gmail_api;
pub mod outlook_api;
mod page_token_store;
pub mod provider;
mod subscription_detector;
mod tray;

use tauri::{Emitter, Manager};
use tracing_subscriber::{fmt, EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use tracing_appender::rolling;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn confirm_quit(app_handle: tauri::AppHandle) {
    app_handle.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(debug_assertions)]
    dotenvy::dotenv().ok();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();

            let app_data_dir = handle.path().app_data_dir().expect("app data dir");
            let log_dir = app_data_dir.join("logs");
            std::fs::create_dir_all(&log_dir).ok();
            let file_appender = rolling::daily(&log_dir, "rustymail.log");
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
            Box::leak(Box::new(guard));

            tracing_subscriber::registry()
                .with(EnvFilter::new("rustymail_lib=info,rustymail_premium=info,warn"))
                .with(fmt::layer().with_writer(non_blocking).with_ansi(false).with_target(true))
                .with(fmt::layer().with_writer(std::io::stderr).with_ansi(true).with_target(true))
                .init();

            tracing::info!("Rustymail starting up");

            let pool = tauri::async_runtime::block_on(async { db::init_db(&handle).await })
                .expect("Failed to initialize database");
            #[cfg(feature = "premium")]
            let pool_clone = pool.clone();
            handle.manage(pool);
            handle.manage(page_token_store::PageTokenStore::new());
            handle.manage(provider::imap::idle::IdleManager::new());
            #[cfg(feature = "premium")]
            {
                let engine = rustymail_premium::llm::engine::LlmEngine::new(
                    app_data_dir.clone()
                );
                engine.start_auto_unload_timer(pool_clone);
                handle.manage(engine);
            }
            tray::setup_tray(app)?;
            tracing::info!("Database initialized, tray setup complete");
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let handle = window.app_handle().clone();
                let pool = handle.state::<sqlx::SqlitePool>();
                let close_behavior = tauri::async_runtime::block_on(async {
                    sqlx::query_scalar::<_, String>(
                        "SELECT value FROM settings WHERE key = 'close_behavior'"
                    )
                    .fetch_optional(pool.inner())
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "minimize".to_string())
                });

                api.prevent_close();
                if close_behavior == "quit" {
                    let _ = window.emit("quit-requested", ());
                } else {
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::accounts::authenticate_gmail,
            commands::accounts::authenticate_microsoft,
            commands::accounts::check_auth_status,
            commands::accounts::get_accounts,
            commands::accounts::switch_account,
            commands::accounts::remove_account,
            commands::accounts::get_credential_config,
            commands::accounts::get_provider_capabilities,
            commands::accounts::test_imap_connection,
            commands::accounts::test_smtp_connection,
            commands::accounts::add_imap_account,
            commands::accounts::autodiscover_imap,
            commands::settings::get_settings,
            commands::settings::get_setting,
            commands::settings::update_setting,
            commands::sync::sync_gmail_data,
            commands::sync::ensure_threads_hydrated,
            commands::labels::get_labels,
            commands::threads::get_threads,
            commands::threads::get_thread_count,
            commands::threads::get_unified_threads,
            commands::threads::get_unified_thread_count,
            commands::threads::fetch_label_threads,
            commands::threads::fetch_category_threads,
            commands::threads::archive_thread,
            commands::threads::move_thread_to_trash,
            commands::threads::untrash_thread,
            commands::threads::mark_thread_read_status,
            commands::threads::set_thread_star,
            commands::threads::get_available_superstars,
            commands::threads::toggle_thread_important,
            commands::snooze::snooze_thread,
            commands::snooze::unsnooze_thread,
            commands::snooze::get_snoozed_threads,
            commands::snooze::check_snoozed_threads,
            commands::threads::batch_archive_threads,
            commands::threads::batch_trash_threads,
            commands::threads::batch_mark_read_status,
            commands::threads::batch_star_threads,
            commands::snooze::batch_snooze_threads,
            commands::schedule::schedule_send,
            commands::schedule::cancel_scheduled_send,
            commands::schedule::get_scheduled_sends,
            commands::schedule::check_scheduled_sends,
            commands::schedule::get_scheduled_count,
            commands::threads::batch_move_to_label,
            commands::messages::get_messages,
            commands::messages::sync_thread_messages,
            commands::messages::get_attachments,
            commands::messages::download_attachment,
            commands::messages::open_attachment,
            commands::messages::get_message_previews,
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
            commands::calendar::get_events,
            commands::calendar::create_event,
            commands::calendar::update_event,
            commands::calendar::delete_event,
            confirm_quit,
            commands::misc::open_external_url,
            commands::misc::get_upcoming_events,
            commands::misc::get_file_size,
            commands::misc::get_log_path,
            commands::misc::get_recent_logs,
            commands::misc::open_log_directory,
            commands::templates::create_template,
            commands::templates::update_template,
            commands::templates::delete_template,
            commands::templates::get_templates,
            commands::templates::get_template,
            commands::subscriptions::get_subscriptions,
            commands::subscriptions::correct_subscription,
            commands::subscriptions::delete_subscription,
            commands::subscriptions::unsubscribe,
            commands::subscriptions::scan_subscriptions,
            commands::subscriptions::mark_unsubscribed,
            commands::contacts::create_contact,
            commands::contacts::get_contact,
            commands::contacts::get_contacts,
            commands::contacts::update_contact,
            commands::contacts::delete_contact,
            commands::contacts::search_contacts_v2,
            commands::contacts::merge_contacts,
            commands::contacts::get_contact_groups,
            commands::contacts::create_contact_group,
            commands::contacts::update_contact_group,
            commands::contacts::delete_contact_group,
            commands::contacts::set_contact_groups,
            commands::contacts::import_contacts,
            commands::contacts::export_contacts,
            tray::update_tray_unread,
            #[cfg(feature = "premium")]
            rustymail_premium::commands::llm::get_ai_status,
            #[cfg(feature = "premium")]
            rustymail_premium::commands::llm::get_ai_hardware,
            #[cfg(feature = "premium")]
            rustymail_premium::commands::llm::ensure_ai_ready,
            #[cfg(feature = "premium")]
            rustymail_premium::commands::llm::summarize_thread,
            #[cfg(feature = "premium")]
            rustymail_premium::commands::llm::clear_ai_cache,
            #[cfg(feature = "premium")]
            rustymail_premium::commands::llm::ai_compose,
            #[cfg(feature = "premium")]
            rustymail_premium::commands::llm::ai_smart_replies,
            #[cfg(feature = "premium")]
            rustymail_premium::commands::llm::ai_extract_actions,
            #[cfg(feature = "premium")]
            rustymail_premium::commands::llm::ai_analyze_sentiment,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        match &event {
            tauri::RunEvent::ExitRequested { api, .. } => {
                api.prevent_exit();
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = window.emit("quit-requested", ());
                }
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            _ => {}
        }
    });
}
