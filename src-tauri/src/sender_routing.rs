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

/// Determine if a sender is "new" — no prior threads from this sender exist.
pub async fn is_new_sender(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    sender_email: &str,
) -> Result<bool, String> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM threads WHERE account_id = ? AND sender LIKE ?"
    )
    .bind(account_id)
    .bind(format!("%{}%", sender_email))
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    // Also check if there's already a routing decision
    let has_routing: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sender_routing WHERE account_id = ? AND sender_email = ?"
    )
    .bind(account_id)
    .bind(sender_email)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(count.0 == 0 && has_routing.0 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::SqlitePool;
    use std::str::FromStr;

    async fn test_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        SqlitePool::connect_with(options).await.unwrap()
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
