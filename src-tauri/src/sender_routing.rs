/// Sender routing — local equivalent of Hey.com's "The Screener"
/// Detects first-time senders and applies user-defined routing rules.

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum Routing {
    Inbox,
    Feed,
    AutoArchive,
    Blocked,
}

impl Routing {
    pub fn as_str(&self) -> &str {
        match self {
            Routing::Inbox => "inbox",
            Routing::Feed => "feed",
            Routing::AutoArchive => "auto_archive",
            Routing::Blocked => "blocked",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "feed" => Routing::Feed,
            "auto_archive" => Routing::AutoArchive,
            "blocked" => Routing::Blocked,
            _ => Routing::Inbox,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SenderRoutingInfo {
    pub sender_email: String,
    pub sender_name: Option<String>,
    pub routing: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Extract and normalize an email address for identity matching.
///
/// Per RFC 5321 §2.4 the domain is case-insensitive (normal DNS rules) while the
/// local-part is technically case-sensitive. In practice every major provider
/// treats the local-part case-insensitively, so for identity matching we
/// lowercase the whole address. We extract the address from a `"Name <addr>"`
/// header form and strip surrounding angle brackets/whitespace. This MUST stay in
/// lock-step with the frontend `normalizeEmail` routine in
/// `src/lib/utils/email.ts`.
pub fn normalize_email(raw: &str) -> String {
    let trimmed = raw.trim();
    let addr = match (trimmed.rfind('<'), trimmed.rfind('>')) {
        (Some(start), Some(end)) if end > start => &trimmed[start + 1..end],
        _ => trimmed,
    };
    addr.trim().trim_matches(|c| c == '<' || c == '>').trim().to_lowercase()
}

/// Determine if a sender is "new" — no prior threads from this sender exist and
/// no routing decision has been recorded yet.
///
/// Matches on a normalized email column (`sender_email`) instead of a
/// `LIKE %raw%` substring scan against the free-form `sender` header. The old
/// substring approach produced false positives (`bob@acme.com` matched
/// `bob@acme.com.evil.com`) and was vulnerable to `LIKE` wildcard broadening when
/// the address contained `%` or `_`. See SQLite docs on the LIKE operator.
pub async fn is_new_sender(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    sender_email: &str,
) -> Result<bool, String> {
    let normalized = normalize_email(sender_email);
    if normalized.is_empty() {
        return Ok(false);
    }

    // Narrow candidates with an escaped LIKE (ESCAPE neutralizes %/_ in the
    // address so a crafted From header cannot broaden the match — SQLite LIKE
    // docs), then confirm the address matches exactly after normalization. This
    // avoids the false positive where `bob@acme.com` matched `bob@acme.com.evil.com`.
    let like_pattern = format!("%{}%", escape_like(&normalized));
    let candidates: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT sender FROM threads WHERE account_id = ? AND LOWER(sender) LIKE ? ESCAPE '\\'",
    )
    .bind(account_id)
    .bind(&like_pattern)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let has_prior_thread = candidates
        .into_iter()
        .filter_map(|c| c.0)
        .any(|s| normalize_email(&s) == normalized);

    let has_routing: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sender_routing WHERE account_id = ? AND sender_email = ?"
    )
    .bind(account_id)
    .bind(&normalized)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(!has_prior_thread && has_routing.0 == 0)
}

/// Escape `%`, `_` and the escape char itself so a value can be embedded in a
/// LIKE pattern that uses `ESCAPE '\'` without the wildcards being interpreted.
fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Apply stored routing decisions to a batch of newly-synced threads.
///
/// This is the consumer that makes routing actually route: for each thread it
/// extracts the sender's normalized email, looks up the stored decision, and
/// applies it to the local label/feed state:
///   - `auto_archive` / `blocked`: remove the thread from the inbox (drop INBOX label)
///   - `feed`: register an active subscription so it appears in the Feed, and drop INBOX
///   - `inbox` (or none): leave untouched
///
/// Returns the number of threads that were routed away from the inbox.
pub async fn apply_routing_to_threads(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    thread_ids: &[String],
) -> Result<usize, String> {
    if thread_ids.is_empty() {
        return Ok(0);
    }

    let mut routed = 0usize;
    let now = chrono::Utc::now().timestamp();

    for thread_id in thread_ids {
        let row: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT sender FROM threads WHERE id = ? AND account_id = ?",
        )
        .bind(thread_id)
        .bind(account_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

        let sender = match row.and_then(|r| r.0) {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };
        let normalized = normalize_email(&sender);
        if normalized.is_empty() {
            continue;
        }

        let decision: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT routing, sender_name FROM sender_routing WHERE account_id = ? AND sender_email = ?",
        )
        .bind(account_id)
        .bind(&normalized)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

        let (routing, sender_name) = match decision {
            Some(d) => d,
            None => continue,
        };

        match Routing::parse(&routing) {
            Routing::Inbox => {}
            Routing::Feed => {
                sqlx::query(
                    "INSERT INTO subscriptions (account_id, sender_email, sender_name, detection_method, first_seen, last_seen, status)
                     VALUES (?, ?, ?, 'smart_routing', ?, ?, 'active')
                     ON CONFLICT(account_id, sender_email) DO UPDATE SET status = 'active', last_seen = excluded.last_seen",
                )
                .bind(account_id)
                .bind(&normalized)
                .bind(&sender_name)
                .bind(now)
                .bind(now)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                remove_from_inbox(pool, thread_id).await?;
                routed += 1;
            }
            Routing::AutoArchive | Routing::Blocked => {
                remove_from_inbox(pool, thread_id).await?;
                routed += 1;
            }
        }
    }

    Ok(routed)
}

async fn remove_from_inbox(pool: &sqlx::SqlitePool, thread_id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM thread_labels WHERE thread_id = ? AND label_id = 'INBOX'")
        .bind(thread_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::SqlitePool;
    use std::str::FromStr;

    async fn seed_db() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePool::connect_with(options).await.unwrap();
        sqlx::query(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, account_id TEXT, sender TEXT, subject TEXT, latest_date INTEGER)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE thread_labels (thread_id TEXT NOT NULL, label_id TEXT NOT NULL, PRIMARY KEY (thread_id, label_id))",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE sender_routing (sender_email TEXT NOT NULL, account_id TEXT NOT NULL, sender_name TEXT, routing TEXT NOT NULL DEFAULT 'inbox', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, PRIMARY KEY (sender_email, account_id))",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE subscriptions (id INTEGER PRIMARY KEY AUTOINCREMENT, account_id TEXT NOT NULL, sender_email TEXT NOT NULL, sender_name TEXT, detection_method TEXT NOT NULL, first_seen INTEGER NOT NULL, last_seen INTEGER NOT NULL, status TEXT DEFAULT 'active', UNIQUE(account_id, sender_email))",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn add_thread(pool: &SqlitePool, id: &str, sender: &str) {
        sqlx::query("INSERT INTO threads (id, account_id, sender) VALUES (?, 'acc1', ?)")
            .bind(id)
            .bind(sender)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES (?, 'INBOX')")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn set_routing(pool: &SqlitePool, email: &str, name: Option<&str>, routing: &str) {
        sqlx::query("INSERT INTO sender_routing (sender_email, account_id, sender_name, routing, created_at, updated_at) VALUES (?, 'acc1', ?, ?, 0, 0)")
            .bind(email)
            .bind(name)
            .bind(routing)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn in_inbox(pool: &SqlitePool, thread_id: &str) -> bool {
        let c: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM thread_labels WHERE thread_id = ? AND label_id = 'INBOX'",
        )
        .bind(thread_id)
        .fetch_one(pool)
        .await
        .unwrap();
        c.0 > 0
    }

    #[test]
    fn test_normalize_email_lowercases_and_extracts() {
        assert_eq!(normalize_email("Bob@Acme.COM"), "bob@acme.com");
        assert_eq!(normalize_email("Alice <Alice@Test.com>"), "alice@test.com");
        assert_eq!(normalize_email("  <x@y.com>  "), "x@y.com");
        assert_eq!(normalize_email("plain@example.com"), "plain@example.com");
        assert_eq!(normalize_email(""), "");
    }

    #[tokio::test]
    async fn test_is_new_sender_exact_match_no_false_positive() {
        let pool = seed_db().await;
        add_thread(&pool, "t1", "Evil <bob@acme.com.evil.com>").await;
        // bob@acme.com must still be considered new despite the substring overlap.
        assert!(is_new_sender(&pool, "acc1", "bob@acme.com").await.unwrap());
        // The exact stored sender is not new.
        assert!(!is_new_sender(&pool, "acc1", "bob@acme.com.evil.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_is_new_sender_case_insensitive() {
        let pool = seed_db().await;
        add_thread(&pool, "t1", "Bob <BOB@Acme.com>").await;
        assert!(!is_new_sender(&pool, "acc1", "bob@acme.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_is_new_sender_wildcard_not_injected() {
        let pool = seed_db().await;
        add_thread(&pool, "t1", "real@example.com").await;
        // A crafted address with LIKE wildcards must not match an unrelated thread.
        assert!(is_new_sender(&pool, "acc1", "%@example.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_is_new_sender_false_when_routing_exists() {
        let pool = seed_db().await;
        set_routing(&pool, "new@example.com", None, "feed").await;
        assert!(!is_new_sender(&pool, "acc1", "new@example.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_apply_routing_blocked_removes_from_inbox() {
        let pool = seed_db().await;
        add_thread(&pool, "t1", "Spam <spam@bad.com>").await;
        set_routing(&pool, "spam@bad.com", None, "blocked").await;

        let routed = apply_routing_to_threads(&pool, "acc1", &["t1".into()]).await.unwrap();
        assert_eq!(routed, 1);
        assert!(!in_inbox(&pool, "t1").await);
    }

    #[tokio::test]
    async fn test_apply_routing_auto_archive_removes_from_inbox() {
        let pool = seed_db().await;
        add_thread(&pool, "t1", "Noise <noise@example.com>").await;
        set_routing(&pool, "noise@example.com", None, "auto_archive").await;

        let routed = apply_routing_to_threads(&pool, "acc1", &["t1".into()]).await.unwrap();
        assert_eq!(routed, 1);
        assert!(!in_inbox(&pool, "t1").await);
    }

    #[tokio::test]
    async fn test_apply_routing_feed_creates_subscription_and_archives() {
        let pool = seed_db().await;
        add_thread(&pool, "t1", "News <news@example.com>").await;
        set_routing(&pool, "news@example.com", Some("News Co"), "feed").await;

        let routed = apply_routing_to_threads(&pool, "acc1", &["t1".into()]).await.unwrap();
        assert_eq!(routed, 1);
        assert!(!in_inbox(&pool, "t1").await);

        let sub: (String, String) = sqlx::query_as(
            "SELECT status, sender_name FROM subscriptions WHERE account_id = 'acc1' AND sender_email = 'news@example.com'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(sub.0, "active");
        assert_eq!(sub.1, "News Co");
    }

    #[tokio::test]
    async fn test_apply_routing_inbox_is_noop() {
        let pool = seed_db().await;
        add_thread(&pool, "t1", "Friend <friend@example.com>").await;
        set_routing(&pool, "friend@example.com", None, "inbox").await;

        let routed = apply_routing_to_threads(&pool, "acc1", &["t1".into()]).await.unwrap();
        assert_eq!(routed, 0);
        assert!(in_inbox(&pool, "t1").await);
    }

    #[tokio::test]
    async fn test_apply_routing_no_decision_leaves_inbox() {
        let pool = seed_db().await;
        add_thread(&pool, "t1", "Unknown <unknown@example.com>").await;

        let routed = apply_routing_to_threads(&pool, "acc1", &["t1".into()]).await.unwrap();
        assert_eq!(routed, 0);
        assert!(in_inbox(&pool, "t1").await);
    }

    #[test]
    fn test_routing_as_str() {
        assert_eq!(Routing::Inbox.as_str(), "inbox");
        assert_eq!(Routing::Feed.as_str(), "feed");
        assert_eq!(Routing::AutoArchive.as_str(), "auto_archive");
        assert_eq!(Routing::Blocked.as_str(), "blocked");
    }

    #[test]
    fn test_routing_parse() {
        assert_eq!(Routing::parse("inbox"), Routing::Inbox);
        assert_eq!(Routing::parse("feed"), Routing::Feed);
        assert_eq!(Routing::parse("auto_archive"), Routing::AutoArchive);
        assert_eq!(Routing::parse("blocked"), Routing::Blocked);
        assert_eq!(Routing::parse("unknown"), Routing::Inbox); // default
    }

    #[test]
    fn test_routing_roundtrip() {
        for routing in &[Routing::Inbox, Routing::Feed, Routing::AutoArchive, Routing::Blocked] {
            assert_eq!(Routing::parse(routing.as_str()), *routing);
        }
    }

    #[test]
    fn test_sender_routing_info_serialization() {
        let info = SenderRoutingInfo {
            sender_email: "test@example.com".to_string(),
            sender_name: Some("Test User".to_string()),
            routing: "feed".to_string(),
            created_at: 1000,
            updated_at: 2000,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test@example.com"));
        assert!(json.contains("feed"));
        let deserialized: SenderRoutingInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.sender_email, "test@example.com");
    }
}
