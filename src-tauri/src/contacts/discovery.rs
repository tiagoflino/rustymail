use sqlx::SqlitePool;

const NOISE_PREFIXES: &[&str] = &[
    "noreply",
    "no-reply",
    "no_reply",
    "donotreply",
    "do-not-reply",
    "mailer-daemon",
    "postmaster",
    "notifications",
    "notification",
    "alert",
    "alerts",
    "digest",
    "bounce",
    "auto",
    "automated",
    "system",
    "admin",
    "root",
    "daemon",
    "unsubscribe",
];

pub fn is_noise_address(email: &str) -> bool {
    let email_lower = email.to_lowercase();
    let local = email_lower.split('@').next().unwrap_or("");

    for prefix in NOISE_PREFIXES {
        if local == *prefix
            || local.starts_with(&format!("{}+", prefix))
            || local.starts_with(&format!("{}.", prefix))
        {
            return true;
        }
    }
    false
}

pub fn is_mass_cc(recipients: &str) -> bool {
    recipients.split(',').filter(|r| r.contains('@')).count() > 5
}

fn parse_address(raw: &str) -> (String, String) {
    let raw = raw.trim();
    if let Some(start) = raw.find('<') {
        if let Some(end) = raw.find('>') {
            let email = raw[start + 1..end].trim().to_string();
            let name = raw[..start].trim().trim_matches('"').trim().to_string();
            return (name, email);
        }
    }
    // Plain email address
    (String::new(), raw.to_string())
}

fn parse_recipients(recipients: &str) -> Vec<(String, String)> {
    recipients
        .split(',')
        .map(|r| parse_address(r))
        .filter(|(_, email)| email.contains('@'))
        .collect()
}

pub async fn extract_contacts_from_message(
    pool: &SqlitePool,
    account_id: &str,
    account_email: &str,
    sender: &str,
    recipients: &str,
    message_timestamp: i64,
) -> Result<(), String> {
    let account_lower = account_email.to_lowercase();

    let (sender_name, sender_email) = parse_address(sender);
    let sender_lower = sender_email.to_lowercase();

    let is_outbound = sender_lower == account_lower;

    if is_outbound {
        if is_mass_cc(recipients) {
            return Ok(());
        }
        for (name, email) in parse_recipients(recipients) {
            let email_lower = email.to_lowercase();
            if email_lower == account_lower {
                continue;
            }
            if is_noise_address(&email_lower) {
                continue;
            }
            if is_blocklisted(pool, &email_lower, account_id).await {
                continue;
            }
            upsert_discovered_contact(pool, account_id, &name, &email_lower, 1, 0, message_timestamp)
                .await?;
        }
    } else {
        if is_noise_address(&sender_lower) {
            return Ok(());
        }
        if is_blocklisted(pool, &sender_lower, account_id).await {
            return Ok(());
        }
        upsert_discovered_contact(
            pool,
            account_id,
            &sender_name,
            &sender_lower,
            0,
            1,
            message_timestamp,
        )
        .await?;
    }

    Ok(())
}

async fn is_blocklisted(pool: &SqlitePool, email: &str, account_id: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM discovery_blocklist WHERE email = ? AND account_id = ?",
    )
    .bind(email)
    .bind(account_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
        > 0
}

async fn upsert_discovered_contact(
    pool: &SqlitePool,
    account_id: &str,
    display_name: &str,
    email: &str,
    sent_delta: i64,
    received_delta: i64,
    timestamp: i64,
) -> Result<(), String> {
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT ce.contact_id FROM contact_emails ce JOIN contacts c ON c.id = ce.contact_id WHERE ce.email = ? COLLATE NOCASE AND c.account_id = ?",
    )
    .bind(email)
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    if let Some((contact_id,)) = existing {
        sqlx::query(
            "UPDATE contacts SET email_count_sent = email_count_sent + ?, email_count_received = email_count_received + ?, last_contacted_at = MAX(COALESCE(last_contacted_at, 0), ?) WHERE id = ?",
        )
        .bind(sent_delta)
        .bind(received_delta)
        .bind(timestamp)
        .bind(&contact_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

        check_and_promote(pool, &contact_id).await?;
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let name = if display_name.is_empty() {
            email.to_string()
        } else {
            display_name.to_string()
        };

        sqlx::query(
            "INSERT INTO contacts (id, account_id, display_name, phones, addresses, social_profiles, urls, relations, source, is_promoted, email_count_sent, email_count_received, first_seen_at, last_contacted_at, created_at, updated_at) VALUES (?, ?, ?, '[]', '[]', '[]', '[]', '[]', 'discovered', 0, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(account_id)
        .bind(&name)
        .bind(sent_delta)
        .bind(received_delta)
        .bind(timestamp)
        .bind(timestamp)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

        let eid = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT OR IGNORE INTO contact_emails (id, contact_id, email, type, is_primary) VALUES (?, ?, ?, 'other', 1)",
        )
        .bind(&eid)
        .bind(&id)
        .bind(email)
        .execute(pool)
        .await
        .ok();

        check_and_promote(pool, &id).await?;
    }

    Ok(())
}

