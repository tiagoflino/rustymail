# Contact Discovery from Message History — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Automatically extract contacts from email traffic, score by interaction frequency, and promote to the visible contact list once they cross a configurable threshold. Works for all providers since it reads the local messages table.

**Architecture:** Background extraction engine hooks into message storage flow. Discovered contacts live in the same `contacts` table with `source = 'discovered'` and `is_promoted` flag. Promotion is threshold-based (default 3 interactions, or instant on reciprocal exchange). Noise filter blocks service addresses.

**Tech Stack:** Rust (sqlx, serde), existing Tauri commands, existing Svelte settings UI.

---

## Task 1: Database Migration 020 — Discovery Columns and Tables

**Files:**
- Modify: `src-tauri/src/db.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_migration_020_adds_discovery_columns() {
    let pool = test_pool().await;
    apply_schema(&pool).await.unwrap();

    // Verify new columns exist on contacts
    assert!(has_column(&pool, "contacts", "email_count_sent").await);
    assert!(has_column(&pool, "contacts", "email_count_received").await);
    assert!(has_column(&pool, "contacts", "first_seen_at").await);
    assert!(has_column(&pool, "contacts", "last_contacted_at").await);
    assert!(has_column(&pool, "contacts", "is_promoted").await);

    // Verify blocklist table
    assert!(has_table(&pool, "discovery_blocklist").await);
}
```

**Step 2: Implement**

Add to `apply_schema` contacts table: `email_count_sent INTEGER NOT NULL DEFAULT 0`, `email_count_received INTEGER NOT NULL DEFAULT 0`, `first_seen_at INTEGER`, `last_contacted_at INTEGER`, `is_promoted INTEGER NOT NULL DEFAULT 1` (default 1 so existing contacts are visible).

Add table:
```sql
CREATE TABLE IF NOT EXISTS discovery_blocklist (
    email TEXT NOT NULL,
    account_id TEXT NOT NULL,
    blocked_at INTEGER NOT NULL,
    PRIMARY KEY (email, account_id)
);
```

Add settings seeds: `contact_discovery_threshold` = `3`, `contact_discovery_enabled` = `true`.

Add migration `m020_add_discovery_columns`:
- ALTER TABLE contacts ADD COLUMN email_count_sent INTEGER NOT NULL DEFAULT 0
- ALTER TABLE contacts ADD COLUMN email_count_received INTEGER NOT NULL DEFAULT 0
- ALTER TABLE contacts ADD COLUMN first_seen_at INTEGER
- ALTER TABLE contacts ADD COLUMN last_contacted_at INTEGER
- ALTER TABLE contacts ADD COLUMN is_promoted INTEGER NOT NULL DEFAULT 1
- CREATE TABLE discovery_blocklist
- INSERT settings rows

Update loop to `1..=20`.

**Step 3: Run tests, commit**

```bash
git commit -m "feat(contacts): add discovery columns and blocklist"
```

---

## Task 2: Noise Filter

**Files:**
- Create: `src-tauri/src/contacts/discovery.rs`
- Modify: `src-tauri/src/contacts/mod.rs`

**Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

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
}
```

**Step 2: Implement**

```rust
const NOISE_PREFIXES: &[&str] = &[
    "noreply", "no-reply", "no_reply", "donotreply", "do-not-reply",
    "mailer-daemon", "postmaster", "notifications", "notification",
    "alert", "alerts", "digest", "bounce", "auto", "automated",
    "system", "admin", "root", "daemon", "unsubscribe",
];

const NOISE_DOMAINS: &[&str] = &[
    "noreply.", "no-reply.", "mailer-daemon.",
];

pub fn is_noise_address(email: &str) -> bool {
    let email_lower = email.to_lowercase();
    let local = email_lower.split('@').next().unwrap_or("");

    for prefix in NOISE_PREFIXES {
        if local == *prefix || local.starts_with(&format!("{}+", prefix)) || local.starts_with(&format!("{}.", prefix)) {
            return true;
        }
    }
    false
}

