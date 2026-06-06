use super::accounts::get_active_account;
use crate::semantic_search::{self, EmbeddingStatus, SearchResult};
use tauri::Manager;

#[tauri::command]
pub async fn semantic_search_query(
    app_handle: tauri::AppHandle,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let max = limit.unwrap_or(20).min(50);

    // Semantic search requires the premium engine for embeddings
    #[cfg(not(feature = "premium"))]
    return Err("Semantic search requires the premium feature (local AI engine)".to_string());

    #[cfg(feature = "premium")]
    {
    let query_embedding = {
        let engine = app_handle.state::<rustymail_premium::llm::engine::LlmEngine>();
        engine.embed(&query).await.map_err(|e| format!("Embedding failed: {e}"))?
    };

    // Load stored embeddings for this account's messages
    #[derive(sqlx::FromRow)]
    struct EmbeddingRow {
        message_id: String,
        embedding: Vec<u8>,
    }

    let rows: Vec<EmbeddingRow> = sqlx::query_as(
        "SELECT e.message_id, e.embedding FROM email_embeddings e
         INNER JOIN messages m ON e.message_id = m.id
         WHERE m.account_id = ?
         LIMIT 5000"
    )
    .bind(&account.id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| format!("Failed to load embeddings: {e}"))?;

    if rows.is_empty() {
        return Ok(vec![]);
    }

    // Compute cosine similarity for each
    let mut scored: Vec<(String, f32)> = rows
        .iter()
        .map(|r| {
            let embedding = semantic_search::deserialize_embedding(&r.embedding);
            let score = semantic_search::cosine_similarity(&query_embedding, &embedding);
            (r.message_id.clone(), score)
        })
        .collect();

    // Sort by score descending, take top N
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(max);

    // Fetch message details for top results
    #[derive(sqlx::FromRow)]
    struct MsgInfo {
        thread_id: String,
        subject: Option<String>,
        sender: Option<String>,
        snippet: Option<String>,
    }

    let mut results = Vec::new();
    for (msg_id, score) in &scored {
        if let Ok(Some(info)) = sqlx::query_as::<_, MsgInfo>(
            "SELECT thread_id, subject, sender, snippet FROM messages WHERE id = ?"
        )
        .bind(msg_id)
        .fetch_optional(pool.inner())
        .await
        {
            results.push(SearchResult {
                message_id: msg_id.clone(),
                thread_id: info.thread_id,
                subject: info.subject.unwrap_or_default(),
                sender: info.sender.unwrap_or_default(),
                snippet: info.snippet.unwrap_or_default(),
                score: *score,
            });
        }
    }

    Ok(results)
    } // #[cfg(feature = "premium")]
}

#[tauri::command]
pub async fn get_embedding_status(
    app_handle: tauri::AppHandle,
) -> Result<EmbeddingStatus, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM messages WHERE account_id = ?"
    ).bind(&account.id).fetch_one(pool.inner()).await.map_err(|e| e.to_string())?;

    let embedded: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM messages WHERE account_id = ? AND embedded = 1"
    ).bind(&account.id).fetch_one(pool.inner()).await.map_err(|e| e.to_string())?;

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
    use super::super::test_helpers::{setup_test_db, insert_account, insert_message, insert_thread};

    #[tokio::test]
    async fn test_embedding_table_exists_after_migration() {
        let pool = setup_test_db().await;
        let result: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='email_embeddings'"
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(result.0, 1);
    }

    #[tokio::test]
    async fn test_messages_embedded_column() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "sender@test.com", "me@test.com", "Subject", 1000).await;

        let embedded: (i64,) = sqlx::query_as(
            "SELECT embedded FROM messages WHERE id = ?"
        ).bind("m1").fetch_one(&pool).await.unwrap();
        assert_eq!(embedded.0, 0); // default
    }

    #[tokio::test]
    async fn test_insert_and_query_embedding() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "r@t.com", "Subj", 1000).await;

        let vec = vec![0.5f32; 768];
        let blob = crate::semantic_search::serialize_embedding(&vec);
        let now = chrono::Utc::now().timestamp();

        sqlx::query("INSERT INTO email_embeddings (message_id, embedding, indexed_at) VALUES (?, ?, ?)")
            .bind("m1").bind(&blob).bind(now)
            .execute(&pool).await.unwrap();

        let (stored_blob,): (Vec<u8>,) = sqlx::query_as(
            "SELECT embedding FROM email_embeddings WHERE message_id = ?"
        ).bind("m1").fetch_one(&pool).await.unwrap();

        let recovered = crate::semantic_search::deserialize_embedding(&stored_blob);
        assert_eq!(recovered.len(), 768);
        for v in &recovered {
            assert!((v - 0.5).abs() < 1e-6);
        }
    }
}