async fn check_and_promote(pool: &SqlitePool, contact_id: &str) -> Result<(), String> {
    let (sent, received, is_promoted): (i64, i64, i64) = sqlx::query_as(
        "SELECT email_count_sent, email_count_received, is_promoted FROM contacts WHERE id = ?",
    )
    .bind(contact_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    if is_promoted == 1 {
        return Ok(());
    }

    let threshold: i64 = sqlx::query_scalar(
        "SELECT CAST(value AS INTEGER) FROM settings WHERE key = 'contact_discovery_threshold'",
    )
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .unwrap_or(3);

    let should_promote = (sent >= 1 && received >= 1) || (sent + received >= threshold);

    if should_promote {
        sqlx::query("UPDATE contacts SET is_promoted = 1 WHERE id = ?")
            .bind(contact_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        // Add to FTS on promotion
        let contact: (String, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT display_name, company, job_title, notes FROM contacts WHERE id = ?",
        )
        .bind(contact_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

        sqlx::query(
            "INSERT INTO contacts_fts (rowid, display_name, company, job_title, notes) VALUES ((SELECT rowid FROM contacts WHERE id = ?), ?, ?, ?, ?)",
        )
        .bind(contact_id)
        .bind(&contact.0)
        .bind(&contact.1)
        .bind(&contact.2)
        .bind(&contact.3)
        .execute(pool)
        .await
        .ok();
    }

    Ok(())
}

pub async fn backfill_discovered_contacts(
    pool: &SqlitePool,
    account_id: &str,
    account_email: &str,
) -> Result<usize, String> {
    // Check if already done
    let key = format!("discovery_backfill_{}", account_id);
    let done: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(&key)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

    if done.as_deref() == Some("true") {
        return Ok(0);
    }

    // Check if discovery is enabled
    let enabled: Option<String> = sqlx::query_scalar(
        "SELECT value FROM settings WHERE key = 'contact_discovery_enabled'",
    )
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    if enabled.as_deref() == Some("false") {
        return Ok(0);
    }

    let mut processed = 0usize;
    let mut offset = 0i64;
    let batch_size = 500i64;

    loop {
        let messages: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT sender, recipients, internal_date FROM messages WHERE account_id = ? ORDER BY internal_date ASC LIMIT ? OFFSET ?",
        )
        .bind(account_id)
        .bind(batch_size)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        if messages.is_empty() {
            break;
        }

        for (sender, recipients, timestamp) in &messages {
            extract_contacts_from_message(pool, account_id, account_email, sender, recipients, *timestamp)
                .await
                .ok();
            processed += 1;
        }

        offset += batch_size;
        tokio::task::yield_now().await;
    }

    // Mark complete
    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, 'true')")
        .bind(&key)
        .execute(pool)
        .await
        .ok();

    Ok(processed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;
    use std::str::FromStr;

    async fn test_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        crate::db::apply_schema(&pool).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, email, display_name, provider_type) VALUES ('acc1', 'user@test.com', 'Test User', 'gmail')")
            .execute(&pool).await.unwrap();
        pool
    }

    #[test]
    fn test_noise_filter_blocks_noreply() {
        assert!(is_noise_address("noreply@github.com"));
        assert!(is_noise_address("no-reply@accounts.google.com"));
        assert!(is_noise_address("mailer-daemon@server.com"));
        assert!(is_noise_address("notifications@linkedin.com"));
    }

    #[test]
    fn test_noise_filter_allows_real_addresses() {
        assert!(!is_noise_address("john@acme.com"));
        assert!(!is_noise_address("jane.doe@company.org"));
        assert!(!is_noise_address("support@smallbiz.com"));
    }

    #[test]
    fn test_mass_cc_detection() {
        let recipients = "a@x.com,b@x.com,c@x.com,d@x.com,e@x.com,f@x.com";
        assert!(is_mass_cc(recipients));

        let few = "a@x.com,b@x.com";
        assert!(!is_mass_cc(few));
    }

    #[tokio::test]
    async fn test_extract_inbound_message() {
        let pool = test_pool().await;
        extract_contacts_from_message(
            &pool,
            "acc1",
            "user@test.com",
            "Alice <alice@corp.com>",
            "user@test.com",
            1700000000,
        )
        .await
        .unwrap();

        let contact: (i64, i64) = sqlx::query_as(
            "SELECT email_count_sent, email_count_received FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'alice@corp.com')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(contact.0, 0);
        assert_eq!(contact.1, 1);
    }

    #[tokio::test]
    async fn test_extract_outbound_message() {
        let pool = test_pool().await;
        extract_contacts_from_message(
            &pool,
            "acc1",
            "user@test.com",
            "user@test.com",
            "bob@work.com",
            1700000000,
        )
        .await
        .unwrap();

        let contact: (i64, i64) = sqlx::query_as(
            "SELECT email_count_sent, email_count_received FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'bob@work.com')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(contact.0, 1);
        assert_eq!(contact.1, 0);
    }

    #[tokio::test]
    async fn test_extract_increments_existing() {
        let pool = test_pool().await;
        extract_contacts_from_message(
            &pool,
            "acc1",
            "user@test.com",
            "alice@corp.com",
            "user@test.com",
            1700000000,
        )
        .await
        .unwrap();
        extract_contacts_from_message(
            &pool,
            "acc1",
            "user@test.com",
            "alice@corp.com",
            "user@test.com",
            1700001000,
        )
        .await
        .unwrap();

        let contact: (i64, i64, i64) = sqlx::query_as(
            "SELECT email_count_sent, email_count_received, last_contacted_at FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'alice@corp.com')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(contact.0, 0);
        assert_eq!(contact.1, 2);
        assert_eq!(contact.2, 1700001000);
    }

    #[tokio::test]
    async fn test_extract_skips_noise() {
        let pool = test_pool().await;
        extract_contacts_from_message(
            &pool,
            "acc1",
            "user@test.com",
            "noreply@github.com",
            "user@test.com",
            1700000000,
        )
        .await
        .unwrap();

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM contacts WHERE account_id = 'acc1' AND source = 'discovered'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn test_extract_skips_blocklisted() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO discovery_blocklist (email, account_id, blocked_at) VALUES ('blocked@test.com', 'acc1', 1000)",
        )
        .execute(&pool)
        .await
        .unwrap();

        extract_contacts_from_message(
            &pool,
            "acc1",
            "user@test.com",
            "blocked@test.com",
            "user@test.com",
            1700000000,
        )
        .await
        .unwrap();

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM contacts WHERE account_id = 'acc1' AND source = 'discovered'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn test_promotion_on_reciprocal() {
        let pool = test_pool().await;
        // Receive from alice
        extract_contacts_from_message(
            &pool,
            "acc1",
            "user@test.com",
            "alice@test.org",
            "user@test.com",
            1000,
        )
        .await
        .unwrap();
        let promoted: (i64,) = sqlx::query_as(
            "SELECT is_promoted FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'alice@test.org')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(promoted.0, 0);

        // Send to alice (reciprocal)
        extract_contacts_from_message(
            &pool,
            "acc1",
            "user@test.com",
            "user@test.com",
            "alice@test.org",
            2000,
        )
        .await
        .unwrap();
        let promoted: (i64,) = sqlx::query_as(
            "SELECT is_promoted FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'alice@test.org')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(promoted.0, 1);
    }

    #[tokio::test]
    async fn test_promotion_on_threshold() {
        let pool = test_pool().await;
        // Receive 3 emails from bob (threshold = 3)
        for i in 0..3 {
            extract_contacts_from_message(
                &pool,
                "acc1",
                "user@test.com",
                "bob@co.com",
                "user@test.com",
                1000 + i,
            )
            .await
            .unwrap();
        }
        let promoted: (i64,) = sqlx::query_as(
            "SELECT is_promoted FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'bob@co.com')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(promoted.0, 1);
    }

    #[tokio::test]
    async fn test_no_promotion_below_threshold() {
        let pool = test_pool().await;
        extract_contacts_from_message(
            &pool,
            "acc1",
            "user@test.com",
            "carol@x.com",
            "user@test.com",
            1000,
        )
        .await
        .unwrap();
        extract_contacts_from_message(
            &pool,
            "acc1",
            "user@test.com",
            "carol@x.com",
            "user@test.com",
            2000,
        )
        .await
        .unwrap();

        let promoted: (i64,) = sqlx::query_as(
            "SELECT is_promoted FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'carol@x.com')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(promoted.0, 0);
    }

    #[tokio::test]
    async fn test_backfill_processes_existing_messages() {
        let pool = test_pool().await;
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id) VALUES ('t1', 'acc1', '', '')")
            .execute(&pool)
            .await
            .unwrap();
        for i in 0..5i64 {
            sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_html, body_plain) VALUES (?, 't1', 'acc1', 'alice@test.com', 'user@test.com', 'Hi', '', ?, '', '')")
                .bind(format!("m{}", i))
                .bind(1700000000 + i)
                .execute(&pool)
                .await
                .unwrap();
        }

        let count = backfill_discovered_contacts(&pool, "acc1", "user@test.com")
            .await
            .unwrap();
        assert_eq!(count, 5);

        // alice should be promoted (5 > threshold 3)
        let promoted: (i64,) = sqlx::query_as(
            "SELECT is_promoted FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'alice@test.com')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(promoted.0, 1);

        // Running again should be a no-op
        let count2 = backfill_discovered_contacts(&pool, "acc1", "user@test.com")
            .await
            .unwrap();
        assert_eq!(count2, 0);
    }
}
