//! Semantic email search using local embeddings.
//!
//! Architecture:
//! - Embeddings stored as BLOBs in SQLite (no vector extension needed)
//! - Cosine similarity computed in Rust (brute-force scan is <50ms at this scale)
//! - Embeddings produced by the local llama.cpp engine in `rustymail-premium`
//!   (embedding mode, mean pooling) — local-only, never leaves the device
//! - Opportunistic batch processing with resource guardrails (see embedding_scheduler)
//!
//! The functions here are embedder-agnostic: they accept already-computed
//! `&[f32]` vectors, which keeps the persistence/search logic fully testable
//! on the default (non-premium) build without loading any model. The
//! production consumers (search command, ingestion batch, scheduler) are
//! premium-gated, so on a non-premium, non-test build this module is inert.
#![cfg_attr(not(feature = "premium"), allow(dead_code))]

use sqlx::SqlitePool;

/// Native embedding dimension of the local model (IBM Granite 4.1 hidden size).
/// Kept as a hint only — actual stored vectors use whatever dimension the
/// engine reports at runtime, and search never assumes a fixed dimension.
#[cfg(test)]
pub const EMBEDDING_DIM: usize = 2560;

const MAX_EMBED_CHARS: usize = 2000;

/// Compute cosine similarity between two equal-length float vectors.
/// Returns a value in [-1.0, 1.0] where 1.0 is identical. Returns 0.0 when
/// either vector is degenerate (zero norm) or the lengths differ.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for i in 0..a.len() {
        let ai = a[i] as f64;
        let bi = b[i] as f64;
        dot += ai * bi;
        norm_a += ai * ai;
        norm_b += bi * bi;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-12 {
        return 0.0;
    }
    (dot / denom) as f32
}

