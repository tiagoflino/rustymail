use crate::semantic_search::{EmbeddingStatus, SearchResult};
#[cfg(feature = "premium")]
use crate::semantic_search;
#[cfg(feature = "premium")]
use super::accounts::get_active_account;
#[cfg(feature = "premium")]
use tauri::Manager;

#[tauri::command]
pub async fn semantic_search_query(
    app_handle: tauri::AppHandle,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, String> {
    // Semantic search requires the premium engine for embeddings. Bail out
    // before doing any DB work on non-premium builds.
    #[cfg(not(feature = "premium"))]
    {
        let _ = (app_handle, query, limit);
        Err("Semantic search requires the premium feature (local AI engine)".to_string())
    }

    #[cfg(feature = "premium")]
    {
        let pool = app_handle.state::<sqlx::SqlitePool>();
        let account = get_active_account(pool.inner()).await?;
        let max = limit.unwrap_or(20).min(50);

        let query_embedding = {
            let engine = app_handle.state::<rustymail_premium::llm::engine::LlmEngine>();
            engine
                .embed(&query)
                .await
                .map_err(|e| format!("Embedding failed: {e}"))?
        };

        let stored = load_account_embeddings(pool.inner(), &account.id).await?;
        if stored.is_empty() {
            return Ok(vec![]);
        }

        let ranked = semantic_search::rank_top_k(&query_embedding, &stored, max);
        if ranked.is_empty() {
            return Ok(vec![]);
        }

        hydrate_results(pool.inner(), &ranked).await
    }
}

/// Load all stored `(message_id, embedding_bytes)` for an account.
#[cfg(feature = "premium")]
async fn load_account_embeddings(
    pool: &sqlx::SqlitePool,
    account_id: &str,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    #[derive(sqlx::FromRow)]
    struct EmbeddingRow {
        message_id: String,
        embedding: Vec<u8>,
    }

    let rows: Vec<EmbeddingRow> = sqlx::query_as(
        "SELECT e.message_id, e.embedding FROM email_embeddings e
         INNER JOIN messages m ON e.message_id = m.id
         WHERE m.account_id = ?
         LIMIT 5000",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to load embeddings: {e}"))?;

    Ok(rows.into_iter().map(|r| (r.message_id, r.embedding)).collect())
}

/// Hydrate ranked `(message_id, score)` pairs into `SearchResult`s using a
/// single `WHERE id IN (...)` query (no N+1), preserving the ranked order.
#[cfg(feature = "premium")]
async fn hydrate_results(
    pool: &sqlx::SqlitePool,
    ranked: &[(String, f32)],
) -> Result<Vec<SearchResult>, String> {
    #[derive(sqlx::FromRow)]
    struct MsgInfo {
        id: String,
        thread_id: String,
        subject: Option<String>,
        sender: Option<String>,
        snippet: Option<String>,
        internal_date: Option<i64>,
    }

    let placeholders = std::iter::repeat_n("?", ranked.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT id, thread_id, subject, sender, snippet, internal_date FROM messages WHERE id IN ({placeholders})"
    );
    let mut q = sqlx::query_as::<_, MsgInfo>(&sql);
    for (id, _) in ranked {
        q = q.bind(id);
    }
    let infos = q
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Failed to hydrate results: {e}"))?;

    let by_id: std::collections::HashMap<String, MsgInfo> =
        infos.into_iter().map(|m| (m.id.clone(), m)).collect();

    let results = ranked
        .iter()
        .filter_map(|(msg_id, score)| {
            by_id.get(msg_id).map(|info| SearchResult {
                message_id: msg_id.clone(),
                thread_id: info.thread_id.clone(),
                subject: info.subject.clone().unwrap_or_default(),
                sender: info.sender.clone().unwrap_or_default(),
                snippet: info.snippet.clone().unwrap_or_default(),
                score: *score,
                internal_date: info.internal_date.unwrap_or(0),
            })
        })
        .collect();

    Ok(results)
}

