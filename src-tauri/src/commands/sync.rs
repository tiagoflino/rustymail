use super::accounts::get_active_account;
use tauri::Manager;

#[derive(serde::Serialize)]
pub struct SyncResult {
    pub new_message_ids: Vec<String>,
    pub new_thread_ids: Vec<String>,
}

#[tauri::command]
pub async fn sync_gmail_data(
    app_handle: tauri::AppHandle,
    label_id: Option<String>,
    account_id: Option<String>,
) -> Result<SyncResult, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = match account_id {
        Some(id) => super::accounts::get_account_by_id(pool.inner(), &id).await?,
        None => super::accounts::get_active_account(pool.inner()).await?,
    };

    tracing::info!("Sync started for account {}", account.id);

    let provider_type = super::accounts::get_provider_type(pool.inner(), &account.id).await;
    if provider_type == "imap" {
        let config = crate::provider::imap::connection::ImapConfig::from_db(pool.inner(), &account.id).await?;
        let provider = crate::provider::imap::provider::ImapProvider::new(config.clone());

        let _ = provider.list_folders(pool.inner()).await;

        let folder = label_id.as_deref().unwrap_or("INBOX");
        let result = provider.sync_folder(pool.inner(), folder).await?;

        let idle_manager = app_handle.state::<crate::provider::imap::idle::IdleManager>();
        idle_manager.start_for_account(config, app_handle.clone()).await;

        return Ok(SyncResult {
            new_message_ids: vec![],
            new_thread_ids: result.updated_thread_ids,
        });
    }

    // Spawn background contact sync + discovery backfill (non-blocking, throttled internally)
    let bg_pool_contacts = pool.inner().clone();
    let bg_account_id_contacts = account.id.clone();
    tokio::spawn(async move {
        if let Err(e) = super::contacts::sync_contacts_inner(&bg_pool_contacts, &bg_account_id_contacts).await {
            tracing::warn!("Contact sync failed: {}", e);
        }
        // Backfill discovered contacts from message history
        let email: String = sqlx::query_scalar("SELECT email FROM accounts WHERE id = ?")
            .bind(&bg_account_id_contacts)
            .fetch_optional(&bg_pool_contacts)
            .await
            .unwrap_or(None)
            .unwrap_or_default();
        if !email.is_empty() {
            if let Err(e) = crate::contacts::discovery::backfill_discovered_contacts(
                &bg_pool_contacts,
                &bg_account_id_contacts,
                &email,
            )
            .await
            {
                tracing::warn!("Contact discovery backfill failed: {}", e);
            }
        }
    });

    if provider_type == "outlook" {
        crate::outlook_api::fetch_and_store_outlook_folders(pool.inner(), &account.id, &account.access_token).await?;
        let folder = label_id.as_deref().unwrap_or("inbox");
        let delta = crate::outlook_api::outlook_delta_sync(pool.inner(), &account.id, &account.access_token, folder).await?;
        return Ok(SyncResult {
            new_message_ids: vec![],
            new_thread_ids: delta.new_thread_ids,
        });
    }

    crate::gmail_api::fetch_and_store_labels(pool.inner(), &account.id, &account.access_token)
        .await?;

    let mut new_message_ids: Vec<String> = Vec::new();
    let mut new_thread_ids: Vec<String> = Vec::new();

    let last_history_id =
        crate::gmail_api::get_last_history_id(pool.inner(), &account.id).await;

    if let Some(ref history_id) = last_history_id {
        tracing::info!("Incremental sync from historyId={}", history_id);
        let result = crate::gmail_api::fetch_history(
            pool.inner(),
            &account.id,
            &account.access_token,
            history_id,
        )
        .await;

        match result {
            Ok(Some(delta)) => {
                if !delta.threads_to_hydrate.is_empty() {
                    crate::gmail_api::batch_metadata_hydrate(
                        pool.inner(),
                        &account.id,
                        &account.access_token,
                        delta.threads_to_hydrate,
                    )
                    .await;
                }
                crate::gmail_api::set_last_history_id(
                    pool.inner(),
                    &account.id,
                    &delta.new_history_id,
                )
                .await;
                new_message_ids = delta.new_inbox_message_ids;
                new_thread_ids = delta.new_inbox_thread_ids;
            }
            Ok(None) => {
                tracing::info!("History expired (404), falling back to full sync");
                full_sync(pool.inner(), &account, &label_id, &app_handle).await?;
            }
            Err(e) => {
                tracing::warn!("Incremental sync error: {}, falling back to full sync", e);
                full_sync(pool.inner(), &account, &label_id, &app_handle).await?;
            }
        }
    } else {
        tracing::info!("No historyId stored, running full sync");
        full_sync(pool.inner(), &account, &label_id, &app_handle).await?;
    }

    spawn_background_cleanup(pool.inner(), &account, &app_handle);

    Ok(SyncResult { new_message_ids, new_thread_ids })
}