/// Serialize a `Vec<f32>` to a `Vec<u8>` (little-endian f32 bytes).
pub fn serialize_embedding(vec: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vec.len() * 4);
    for v in vec {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

/// Deserialize bytes back to `Vec<f32>`. Trailing bytes that don't form a full
/// f32 are ignored.
pub fn deserialize_embedding(bytes: &[u8]) -> Vec<f32> {
    let count = bytes.len() / 4;
    let mut vec = Vec::with_capacity(count);
    for i in 0..count {
        let start = i * 4;
        let arr: [u8; 4] = [bytes[start], bytes[start + 1], bytes[start + 2], bytes[start + 3]];
        vec.push(f32::from_le_bytes(arr));
    }
    vec
}

/// Extract text suitable for embedding from an email.
/// Prioritizes subject + the first `MAX_EMBED_CHARS` characters of the body for
/// a quality/speed balance. Truncation is char-boundary safe (never panics on
/// multi-byte UTF-8 such as emoji/CJK).
pub fn extract_embedding_text(subject: &str, body_plain: &str) -> String {
    let body = truncate_chars(body_plain, MAX_EMBED_CHARS);
    format!("Subject: {}\n\n{}", subject, body)
}

/// Truncate a string to at most `max_chars` characters, respecting UTF-8
/// character boundaries.
fn truncate_chars(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    pub message_id: String,
    pub thread_id: String,
    pub subject: String,
    pub sender: String,
    pub snippet: String,
    pub score: f32,
    /// Message timestamp (epoch seconds) so the UI can sort/display correctly
    /// instead of fabricating a zero date.
    pub internal_date: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingStatus {
    pub total_messages: i64,
    pub embedded_messages: i64,
    pub pending_messages: i64,
    pub is_processing: bool,
    pub progress_pct: f32,
}

/// A message awaiting embedding (subject + body to embed).
/// `subject`/`body_plain` are consumed by the premium ingestion batch; on the
/// default build only `id` is read (the fields are still selected for tests).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PendingMessage {
    pub id: String,
    #[sqlx(default)]
    #[cfg_attr(not(feature = "premium"), allow(dead_code))]
    pub subject: Option<String>,
    #[sqlx(default)]
    #[cfg_attr(not(feature = "premium"), allow(dead_code))]
    pub body_plain: Option<String>,
}

/// Persist an embedding for a message and flip its `embedded` flag in a single
/// transaction so the `email_embeddings` table and `messages.embedded` can
/// never diverge. This is the one and only write path for embeddings.
pub async fn persist_embedding(
    pool: &SqlitePool,
    message_id: &str,
    embedding: &[f32],
) -> Result<(), sqlx::Error> {
    let blob = serialize_embedding(embedding);
    let now = chrono::Utc::now().timestamp();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO email_embeddings (message_id, embedding, indexed_at)
         VALUES (?, ?, ?)
         ON CONFLICT(message_id) DO UPDATE SET embedding = excluded.embedding, indexed_at = excluded.indexed_at",
    )
    .bind(message_id)
    .bind(&blob)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE messages SET embedded = 1 WHERE id = ?")
        .bind(message_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Fetch up to `limit` messages for an account that have not been embedded yet.
pub async fn fetch_pending_messages(
    pool: &SqlitePool,
    account_id: &str,
    limit: i64,
) -> Result<Vec<PendingMessage>, sqlx::Error> {
    sqlx::query_as::<_, PendingMessage>(
        "SELECT id, subject, body_plain FROM messages
         WHERE account_id = ? AND embedded = 0
         ORDER BY internal_date DESC
         LIMIT ?",
    )
    .bind(account_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Count messages for an account that still need embedding.
pub async fn count_pending(pool: &SqlitePool, account_id: &str) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM messages WHERE account_id = ? AND embedded = 0",
    )
    .bind(account_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Reconcile the `messages.embedded` flag with the `email_embeddings` table.
/// Repairs any divergence (e.g. rows deleted from one side, or a flag set
/// without a stored vector). Returns the number of rows whose flag changed.
pub async fn backfill_embedded_flag(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    // Set embedded=1 where a vector exists but the flag is unset.
    let set = sqlx::query(
        "UPDATE messages SET embedded = 1
         WHERE embedded = 0
           AND id IN (SELECT message_id FROM email_embeddings)",
    )
    .execute(pool)
    .await?
    .rows_affected();
    // Clear embedded=1 where no vector exists (stale flag).
    let clear = sqlx::query(
        "UPDATE messages SET embedded = 0
         WHERE embedded = 1
           AND id NOT IN (SELECT message_id FROM email_embeddings)",
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(set + clear)
}

/// Score a query embedding against a set of stored `(message_id, raw_bytes)`
/// embeddings and return the top `k` by cosine similarity, descending.
/// Uses a bounded min-heap so memory/time stay O(n log k) rather than a full
/// sort of every candidate.
pub fn rank_top_k(
    query: &[f32],
    stored: &[(String, Vec<u8>)],
    k: usize,
) -> Vec<(String, f32)> {
    use std::cmp::Ordering;
    use std::collections::BinaryHeap;

    if k == 0 {
        return Vec::new();
    }

    // Min-heap by score: the smallest score sits at the top so we can evict it
    // once the heap is full.
    #[derive(PartialEq)]
    struct Scored {
        score: f32,
        id: String,
    }
    impl Eq for Scored {}
    impl Ord for Scored {
        fn cmp(&self, other: &Self) -> Ordering {
            // Reverse so BinaryHeap (a max-heap) behaves as a min-heap on score.
            other
                .score
                .partial_cmp(&self.score)
                .unwrap_or(Ordering::Equal)
        }
    }
    impl PartialOrd for Scored {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    let mut heap: BinaryHeap<Scored> = BinaryHeap::with_capacity(k + 1);
    for (id, bytes) in stored {
        let emb = deserialize_embedding(bytes);
        let score = cosine_similarity(query, &emb);
        if heap.len() < k {
            heap.push(Scored { score, id: id.clone() });
        } else if let Some(min) = heap.peek() {
            if score > min.score {
                heap.pop();
                heap.push(Scored { score, id: id.clone() });
            }
        }
    }

    let mut out: Vec<(String, f32)> = heap.into_iter().map(|s| (s.id, s.score)).collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_helpers::{insert_account, insert_message, insert_thread, setup_test_db};

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0f32, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 0.001, "Identical vectors should have similarity ~1.0, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![0.0f32, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 0.001, "Orthogonal vectors should have similarity ~0.0, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0f32, 0.0];
        let b = vec![-1.0f32, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 0.001, "Opposite vectors should have similarity ~-1.0, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0f32, 0.0];
        let b = vec![1.0f32, 2.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0, "Zero vector should return 0 similarity");
    }

    #[test]
    fn test_cosine_similarity_mismatched_len() {
        let a = vec![1.0f32, 2.0, 3.0];
        let b = vec![1.0f32, 2.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let original = vec![1.0f32, -0.5, 3.14159, 0.0, 42.0];
        let bytes = serialize_embedding(&original);
        assert_eq!(bytes.len(), original.len() * 4);
        let recovered = deserialize_embedding(&bytes);
        assert_eq!(recovered.len(), original.len());
        for (i, (a, b)) in original.iter().zip(recovered.iter()).enumerate() {
            assert!((a - b).abs() < 1e-6, "Mismatch at index {i}: {a} vs {b}");
        }
    }

    #[test]
    fn test_deserialize_empty() {
        let vec = deserialize_embedding(&[]);
        assert!(vec.is_empty());
    }

    #[test]
    fn test_serialize_dimension() {
        let embedding = vec![0.0f32; EMBEDDING_DIM];
        let bytes = serialize_embedding(&embedding);
        assert_eq!(bytes.len(), EMBEDDING_DIM * 4);
    }

    #[test]
    fn test_extract_embedding_text() {
        let subject = "Meeting Tomorrow";
        let body = "Hi team, let's meet at 3pm to discuss the Q3 budget.";
        let text = extract_embedding_text(subject, body);
        assert!(text.contains("Meeting Tomorrow"));
        assert!(text.contains("Q3 budget"));
    }

    #[test]
    fn test_extract_embedding_text_truncates_long_body() {
        let subject = "Test";
        let body = "a".repeat(5000);
        let text = extract_embedding_text(subject, &body);
        // Subject prefix "Subject: " (9) + subject + "\n\n" (2) + 2000 body chars.
        assert!(text.len() <= MAX_EMBED_CHARS + subject.len() + 11);
    }

    #[test]
    fn test_extract_embedding_text_multibyte_no_panic() {
        // A body where byte index 2000 lands mid-codepoint: 2500 four-byte emoji
        // = 10000 bytes, so byte 2000 is inside a codepoint. Byte slicing would
        // panic; char-safe truncation must not, and must keep exactly
        // MAX_EMBED_CHARS emoji.
        let subject = "📧 multibyte";
        let body = "😀".repeat(2500);
        let text = extract_embedding_text(subject, &body);
        assert!(text.starts_with("Subject: 📧 multibyte"));
        let emoji_count = text.matches('😀').count();
        assert_eq!(emoji_count, MAX_EMBED_CHARS);
    }

    #[test]
    fn test_extract_embedding_text_cjk_no_panic() {
        let body = "日本語".repeat(1000); // 3000 chars, 9000 bytes
        let text = extract_embedding_text("件名", &body);
        assert!(text.contains("件名"));
    }

    #[test]
    fn test_rank_top_k_orders_by_similarity() {
        let query = vec![1.0f32, 0.0, 0.0];
        let stored = vec![
            ("close".to_string(), serialize_embedding(&[0.9, 0.1, 0.0])),
            ("far".to_string(), serialize_embedding(&[0.0, 1.0, 0.0])),
            ("closest".to_string(), serialize_embedding(&[1.0, 0.0, 0.0])),
        ];
        let ranked = rank_top_k(&query, &stored, 2);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].0, "closest");
        assert_eq!(ranked[1].0, "close");
    }

    #[test]
    fn test_rank_top_k_zero() {
        let query = vec![1.0f32];
        let stored = vec![("a".to_string(), serialize_embedding(&[1.0]))];
        assert!(rank_top_k(&query, &stored, 0).is_empty());
    }

    #[tokio::test]
    async fn test_persist_embedding_sets_flag_and_table_transactionally() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "r@t.com", "Subj", 1000).await;

        let emb = vec![0.25f32; 16];
        persist_embedding(&pool, "m1", &emb).await.unwrap();

        // Flag flipped.
        let flag: (i64,) = sqlx::query_as("SELECT embedded FROM messages WHERE id = ?")
            .bind("m1").fetch_one(&pool).await.unwrap();
        assert_eq!(flag.0, 1);

        // Vector stored and recoverable.
        let (blob,): (Vec<u8>,) = sqlx::query_as(
            "SELECT embedding FROM email_embeddings WHERE message_id = ?",
        ).bind("m1").fetch_one(&pool).await.unwrap();
        let recovered = deserialize_embedding(&blob);
        assert_eq!(recovered.len(), 16);
    }

    #[tokio::test]
    async fn test_persist_embedding_is_idempotent() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "r@t.com", "Subj", 1000).await;

        persist_embedding(&pool, "m1", &[0.1f32; 8]).await.unwrap();
        persist_embedding(&pool, "m1", &[0.9f32; 8]).await.unwrap();

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM email_embeddings WHERE message_id = ?",
        ).bind("m1").fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 1, "Re-embedding must update in place, not duplicate");

        let (blob,): (Vec<u8>,) = sqlx::query_as(
            "SELECT embedding FROM email_embeddings WHERE message_id = ?",
        ).bind("m1").fetch_one(&pool).await.unwrap();
        let recovered = deserialize_embedding(&blob);
        assert!((recovered[0] - 0.9).abs() < 1e-6, "Latest embedding should win");
    }

    #[tokio::test]
    async fn test_fetch_pending_and_count() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "r@t.com", "Subj1", 1000).await;
        insert_message(&pool, "m2", "t1", "acc1", "s@t.com", "r@t.com", "Subj2", 2000).await;

        assert_eq!(count_pending(&pool, "acc1").await.unwrap(), 2);
        persist_embedding(&pool, "m1", &[0.5f32; 4]).await.unwrap();
        assert_eq!(count_pending(&pool, "acc1").await.unwrap(), 1);

        let pending = fetch_pending_messages(&pool, "acc1", 10).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "m2");
    }

    #[tokio::test]
    async fn test_backfill_repairs_divergence() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "r@t.com", "Subj", 1000).await;
        insert_message(&pool, "m2", "t1", "acc1", "s@t.com", "r@t.com", "Subj", 2000).await;

        // Divergence A: vector present, flag unset (simulate a crash between writes).
        sqlx::query("INSERT INTO email_embeddings (message_id, embedding, indexed_at) VALUES ('m1', ?, 1)")
            .bind(serialize_embedding(&[0.1f32; 4]))
            .execute(&pool).await.unwrap();
        // Divergence B: flag set, no vector.
        sqlx::query("UPDATE messages SET embedded = 1 WHERE id = 'm2'")
            .execute(&pool).await.unwrap();

        let changed = backfill_embedded_flag(&pool).await.unwrap();
        assert_eq!(changed, 2);

        let f1: (i64,) = sqlx::query_as("SELECT embedded FROM messages WHERE id='m1'").fetch_one(&pool).await.unwrap();
        let f2: (i64,) = sqlx::query_as("SELECT embedded FROM messages WHERE id='m2'").fetch_one(&pool).await.unwrap();
        assert_eq!(f1.0, 1, "m1 should be marked embedded (vector exists)");
        assert_eq!(f2.0, 0, "m2 flag should be cleared (no vector)");
    }

    /// ACCEPTANCE TEST: ingest an embedding for a message, then run the same
    /// brute-force search the command performs, and prove it returns a
    /// NON-EMPTY result for a semantically-close query. This is the bar the PR
    /// review demanded: search returns something after ingestion.
    #[tokio::test]
    async fn test_ingest_then_search_returns_nonempty() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@t.com", "me@t.com", "Quarterly budget review", 1000).await;
        insert_message(&pool, "m2", "t1", "acc1", "bob@t.com", "me@t.com", "Lunch plans", 2000).await;

        // Ingest two distinct embeddings (stand-ins for model output).
        persist_embedding(&pool, "m1", &[1.0f32, 0.0, 0.0]).await.unwrap();
        persist_embedding(&pool, "m2", &[0.0f32, 1.0, 0.0]).await.unwrap();

        // Load stored vectors exactly as the command does.
        let rows: Vec<(String, Vec<u8>)> = sqlx::query_as(
            "SELECT e.message_id, e.embedding FROM email_embeddings e
             INNER JOIN messages m ON e.message_id = m.id
             WHERE m.account_id = ?",
        )
        .bind("acc1")
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2, "Both embeddings must be stored");

        // A query close to m1's vector.
        let query = vec![0.95f32, 0.05, 0.0];
        let ranked = rank_top_k(&query, &rows, 20);

        assert!(!ranked.is_empty(), "Search MUST return non-empty after ingestion");
        assert_eq!(ranked[0].0, "m1", "Closest message should rank first");
        assert!(ranked[0].1 > ranked[1].1, "Scores should be ordered");
    }

    #[test]
    fn test_search_result_serialization() {
        let result = SearchResult {
            message_id: "m1".to_string(),
            thread_id: "t1".to_string(),
            subject: "Hello".to_string(),
            sender: "alice@example.com".to_string(),
            snippet: "Hi there".to_string(),
            score: 0.85,
            internal_date: 1700000000,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("m1"));
        assert!(json.contains("0.85"));
        assert!(json.contains("1700000000"));
    }

    #[test]
    fn test_embedding_status() {
        let status = EmbeddingStatus {
            total_messages: 1000,
            embedded_messages: 750,
            pending_messages: 250,
            is_processing: false,
            progress_pct: 75.0,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("750"));
        assert!(json.contains("75.0"));
    }
}