pub fn is_mass_cc(recipients: &str) -> bool {
    recipients.split(',').filter(|r| r.contains('@')).count() > 5
}
```

**Step 3: Run tests, commit**

```bash
git commit -m "feat(contacts): add noise filter for discovery"
```

---

## Task 3: Core Extraction Function

**Files:**
- Modify: `src-tauri/src/contacts/discovery.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_extract_inbound_message() {
    let pool = test_pool().await;
    // account email is user@test.com
    extract_contacts_from_message(
        &pool, "acc1", "user@test.com",
        "Alice <alice@corp.com>",  // sender
        "user@test.com",           // recipients (to us)
        1700000000,
    ).await.unwrap();

    // Alice should be created as discovered, inbound
    let contact: (i64, i64) = sqlx::query_as(
        "SELECT email_count_sent, email_count_received FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'alice@corp.com')"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(contact.0, 0); // we didn't send
    assert_eq!(contact.1, 1); // we received from alice
}

#[tokio::test]
async fn test_extract_outbound_message() {
    let pool = test_pool().await;
    extract_contacts_from_message(
        &pool, "acc1", "user@test.com",
        "user@test.com",           // sender is us
        "bob@work.com",            // we sent to bob
        1700000000,
    ).await.unwrap();

    let contact: (i64, i64) = sqlx::query_as(
        "SELECT email_count_sent, email_count_received FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'bob@work.com')"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(contact.0, 1); // we sent to bob
    assert_eq!(contact.1, 0);
}

#[tokio::test]
async fn test_extract_increments_existing() {
    let pool = test_pool().await;
    extract_contacts_from_message(&pool, "acc1", "user@test.com", "alice@corp.com", "user@test.com", 1700000000).await.unwrap();
    extract_contacts_from_message(&pool, "acc1", "user@test.com", "alice@corp.com", "user@test.com", 1700001000).await.unwrap();

    let contact: (i64, i64, i64) = sqlx::query_as(
        "SELECT email_count_sent, email_count_received, last_contacted_at FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'alice@corp.com')"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(contact.0, 0);
    assert_eq!(contact.1, 2); // incremented
    assert_eq!(contact.2, 1700001000); // latest timestamp
}

#[tokio::test]
async fn test_extract_skips_noise() {
    let pool = test_pool().await;
    extract_contacts_from_message(&pool, "acc1", "user@test.com", "noreply@github.com", "user@test.com", 1700000000).await.unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM contacts WHERE account_id = 'acc1' AND source = 'discovered'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count.0, 0);
}

#[tokio::test]
async fn test_extract_skips_blocklisted() {
    let pool = test_pool().await;
    sqlx::query("INSERT INTO discovery_blocklist (email, account_id, blocked_at) VALUES ('blocked@test.com', 'acc1', 1000)")
        .execute(&pool).await.unwrap();

    extract_contacts_from_message(&pool, "acc1", "user@test.com", "blocked@test.com", "user@test.com", 1700000000).await.unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM contacts WHERE account_id = 'acc1' AND source = 'discovered'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count.0, 0);
}
```

**Step 2: Implement**

```rust
pub async fn extract_contacts_from_message(
    pool: &SqlitePool,
    account_id: &str,
    account_email: &str,
    sender: &str,
    recipients: &str,
    message_timestamp: i64,
) -> Result<(), String> {
    let account_lower = account_email.to_lowercase();

    // Parse sender
    let (sender_name, sender_email) = parse_address(sender);
    let sender_lower = sender_email.to_lowercase();

    let is_outbound = sender_lower == account_lower;

    if is_outbound {
        // We sent this — extract recipients (skip mass CC)
        if is_mass_cc(recipients) { return Ok(()); }
        for addr in parse_recipients(recipients) {
            let (name, email) = addr;
            let email_lower = email.to_lowercase();
            if email_lower == account_lower { continue; }
            if is_noise_address(&email_lower) { continue; }
            if is_blocklisted(pool, &email_lower, account_id).await { continue; }
            upsert_discovered_contact(pool, account_id, &name, &email_lower, 1, 0, message_timestamp).await?;
        }
    } else {
        // We received this — extract sender
        if is_noise_address(&sender_lower) { return Ok(()); }
        if is_blocklisted(pool, &sender_lower, account_id).await { return Ok(()); }
        upsert_discovered_contact(pool, account_id, &sender_name, &sender_lower, 0, 1, message_timestamp).await?;
    }

    Ok(())
}

async fn is_blocklisted(pool: &SqlitePool, email: &str, account_id: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM discovery_blocklist WHERE email = ? AND account_id = ?"
    ).bind(email).bind(account_id).fetch_one(pool).await.unwrap_or(0) > 0
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
    // Check if email already exists in contact store
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT ce.contact_id FROM contact_emails ce JOIN contacts c ON c.id = ce.contact_id WHERE ce.email = ? COLLATE NOCASE AND c.account_id = ?"
    ).bind(email).bind(account_id).fetch_optional(pool).await.unwrap_or(None);

    if let Some((contact_id,)) = existing {
        // Update counters
        sqlx::query(
            "UPDATE contacts SET email_count_sent = email_count_sent + ?, email_count_received = email_count_received + ?, last_contacted_at = MAX(COALESCE(last_contacted_at, 0), ?) WHERE id = ?"
        ).bind(sent_delta).bind(received_delta).bind(timestamp).bind(&contact_id)
        .execute(pool).await.map_err(|e| e.to_string())?;

        // Check promotion
        check_and_promote(pool, &contact_id, account_id).await?;
    } else {
        // Create new discovered contact
        let id = uuid::Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let name = if display_name.is_empty() { email.to_string() } else { display_name.to_string() };

        sqlx::query(
            "INSERT INTO contacts (id, account_id, display_name, phones, addresses, social_profiles, urls, relations, source, is_promoted, email_count_sent, email_count_received, first_seen_at, last_contacted_at, created_at, updated_at) VALUES (?, ?, ?, '[]', '[]', '[]', '[]', '[]', 'discovered', 0, ?, ?, ?, ?, ?, ?)"
        ).bind(&id).bind(account_id).bind(&name)
        .bind(sent_delta).bind(received_delta).bind(timestamp).bind(timestamp).bind(now).bind(now)
        .execute(pool).await.map_err(|e| e.to_string())?;

        // Insert email (ignore if somehow already exists)
        let eid = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT OR IGNORE INTO contact_emails (id, contact_id, email, type, is_primary) VALUES (?, ?, ?, 'other', 1)")
            .bind(&eid).bind(&id).bind(email)
            .execute(pool).await.ok();

        // Check promotion immediately
        check_and_promote(pool, &id, account_id).await?;
    }

    Ok(())
}
```

**Step 3: Run tests, commit**

```bash
git commit -m "feat(contacts): add contact extraction from messages"
```

---

## Task 4: Promotion Logic

**Files:**
- Modify: `src-tauri/src/contacts/discovery.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_promotion_on_reciprocal() {
    let pool = test_pool().await;
    // Receive from alice
    extract_contacts_from_message(&pool, "acc1", "user@test.com", "alice@test.org", "user@test.com", 1000).await.unwrap();
    let promoted: (i64,) = sqlx::query_as("SELECT is_promoted FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'alice@test.org')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(promoted.0, 0); // not yet

    // Send to alice (reciprocal)
    extract_contacts_from_message(&pool, "acc1", "user@test.com", "user@test.com", "alice@test.org", 2000).await.unwrap();
    let promoted: (i64,) = sqlx::query_as("SELECT is_promoted FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'alice@test.org')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(promoted.0, 1); // reciprocal = instant promotion
}

#[tokio::test]
async fn test_promotion_on_threshold() {
    let pool = test_pool().await;
    // Receive 3 emails from bob (threshold = 3)
    for i in 0..3 {
        extract_contacts_from_message(&pool, "acc1", "user@test.com", "bob@co.com", "user@test.com", 1000 + i).await.unwrap();
    }
    let promoted: (i64,) = sqlx::query_as("SELECT is_promoted FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'bob@co.com')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(promoted.0, 1); // 3 total = threshold reached
}

#[tokio::test]
async fn test_no_promotion_below_threshold() {
    let pool = test_pool().await;
    extract_contacts_from_message(&pool, "acc1", "user@test.com", "carol@x.com", "user@test.com", 1000).await.unwrap();
    extract_contacts_from_message(&pool, "acc1", "user@test.com", "carol@x.com", "user@test.com", 2000).await.unwrap();

    let promoted: (i64,) = sqlx::query_as("SELECT is_promoted FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'carol@x.com')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(promoted.0, 0); // only 2 interactions, threshold is 3
}
```

**Step 2: Implement**

```rust
async fn check_and_promote(pool: &SqlitePool, contact_id: &str, account_id: &str) -> Result<(), String> {
    let (sent, received, is_promoted): (i64, i64, i64) = sqlx::query_as(
        "SELECT email_count_sent, email_count_received, is_promoted FROM contacts WHERE id = ?"
    ).bind(contact_id).fetch_one(pool).await.map_err(|e| e.to_string())?;

    if is_promoted == 1 { return Ok(()); } // already promoted

    // Get threshold from settings
    let threshold: i64 = sqlx::query_scalar(
        "SELECT CAST(value AS INTEGER) FROM settings WHERE key = 'contact_discovery_threshold'"
    ).fetch_optional(pool).await.unwrap_or(None).unwrap_or(3);

    let should_promote = (sent >= 1 && received >= 1) || (sent + received >= threshold);

    if should_promote {
        sqlx::query("UPDATE contacts SET is_promoted = 1 WHERE id = ?")
            .bind(contact_id).execute(pool).await.map_err(|e| e.to_string())?;

        // Add to FTS on promotion
        let contact: (String, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT display_name, company, job_title, notes FROM contacts WHERE id = ?"
        ).bind(contact_id).fetch_one(pool).await.map_err(|e| e.to_string())?;

        sqlx::query(
            "INSERT INTO contacts_fts (rowid, display_name, company, job_title, notes) VALUES ((SELECT rowid FROM contacts WHERE id = ?), ?, ?, ?, ?)"
        ).bind(contact_id).bind(&contact.0).bind(&contact.1).bind(&contact.2).bind(&contact.3)
        .execute(pool).await.ok();
    }

    Ok(())
}
```

**Step 3: Run tests, commit**

```bash
git commit -m "feat(contacts): add threshold-based promotion logic"
```

---

## Task 5: Backfill Engine

**Files:**
- Modify: `src-tauri/src/contacts/discovery.rs`
- Modify: `src-tauri/src/commands/contacts.rs` (add Tauri command)

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_backfill_processes_existing_messages() {
    let pool = test_pool().await;
    // Insert some messages
    sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id) VALUES ('t1', 'acc1', '', '')").execute(&pool).await.unwrap();
    for i in 0..5 {
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_html, body_plain) VALUES (?, 't1', 'acc1', 'alice@test.com', 'user@test.com', 'Hi', '', ?, '', '')")
            .bind(format!("m{}", i)).bind(1700000000i64 + i)
            .execute(&pool).await.unwrap();
    }

    backfill_discovered_contacts(&pool, "acc1", "user@test.com").await.unwrap();

    // alice should be promoted (5 interactions > threshold 3)
    let promoted: (i64,) = sqlx::query_as("SELECT is_promoted FROM contacts WHERE id = (SELECT contact_id FROM contact_emails WHERE email = 'alice@test.com')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(promoted.0, 1);
}
```

**Step 2: Implement**

```rust
pub async fn backfill_discovered_contacts(
    pool: &SqlitePool,
    account_id: &str,
    account_email: &str,
) -> Result<usize, String> {
    // Check if already done
    let done: Option<String> = sqlx::query_scalar(
        "SELECT value FROM settings WHERE key = ?")
        .bind(format!("discovery_backfill_{}", account_id))
        .fetch_optional(pool).await.unwrap_or(None);

    if done.as_deref() == Some("true") { return Ok(0); }

    let mut processed = 0usize;
    let mut offset = 0i64;
    let batch_size = 500i64;

    loop {
        let messages: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT sender, recipients, internal_date FROM messages WHERE account_id = ? ORDER BY internal_date ASC LIMIT ? OFFSET ?"
        ).bind(account_id).bind(batch_size).bind(offset)
        .fetch_all(pool).await.map_err(|e| e.to_string())?;

        if messages.is_empty() { break; }

        for (sender, recipients, timestamp) in &messages {
            extract_contacts_from_message(pool, account_id, account_email, sender, &recipients, *timestamp).await.ok();
            processed += 1;
        }

        offset += batch_size;

        // Yield to event loop between batches
        tokio::task::yield_now().await;
    }

    // Mark complete
    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, 'true')")
        .bind(format!("discovery_backfill_{}", account_id))
        .execute(pool).await.ok();

    Ok(processed)
}
```

Add Tauri command + register in lib.rs:
```rust
#[tauri::command]
pub async fn backfill_contacts(app_handle: tauri::AppHandle, account_id: Option<String>) -> Result<usize, String> { ... }
```

**Step 3: Run tests, commit**

```bash
git commit -m "feat(contacts): add backfill engine for message history"
```

---

## Task 6: Incremental Extraction Hook

**Files:**
- Modify: `src-tauri/src/commands/sync.rs`
- Modify: `src-tauri/src/contacts/discovery.rs`

**Step 1: Implement**

After messages are stored during sync (both Gmail and Outlook), call `extract_contacts_from_message` for each newly stored message.

Find where messages are inserted in the sync flow and add:
```rust
// After storing a message:
if let Ok(enabled) = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = 'contact_discovery_enabled'")
    .fetch_optional(pool).await {
    if enabled.as_deref() != Some("false") {
        let _ = crate::contacts::discovery::extract_contacts_from_message(
            pool, account_id, &account_email, &msg.sender, &msg.recipients, msg.internal_date
        ).await;
    }
}
```

Also hook into the app launch to trigger backfill if not yet done:
```rust
// In the sync flow or on app startup:
tauri::async_runtime::spawn(async move {
    let _ = crate::contacts::discovery::backfill_discovered_contacts(&pool, &account_id, &email).await;
});
```

**Step 2: Run cargo check + tests, commit**

```bash
git commit -m "feat(contacts): hook extraction into email sync"
```

---

## Task 7: Update Contact List Filtering

**Files:**
- Modify: `src-tauri/src/commands/contacts.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_get_contacts_hides_unpromoted() {
    let pool = test_pool().await;
    // Create a promoted contact
    create_contact_inner(&pool, "acc1", CreateContactInput {
        display_name: "Visible".to_string(),
        emails: vec![EmailInput { email: "visible@test.com".into(), r#type: "work".into(), is_primary: true }],
        ..Default::default()
    }).await.unwrap();

    // Create a discovered unpromoted contact
    sqlx::query("INSERT INTO contacts (id, account_id, display_name, phones, addresses, social_profiles, urls, relations, source, is_promoted, email_count_sent, email_count_received, created_at, updated_at) VALUES ('d1', 'acc1', 'Hidden', '[]', '[]', '[]', '[]', '[]', 'discovered', 0, 1, 0, 1000, 1000)")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO contact_emails (id, contact_id, email, type, is_primary) VALUES ('de1', 'd1', 'hidden@test.com', 'other', 1)")
        .execute(&pool).await.unwrap();

    let list = get_contacts_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].contact.display_name, "Visible");
}

#[tokio::test]
async fn test_autocomplete_includes_unpromoted() {
    let pool = test_pool().await;
    // Unpromoted discovered contact
    sqlx::query("INSERT INTO contacts (id, account_id, display_name, phones, addresses, social_profiles, urls, relations, source, is_promoted, email_count_sent, email_count_received, created_at, updated_at) VALUES ('d1', 'acc1', 'Discovered Person', '[]', '[]', '[]', '[]', '[]', 'discovered', 0, 1, 0, 1000, 1000)")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO contact_emails (id, contact_id, email, type, is_primary) VALUES ('de1', 'd1', 'discovered@test.com', 'other', 1)")
        .execute(&pool).await.unwrap();

    let results = search_contacts_autocomplete(&pool, "acc1", "discovered").await.unwrap();
    assert_eq!(results.len(), 1);
}
```

**Step 2: Implement**

In `get_contacts_inner`, modify the base query to add a filter:
```sql
WHERE account_id = ? AND (is_promoted = 1 OR source != 'discovered')
```

The autocomplete function (`search_contacts_autocomplete`) should NOT filter — it searches all contacts regardless of promotion status.

**Step 3: Run tests, commit**

```bash
git commit -m "feat(contacts): filter unpromoted from list, keep in autocomplete"
```

---

## Task 8: Settings UI and Delete Blocklist

**Files:**
- Modify: `src/lib/components/Contacts.svelte` (or Settings.svelte if appropriate)
- Modify: `src-tauri/src/commands/contacts.rs`

**Step 1: Implement**

Add a command to blocklist an email (when user deletes a discovered contact):
```rust
#[tauri::command]
pub async fn blocklist_discovered_email(app_handle, email: String, account_id: Option<String>) -> Result<(), String>
```

Modify `delete_contact_inner`: if the contact being deleted has `source = 'discovered'`, automatically add its primary email to `discovery_blocklist`.

For settings: the existing settings infrastructure can handle `contact_discovery_threshold` and `contact_discovery_enabled`. Add them to the Settings UI if there's a relevant section, or leave them as advanced settings configurable via the existing `update_setting` command.

**Step 2: Run tests, commit**

```bash
git commit -m "feat(contacts): add discovery blocklist on delete"
```

---

## Task 9: Frontend — Discovery Badge and Counts

**Files:**
- Modify: `src/lib/components/Contacts.svelte`

**Step 1: Implement**

In the contact detail panel, when `selectedContact.source === 'discovered'`:
- Show a subtle "Auto-discovered" badge
- Show interaction counts: "Sent: X | Received: Y"

Parse the new fields from the contact object (they'll be in the Contact struct returned by the backend).

**Step 2: Commit**

```bash
git commit -m "feat(contacts): show discovery badge and interaction counts"
```

---

## Summary of Commits

| # | Message |
|---|---------|
| 1 | `feat(contacts): add discovery columns and blocklist` |
| 2 | `feat(contacts): add noise filter for discovery` |
| 3 | `feat(contacts): add contact extraction from messages` |
| 4 | `feat(contacts): add threshold-based promotion logic` |
| 5 | `feat(contacts): add backfill engine for message history` |
| 6 | `feat(contacts): hook extraction into email sync` |
| 7 | `feat(contacts): filter unpromoted from list, keep in autocomplete` |
| 8 | `feat(contacts): add discovery blocklist on delete` |
| 9 | `feat(contacts): show discovery badge and interaction counts` |
