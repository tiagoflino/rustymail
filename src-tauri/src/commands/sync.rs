use super::accounts::get_active_account;
use tauri::Manager;

#[tauri::command]
pub async fn sync_gmail_data(
    app_handle: tauri::AppHandle,
    label_id: Option<String>,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    println!("[Sync] Starting fast sync for: {}", account.id);

    crate::gmail_api::fetch_and_store_labels(pool.inner(), &account.id, &account.access_token)
        .await?;

    let target_labels = if let Some(ref lid) = label_id {
        vec![lid.as_str()]
    } else {
        vec!["INBOX"]
    };

    crate::gmail_api::fetch_and_store_threads(
        pool.inner(),
        &account.id,
        &account.access_token,
        Some(&target_labels),
        100,
    )
    .await?;

    #[derive(sqlx::FromRow)]
    struct PrefetchSetting { value: String }
    let prefetch = sqlx::query_as::<_, PrefetchSetting>("SELECT value FROM settings WHERE key = 'prefetch_bodies'")
        .fetch_optional(pool.inner())
        .await
        .unwrap_or(None)
        .map(|r| r.value == "true")
        .unwrap_or(false);

    // Re-hydrate threads with new activity (history_id changed on Gmail)
    let stale = crate::gmail_api::get_stale_thread_ids(pool.inner(), &account.id).await;
    if !stale.is_empty() {
        println!("[Sync] Re-hydrating {} stale threads", stale.len());
        crate::gmail_api::batch_hydrate_threads(
            pool.inner(),
            &account.id,
            &account.access_token,
            stale,
        )
        .await;
    }

    // Hydrate threads that have never been fetched
    let unhydrated = crate::gmail_api::get_unhydrated_thread_ids(pool.inner(), &account.id).await;
    if !unhydrated.is_empty() {
        let limit = if prefetch { unhydrated.len() } else { 100 };
        let batch: Vec<String> = unhydrated.into_iter().take(limit).collect();
        crate::gmail_api::batch_hydrate_threads(
            pool.inner(),
            &account.id,
            &account.access_token,
            batch,
        )
        .await;
    }

    let bg_pool = pool.inner().clone();
    let bg_account_id = account.id.clone();
    let bg_token = account.access_token.clone();
    let bg_app = app_handle.clone();
    tokio::spawn(async move {
        let all_unhydrated =
            crate::gmail_api::get_unhydrated_thread_ids(&bg_pool, &bg_account_id).await;
        if !all_unhydrated.is_empty() {
            crate::gmail_api::batch_hydrate_threads(
                &bg_pool,
                &bg_account_id,
                &bg_token,
                all_unhydrated,
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

    Ok(())
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
    if !need_hydration.is_empty() {
        crate::gmail_api::batch_hydrate_threads(
            pool.inner(),
            &account.id,
            &account.access_token,
            need_hydration,
        )
        .await;
    }

    Ok(())
}