async fn full_sync(
    pool: &sqlx::SqlitePool,
    account: &super::accounts::ActiveAccountFull,
    label_id: &Option<String>,
    app_handle: &tauri::AppHandle,
) -> Result<(), String> {
    let token_store = app_handle.state::<crate::page_token_store::PageTokenStore>();

    let target_labels = if let Some(ref lid) = label_id {
        vec![lid.as_str()]
    } else {
        vec!["INBOX"]
    };

    let sync_key = format!("{}:{}", account.id, target_labels.first().unwrap_or(&"INBOX"));
    token_store.remove(&sync_key);

    let fetch_result = crate::gmail_api::fetch_and_store_threads(
        pool,
        &account.id,
        &account.access_token,
        Some(&target_labels),
        100,
        None,
        None,
    )
    .await?;

    if let Some(token) = fetch_result.next_page_token {
        token_store.set(&sync_key, token);
    }

    let stale = crate::gmail_api::get_stale_thread_ids(pool, &account.id).await;
    if !stale.is_empty() {
        tracing::info!("Re-hydrating {} stale threads", stale.len());
        crate::gmail_api::batch_metadata_hydrate(
            pool,
            &account.id,
            &account.access_token,
            stale,
        )
        .await;
    }

    let no_metadata = crate::gmail_api::get_no_metadata_thread_ids(pool, &account.id).await;
    if !no_metadata.is_empty() {
        let batch: Vec<String> = no_metadata.into_iter().take(100).collect();
        crate::gmail_api::batch_metadata_hydrate(
            pool,
            &account.id,
            &account.access_token,
            batch,
        )
        .await;
    }

    if let Ok(history_id) = crate::gmail_api::get_profile_history_id(&account.access_token).await {
        crate::gmail_api::set_last_history_id(pool, &account.id, &history_id).await;
        tracing::info!("Stored historyId={} from profile after full sync", history_id);
    }

    Ok(())
}

fn spawn_background_cleanup(
    pool: &sqlx::SqlitePool,
    account: &super::accounts::ActiveAccountFull,
    app_handle: &tauri::AppHandle,
) {
    let bg_pool = pool.clone();
    let bg_account_id = account.id.clone();
    let bg_token = account.access_token.clone();
    let bg_app = app_handle.clone();
    tokio::spawn(async move {
        let no_metadata =
            crate::gmail_api::get_no_metadata_thread_ids(&bg_pool, &bg_account_id).await;
        if !no_metadata.is_empty() {
            crate::gmail_api::batch_metadata_hydrate(
                &bg_pool,
                &bg_account_id,
                &bg_token,
                no_metadata,
            )
            .await;
        }

        let app_dir = bg_app.path().app_data_dir().unwrap_or_default();
        let db_path = app_dir.join("rustymail.db");
        let db_size_mb = std::fs::metadata(&db_path)
            .map(|m| m.len() / (1024 * 1024))
            .unwrap_or(0);
        #[derive(sqlx::FromRow)]
        struct S {
            value: String,
        }
        let max_mb: u64 =
            sqlx::query_as::<_, S>("SELECT value FROM settings WHERE key = 'max_cache_mb'")
                .fetch_optional(&bg_pool)
                .await
                .unwrap_or(None)
                .and_then(|r| r.value.parse().ok())
                .unwrap_or(500);
        if db_size_mb > max_mb {
            crate::gmail_api::evict_old_message_bodies(&bg_pool, &bg_account_id, 200).await;
        }
    });
}

#[tauri::command]
pub async fn ensure_threads_hydrated(
    app_handle: tauri::AppHandle,
    thread_ids: Vec<String>,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    let mut need_hydration = Vec::new();
    for tid in &thread_ids {
        #[derive(sqlx::FromRow)]
        struct Count {
            cnt: i32,
        }
        let has_msgs =
            sqlx::query_as::<_, Count>("SELECT COUNT(*) as cnt FROM messages WHERE thread_id = ?")
                .bind(tid)
                .fetch_one(pool.inner())
                .await
                .map(|r| r.cnt)
                .unwrap_or(0);
        if has_msgs == 0 {
            need_hydration.push(tid.clone());
        }
    }

    let provider_type = super::accounts::get_provider_type(pool.inner(), &account.id).await;
    if provider_type == "imap" {
        let config = crate::provider::imap::connection::ImapConfig::from_db(pool.inner(), &account.id).await?;
        let provider = crate::provider::imap::provider::ImapProvider::new(config);
        for tid in &need_hydration {
            let msg_ids: Vec<(String,)> = sqlx::query_as(
                "SELECT id FROM messages WHERE thread_id = ? AND body_html = ''",
            )
            .bind(tid)
            .fetch_all(pool.inner())
            .await
            .unwrap_or_default();
            for (mid,) in msg_ids {
                let _ = provider.fetch_message_body(pool.inner(), &mid).await;
            }
        }
        return Ok(());
    }

    if provider_type == "outlook" {
        for tid in &need_hydration {
            let msg_ids: Vec<(String,)> = sqlx::query_as(
                "SELECT id FROM messages WHERE thread_id = ? AND body_html = ''",
            )
            .bind(tid)
            .fetch_all(pool.inner())
            .await
            .unwrap_or_default();
            for (mid,) in msg_ids {
                let _ = crate::outlook_api::fetch_outlook_message_body(
                    pool.inner(),
                    &account.access_token,
                    &mid,
                )
                .await;
            }
        }
        return Ok(());
    }

    if !need_hydration.is_empty() {
        tracing::info!("Hydration started for {} threads", need_hydration.len());
        crate::gmail_api::batch_hydrate_threads(
            pool.inner(),
            &account.id,
            &account.access_token,
            need_hydration,
        )
        .await;
        tracing::info!("Hydration completed");
    }

    Ok(())
}