/// Embed a batch of pending messages for an account and persist them.
/// Returns the number of messages embedded. Premium-only: needs the engine.
#[cfg(feature = "premium")]
pub async fn run_embedding_batch(
    pool: &sqlx::SqlitePool,
    engine: &rustymail_premium::llm::engine::LlmEngine,
    account_id: &str,
    batch_size: i64,
) -> Result<usize, String> {
    let pending = semantic_search::fetch_pending_messages(pool, account_id, batch_size)
        .await
        .map_err(|e| format!("Failed to fetch pending messages: {e}"))?;

    let mut embedded = 0usize;
    for msg in pending {
        let text = semantic_search::extract_embedding_text(
            msg.subject.as_deref().unwrap_or(""),
            msg.body_plain.as_deref().unwrap_or(""),
        );
        if text.trim().is_empty() {
            // Nothing to embed; still flag it so we don't reprocess forever.
            let _ = semantic_search::persist_embedding(pool, &msg.id, &[]).await;
            continue;
        }
        match engine.embed(&text).await {
            Ok(vec) => {
                semantic_search::persist_embedding(pool, &msg.id, &vec)
                    .await
                    .map_err(|e| format!("Failed to persist embedding: {e}"))?;
                embedded += 1;
            }
            Err(e) => {
                return Err(format!("Embedding failed for {}: {e}", msg.id));
            }
        }
    }
    Ok(embedded)
}

#[tauri::command]
pub async fn get_embedding_status(
    app_handle: tauri::AppHandle,
) -> Result<EmbeddingStatus, String> {
    use tauri::Manager;
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = super::accounts::get_active_account(pool.inner()).await?;
    embedding_status_for(pool.inner(), &account.id).await
}

/// Pure status computation, shared by the command and tests.
pub async fn embedding_status_for(
    pool: &sqlx::SqlitePool,
    account_id: &str,
) -> Result<EmbeddingStatus, String> {
    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages WHERE account_id = ?")
        .bind(account_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    let embedded: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM messages WHERE account_id = ? AND embedded = 1")
            .bind(account_id)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;

    let pending = total.0 - embedded.0;
    let progress = if total.0 > 0 {
        (embedded.0 as f32 / total.0 as f32) * 100.0
    } else {
        100.0
    };

    Ok(EmbeddingStatus {
        total_messages: total.0,
        embedded_messages: embedded.0,
        pending_messages: pending,
        is_processing: false,
        progress_pct: progress,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic_search;
    use crate::commands::test_helpers::{insert_account, insert_message, insert_thread, setup_test_db};

    #[tokio::test]
    async fn test_embedding_table_exists_after_migration() {
        let pool = setup_test_db().await;
        let result: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='email_embeddings'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(result.0, 1);
    }

    #[tokio::test]
    async fn test_messages_embedded_column_defaults_zero() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "sender@test.com", "me@test.com", "Subject", 1000).await;

        let embedded: (i64,) = sqlx::query_as("SELECT embedded FROM messages WHERE id = ?")
            .bind("m1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(embedded.0, 0);
    }

    #[tokio::test]
    async fn test_embedding_status_reports_progress() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "r@t.com", "S1", 1000).await;
        insert_message(&pool, "m2", "t1", "acc1", "s@t.com", "r@t.com", "S2", 2000).await;
        insert_message(&pool, "m3", "t1", "acc1", "s@t.com", "r@t.com", "S3", 3000).await;
        insert_message(&pool, "m4", "t1", "acc1", "s@t.com", "r@t.com", "S4", 4000).await;

        // Initially nothing embedded.
        let st0 = embedding_status_for(&pool, "acc1").await.unwrap();
        assert_eq!(st0.total_messages, 4);
        assert_eq!(st0.embedded_messages, 0);
        assert_eq!(st0.pending_messages, 4);
        assert_eq!(st0.progress_pct, 0.0);

        // Embed one.
        semantic_search::persist_embedding(&pool, "m1", &[0.5f32; 4]).await.unwrap();
        let st1 = embedding_status_for(&pool, "acc1").await.unwrap();
        assert_eq!(st1.embedded_messages, 1);
        assert_eq!(st1.pending_messages, 3);
        assert_eq!(st1.progress_pct, 25.0);
    }

    #[tokio::test]
    async fn test_embedding_status_empty_account_is_complete() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        let st = embedding_status_for(&pool, "acc1").await.unwrap();
        assert_eq!(st.total_messages, 0);
        assert_eq!(st.progress_pct, 100.0);
    }

    #[tokio::test]
    async fn test_insert_and_query_embedding_roundtrip() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "r@t.com", "Subj", 1000).await;

        let vec = vec![0.5f32; 768];
        semantic_search::persist_embedding(&pool, "m1", &vec).await.unwrap();

        let (stored_blob,): (Vec<u8>,) =
            sqlx::query_as("SELECT embedding FROM email_embeddings WHERE message_id = ?")
                .bind("m1")
                .fetch_one(&pool)
                .await
                .unwrap();

        let recovered = semantic_search::deserialize_embedding(&stored_blob);
        assert_eq!(recovered.len(), 768);
        for v in &recovered {
            assert!((v - 0.5).abs() < 1e-6);
        }
    }
}
