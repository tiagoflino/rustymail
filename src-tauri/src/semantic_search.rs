/// Semantic email search using local embeddings.
///
/// Architecture:
/// - Embeddings stored as BLOBs in SQLite (no vector extension needed)
/// - Cosine similarity computed in Rust (fast for <5000 indexed emails)
/// - Granite model embeddings extracted via llama.cpp (premium crate)
/// - Opportunistic batch processing with resource guardrails
///
/// With 5000 emails × 768 dims = ~15MB vector data: search is <50ms.

const EMBEDDING_DIM: usize = 768;

/// Compute cosine similarity between two equal-length float vectors.
/// Returns a value in [-1.0, 1.0] where 1.0 is identical.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have same dimension");
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
    let denom = (norm_a.sqrt() * norm_b.sqrt()) as f64;
    if denom < 1e-12 {
        return 0.0;
    }
    (dot / denom) as f32
}

/// Serialize a Vec<f32> to a Vec<u8> (little-endian f32 bytes).
pub fn serialize_embedding(vec: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vec.len() * 4);
    for v in vec {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

/// Deserialize bytes back to Vec<f32>.
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
/// Prioritizes subject + first 2000 chars of body for quality/speed balance.
pub fn extract_embedding_text(subject: &str, body_plain: &str) -> String {
    let body = if body_plain.len() > 2000 {
        &body_plain[..2000]
    } else {
        body_plain
    };
    format!("Subject: {}\n\n{}", subject, body)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    pub message_id: String,
    pub thread_id: String,
    pub subject: String,
    pub sender: String,
    pub snippet: String,
    pub score: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingStatus {
    pub total_messages: i64,
    pub embedded_messages: i64,
    pub pending_messages: i64,
    pub is_processing: bool,
    pub progress_pct: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(text.len() <= 2000 + subject.len() + 13); // "Subject: \n\n" + subject + body
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
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("m1"));
        assert!(json.contains("0.85"));
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
