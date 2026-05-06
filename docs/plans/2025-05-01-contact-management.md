# Contact Management System — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement a unified contact management system with SQLite storage, multi-provider sync (Google People API, Microsoft Graph, CardDAV), full CRUD UI, hover cards, groups, and import/export.

**Architecture:** Runtime-dispatch provider modules (matching existing pattern). Normalized `contact_emails` table for fast lookups, JSON columns for display-only fields. FTS5 for full-text search. Contact sync piggybacks on email sync with 15-min throttle.

**Tech Stack:** Rust (sqlx, reqwest, serde, uuid), Svelte 5 (runes: $state, $derived, $effect, $props), SQLite with FTS5, Tauri 2 commands.

---

## Task 1: Database Migration — Contact Tables

**Files:**
- Modify: `src-tauri/src/db.rs`

**Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` block at the bottom of `db.rs`:

```rust
#[tokio::test]
async fn test_contacts_tables_exist_after_schema() {
    let pool = test_pool().await;
    apply_schema(&pool).await.unwrap();

    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'contact%' ORDER BY name"
    ).fetch_all(&pool).await.unwrap();

    let names: Vec<&str> = tables.iter().map(|r| r.0.as_str()).collect();
    for expected in &[
        "contact_emails",
        "contact_group_members",
        "contact_groups",
        "contact_provider_links",
        "contacts",
        "contacts_sync_state",
    ] {
        assert!(names.contains(expected), "Missing table: {expected}");
    }
}

#[tokio::test]
async fn test_contacts_fts_exists_after_schema() {
    let pool = test_pool().await;
    apply_schema(&pool).await.unwrap();

    let result: Result<Vec<(String,)>, _> = sqlx::query_as(
        "SELECT display_name FROM contacts_fts WHERE contacts_fts MATCH 'test' LIMIT 1"
    ).fetch_all(&pool).await;

    assert!(result.is_ok(), "contacts_fts table should exist and be queryable");
}

#[tokio::test]
async fn test_contact_emails_unique_index() {
    let pool = test_pool().await;
    apply_schema(&pool).await.unwrap();

    sqlx::query("INSERT INTO contacts (id, account_id, display_name, phones, addresses, social_profiles, urls, relations, source, created_at, updated_at) VALUES ('c1', 'a1', 'Test', '[]', '[]', '[]', '[]', '[]', 'local', 1000, 1000)")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO contact_emails (id, contact_id, email, type, is_primary) VALUES ('e1', 'c1', 'test@example.com', 'work', 1)")
        .execute(&pool).await.unwrap();

    let dup = sqlx::query("INSERT INTO contact_emails (id, contact_id, email, type, is_primary) VALUES ('e2', 'c1', 'test@example.com', 'personal', 0)")
        .execute(&pool).await;

    assert!(dup.is_err(), "Duplicate email should be rejected by unique index");
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test test_contacts_tables_exist_after_schema -- --nocapture`
Expected: FAIL — tables don't exist yet

**Step 3: Write the implementation**

Add to `apply_schema` function (after existing CREATE TABLE statements, before the closing `"#;`):

```rust
    CREATE TABLE IF NOT EXISTS contacts (
        id TEXT PRIMARY KEY,
        account_id TEXT NOT NULL,
        display_name TEXT NOT NULL,
        given_name TEXT,
        surname TEXT,
        nickname TEXT,
        company TEXT,
        job_title TEXT,
        department TEXT,
        notes TEXT,
        birthday TEXT,
        photo_uri TEXT,
        phones TEXT NOT NULL DEFAULT '[]',
        addresses TEXT NOT NULL DEFAULT '[]',
        social_profiles TEXT NOT NULL DEFAULT '[]',
        urls TEXT NOT NULL DEFAULT '[]',
        relations TEXT NOT NULL DEFAULT '[]',
        is_starred INTEGER NOT NULL DEFAULT 0,
        source TEXT NOT NULL DEFAULT 'local',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        FOREIGN KEY (account_id) REFERENCES accounts(id)
    );

    CREATE TABLE IF NOT EXISTS contact_emails (
        id TEXT PRIMARY KEY,
        contact_id TEXT NOT NULL,
        email TEXT NOT NULL,
        type TEXT NOT NULL DEFAULT 'other',
        is_primary INTEGER NOT NULL DEFAULT 0,
        FOREIGN KEY (contact_id) REFERENCES contacts(id) ON DELETE CASCADE
    );

    CREATE UNIQUE INDEX IF NOT EXISTS idx_contact_emails_email
        ON contact_emails(email COLLATE NOCASE);
    CREATE INDEX IF NOT EXISTS idx_contact_emails_contact
        ON contact_emails(contact_id);
    CREATE INDEX IF NOT EXISTS idx_contacts_account
        ON contacts(account_id);

    CREATE TABLE IF NOT EXISTS contact_provider_links (
        id TEXT PRIMARY KEY,
        contact_id TEXT NOT NULL,
        account_id TEXT NOT NULL,
        provider TEXT NOT NULL,
        remote_id TEXT NOT NULL,
        etag TEXT,
        last_synced_at INTEGER,
        FOREIGN KEY (contact_id) REFERENCES contacts(id) ON DELETE CASCADE,
        UNIQUE(account_id, provider, remote_id)
    );

    CREATE TABLE IF NOT EXISTS contact_groups (
        id TEXT PRIMARY KEY,
        account_id TEXT NOT NULL,
        name TEXT NOT NULL,
        color TEXT,
        remote_id TEXT,
        created_at INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS contact_group_members (
        contact_id TEXT NOT NULL,
        group_id TEXT NOT NULL,
        PRIMARY KEY (contact_id, group_id),
        FOREIGN KEY (contact_id) REFERENCES contacts(id) ON DELETE CASCADE,
        FOREIGN KEY (group_id) REFERENCES contact_groups(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS contacts_sync_state (
        account_id TEXT NOT NULL,
        provider TEXT NOT NULL,
        sync_token TEXT,
        last_full_sync INTEGER,
        PRIMARY KEY (account_id, provider)
    );
```

Then after the schema string execution, add the FTS5 table creation (FTS5 uses a separate statement since it can't be inside the multi-statement string with IF NOT EXISTS reliably):

```rust
// After the main schema execution, before the settings seed:
sqlx::query(
    "CREATE VIRTUAL TABLE IF NOT EXISTS contacts_fts USING fts5(display_name, company, job_title, notes, content='contacts', content_rowid='rowid')"
).execute(pool).await?;
```

**Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test test_contacts -- --nocapture`
Expected: All 3 new tests PASS

**Step 5: Commit**

```bash
git add src-tauri/src/db.rs
git commit -m "feat(contacts): add database schema for contact management"
```

---

## Task 2: Migration for Existing Databases

**Files:**
- Modify: `src-tauri/src/db.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_migration_018_creates_contact_tables() {
    let pool = test_pool().await;
    // Apply base schema WITHOUT contacts (simulating old DB)
    sqlx::query("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY)")
        .execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE IF NOT EXISTS accounts (id TEXT PRIMARY KEY, email TEXT, display_name TEXT, avatar_url TEXT, token_expiry INTEGER, is_active INTEGER DEFAULT 1, created_at INTEGER, credential_source TEXT DEFAULT 'builtin', provider_type TEXT DEFAULT 'gmail')")
        .execute(&pool).await.unwrap();
    // Mark migrations 1-17 as applied
    for v in 1..=17i64 {
        sqlx::query("INSERT INTO schema_migrations (version) VALUES (?)")
            .bind(v).execute(&pool).await.unwrap();
    }

    run_pending_migrations(&pool, &(1..=17).collect::<Vec<_>>()).await.unwrap();
    // 17 was already applied, so nothing should happen yet

    // Now run with 18 included
    let applied: Vec<i64> = sqlx::query_scalar("SELECT version FROM schema_migrations")
        .fetch_all(&pool).await.unwrap();
    run_pending_migrations(&pool, &applied).await.unwrap();

    let has = has_table(&pool, "contacts").await;
    assert!(has, "contacts table should exist after migration 18");
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test test_migration_018 -- --nocapture`
Expected: FAIL — migration 18 doesn't exist yet

**Step 3: Write the implementation**

Add migration function:

```rust
async fn m018_create_contacts(pool: &SqlitePool) -> Result<()> {
    if !has_table(pool, "contacts").await {
        sqlx::query(
            "CREATE TABLE contacts (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                display_name TEXT NOT NULL,
                given_name TEXT,
                surname TEXT,
                nickname TEXT,
                company TEXT,
                job_title TEXT,
                department TEXT,
                notes TEXT,
                birthday TEXT,
                photo_uri TEXT,
                phones TEXT NOT NULL DEFAULT '[]',
                addresses TEXT NOT NULL DEFAULT '[]',
                social_profiles TEXT NOT NULL DEFAULT '[]',
                urls TEXT NOT NULL DEFAULT '[]',
                relations TEXT NOT NULL DEFAULT '[]',
                is_starred INTEGER NOT NULL DEFAULT 0,
                source TEXT NOT NULL DEFAULT 'local',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY (account_id) REFERENCES accounts(id)
            )"
        ).execute(pool).await?;

        sqlx::query("CREATE INDEX idx_contacts_account ON contacts(account_id)")
            .execute(pool).await?;
    }

    if !has_table(pool, "contact_emails").await {
        sqlx::query(
            "CREATE TABLE contact_emails (
                id TEXT PRIMARY KEY,
                contact_id TEXT NOT NULL,
                email TEXT NOT NULL,
                type TEXT NOT NULL DEFAULT 'other',
                is_primary INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (contact_id) REFERENCES contacts(id) ON DELETE CASCADE
            )"
        ).execute(pool).await?;

        sqlx::query("CREATE UNIQUE INDEX idx_contact_emails_email ON contact_emails(email COLLATE NOCASE)")
            .execute(pool).await?;
        sqlx::query("CREATE INDEX idx_contact_emails_contact ON contact_emails(contact_id)")
            .execute(pool).await?;
    }

    if !has_table(pool, "contact_provider_links").await {
        sqlx::query(
            "CREATE TABLE contact_provider_links (
                id TEXT PRIMARY KEY,
                contact_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                remote_id TEXT NOT NULL,
                etag TEXT,
                last_synced_at INTEGER,
                FOREIGN KEY (contact_id) REFERENCES contacts(id) ON DELETE CASCADE,
                UNIQUE(account_id, provider, remote_id)
            )"
        ).execute(pool).await?;
    }

    if !has_table(pool, "contact_groups").await {
        sqlx::query(
            "CREATE TABLE contact_groups (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                name TEXT NOT NULL,
                color TEXT,
                remote_id TEXT,
                created_at INTEGER NOT NULL
            )"
        ).execute(pool).await?;
    }

    if !has_table(pool, "contact_group_members").await {
        sqlx::query(
            "CREATE TABLE contact_group_members (
                contact_id TEXT NOT NULL,
                group_id TEXT NOT NULL,
                PRIMARY KEY (contact_id, group_id),
                FOREIGN KEY (contact_id) REFERENCES contacts(id) ON DELETE CASCADE,
                FOREIGN KEY (group_id) REFERENCES contact_groups(id) ON DELETE CASCADE
            )"
        ).execute(pool).await?;
    }

    if !has_table(pool, "contacts_sync_state").await {
        sqlx::query(
            "CREATE TABLE contacts_sync_state (
                account_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                sync_token TEXT,
                last_full_sync INTEGER,
                PRIMARY KEY (account_id, provider)
            )"
        ).execute(pool).await?;
    }

    // FTS5
    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS contacts_fts USING fts5(display_name, company, job_title, notes, content='contacts', content_rowid='rowid')"
    ).execute(pool).await?;

    Ok(())
}
```

Update `run_pending_migrations`:
- Change `for version in 1..=17i64` → `for version in 1..=18i64`
- Add: `18 => m018_create_contacts(pool).await?,`

**Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test test_migration_018 -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/db.rs
git commit -m "feat(contacts): add migration 018 for existing databases"
```

---

## Task 3: Contact CRUD Commands — Types and Create

**Files:**
- Create: `src-tauri/src/commands/contacts.rs`
- Modify: `src-tauri/src/commands/mod.rs`

**Step 1: Write the failing test**

In `src-tauri/src/commands/contacts.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
    use std::str::FromStr;

    async fn test_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        crate::db::apply_schema(&pool).await.unwrap();
        // Seed an account
        sqlx::query("INSERT INTO accounts (id, email, display_name, provider_type) VALUES ('acc1', 'user@test.com', 'Test User', 'gmail')")
            .execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn test_create_contact_basic() {
        let pool = test_pool().await;
        let input = CreateContactInput {
            display_name: "John Doe".to_string(),
            given_name: Some("John".to_string()),
            surname: Some("Doe".to_string()),
            nickname: None,
            company: Some("Acme Corp".to_string()),
            job_title: Some("Engineer".to_string()),
            department: None,
            notes: None,
            birthday: None,
            photo_uri: None,
            phones: vec![],
            addresses: vec![],
            social_profiles: vec![],
            urls: vec![],
            relations: vec![],
            emails: vec![
                EmailInput { email: "john@acme.com".to_string(), r#type: "work".to_string(), is_primary: true },
                EmailInput { email: "john.doe@gmail.com".to_string(), r#type: "personal".to_string(), is_primary: false },
            ],
            groups: vec![],
        };

        let contact = create_contact_inner(&pool, "acc1", input).await.unwrap();

        assert_eq!(contact.contact.display_name, "John Doe");
        assert_eq!(contact.contact.company, Some("Acme Corp".to_string()));
        assert_eq!(contact.emails.len(), 2);
        assert_eq!(contact.emails[0].email, "john@acme.com");
        assert!(contact.emails[0].is_primary);
    }

    #[tokio::test]
    async fn test_create_contact_duplicate_email_rejected() {
        let pool = test_pool().await;
        let input1 = CreateContactInput {
            display_name: "John".to_string(),
            emails: vec![EmailInput { email: "john@acme.com".to_string(), r#type: "work".to_string(), is_primary: true }],
            ..Default::default()
        };
        create_contact_inner(&pool, "acc1", input1).await.unwrap();

        let input2 = CreateContactInput {
            display_name: "Johnny".to_string(),
            emails: vec![EmailInput { email: "john@acme.com".to_string(), r#type: "work".to_string(), is_primary: true }],
            ..Default::default()
        };
        let result = create_contact_inner(&pool, "acc1", input2).await;
        assert!(result.is_err(), "Should reject duplicate email");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test commands::contacts::tests -- --nocapture`
Expected: FAIL — module and types don't exist

**Step 3: Write the implementation**

Create `src-tauri/src/commands/contacts.rs`:

```rust
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_epoch() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

// --- Types ---

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Contact {
    pub id: String,
    pub account_id: String,
    pub display_name: String,
    pub given_name: Option<String>,
    pub surname: Option<String>,
    pub nickname: Option<String>,
    pub company: Option<String>,
    pub job_title: Option<String>,
    pub department: Option<String>,
    pub notes: Option<String>,
    pub birthday: Option<String>,
    pub photo_uri: Option<String>,
    pub phones: String,
    pub addresses: String,
    pub social_profiles: String,
    pub urls: String,
    pub relations: String,
    pub is_starred: bool,
    pub source: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactWithEmails {
    #[serde(flatten)]
    pub contact: Contact,
    pub emails: Vec<ContactEmail>,
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ContactEmail {
    pub id: String,
    pub contact_id: String,
    pub email: String,
    pub r#type: String,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateContactInput {
    pub display_name: String,
    pub given_name: Option<String>,
    pub surname: Option<String>,
    pub nickname: Option<String>,
    pub company: Option<String>,
    pub job_title: Option<String>,
    pub department: Option<String>,
    pub notes: Option<String>,
    pub birthday: Option<String>,
    pub photo_uri: Option<String>,
    pub phones: Vec<serde_json::Value>,
    pub addresses: Vec<serde_json::Value>,
    pub social_profiles: Vec<serde_json::Value>,
    pub urls: Vec<serde_json::Value>,
    pub relations: Vec<serde_json::Value>,
    pub emails: Vec<EmailInput>,
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmailInput {
    pub email: String,
    pub r#type: String,
    pub is_primary: bool,
}

// --- Inner functions ---

pub(crate) async fn create_contact_inner(
    pool: &SqlitePool,
    account_id: &str,
    input: CreateContactInput,
) -> Result<ContactWithEmails, String> {
    let id = new_id();
    let now = now_epoch();

    let phones_json = serde_json::to_string(&input.phones).unwrap_or_else(|_| "[]".to_string());
    let addresses_json = serde_json::to_string(&input.addresses).unwrap_or_else(|_| "[]".to_string());
    let social_json = serde_json::to_string(&input.social_profiles).unwrap_or_else(|_| "[]".to_string());
    let urls_json = serde_json::to_string(&input.urls).unwrap_or_else(|_| "[]".to_string());
    let relations_json = serde_json::to_string(&input.relations).unwrap_or_else(|_| "[]".to_string());

    sqlx::query(
        "INSERT INTO contacts (id, account_id, display_name, given_name, surname, nickname, company, job_title, department, notes, birthday, photo_uri, phones, addresses, social_profiles, urls, relations, source, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'local', ?, ?)"
    )
    .bind(&id).bind(account_id).bind(&input.display_name)
    .bind(&input.given_name).bind(&input.surname).bind(&input.nickname)
    .bind(&input.company).bind(&input.job_title).bind(&input.department)
    .bind(&input.notes).bind(&input.birthday).bind(&input.photo_uri)
    .bind(&phones_json).bind(&addresses_json).bind(&social_json)
    .bind(&urls_json).bind(&relations_json)
    .bind(now).bind(now)
    .execute(pool).await.map_err(|e| e.to_string())?;

    // Insert emails
    let mut emails = Vec::new();
    for e_input in &input.emails {
        let eid = new_id();
        sqlx::query(
            "INSERT INTO contact_emails (id, contact_id, email, type, is_primary) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&eid).bind(&id).bind(&e_input.email).bind(&e_input.r#type).bind(e_input.is_primary)
        .execute(pool).await.map_err(|e| format!("Email '{}' already exists for another contact", e_input.email))?;

        emails.push(ContactEmail {
            id: eid,
            contact_id: id.clone(),
            email: e_input.email.clone(),
            r#type: e_input.r#type.clone(),
            is_primary: e_input.is_primary,
        });
    }

    // Insert group memberships
    for group_id in &input.groups {
        sqlx::query("INSERT OR IGNORE INTO contact_group_members (contact_id, group_id) VALUES (?, ?)")
            .bind(&id).bind(group_id)
            .execute(pool).await.map_err(|e| e.to_string())?;
    }

    // Update FTS
    sqlx::query(
        "INSERT INTO contacts_fts (rowid, display_name, company, job_title, notes) VALUES ((SELECT rowid FROM contacts WHERE id = ?), ?, ?, ?, ?)"
    )
    .bind(&id).bind(&input.display_name).bind(&input.company).bind(&input.job_title).bind(&input.notes)
    .execute(pool).await.map_err(|e| e.to_string())?;

    let contact = Contact {
        id,
        account_id: account_id.to_string(),
        display_name: input.display_name,
        given_name: input.given_name,
        surname: input.surname,
        nickname: input.nickname,
        company: input.company,
        job_title: input.job_title,
        department: input.department,
        notes: input.notes,
        birthday: input.birthday,
        photo_uri: input.photo_uri,
        phones: phones_json,
        addresses: addresses_json,
        social_profiles: social_json,
        urls: urls_json,
        relations: relations_json,
        is_starred: false,
        source: "local".to_string(),
        created_at: now,
        updated_at: now,
    };

    Ok(ContactWithEmails { contact, emails, groups: input.groups })
}

// --- Tauri Commands ---

#[tauri::command]
pub async fn create_contact(
    app_handle: tauri::AppHandle,
    input: CreateContactInput,
    account_id: Option<String>,
) -> Result<ContactWithEmails, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let acc_id = match account_id {
        Some(id) => id,
        None => {
            let account = super::accounts::get_active_account(pool.inner()).await?;
            account.id
        }
    };
    create_contact_inner(pool.inner(), &acc_id, input).await
}
```

Add to `src-tauri/src/commands/mod.rs`:

```rust
pub mod contacts;
```

Add `uuid` dependency to `src-tauri/Cargo.toml`:

```toml
uuid = { version = "1", features = ["v4"] }
```

**Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test commands::contacts::tests -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/commands/contacts.rs src-tauri/src/commands/mod.rs src-tauri/Cargo.toml
git commit -m "feat(contacts): add contact types and create command"
```

---

## Task 4: Contact CRUD — Get, List, Update, Delete

**Files:**
- Modify: `src-tauri/src/commands/contacts.rs`

**Step 1: Write the failing tests**

Add to the test module:

```rust
#[tokio::test]
async fn test_get_contact_by_id() {
    let pool = test_pool().await;
    let input = CreateContactInput {
        display_name: "Jane Smith".to_string(),
        company: Some("BigCo".to_string()),
        emails: vec![EmailInput { email: "jane@bigco.com".to_string(), r#type: "work".to_string(), is_primary: true }],
        ..Default::default()
    };
    let created = create_contact_inner(&pool, "acc1", input).await.unwrap();

    let fetched = get_contact_inner(&pool, &created.contact.id).await.unwrap();
    assert_eq!(fetched.contact.display_name, "Jane Smith");
    assert_eq!(fetched.emails.len(), 1);
}

#[tokio::test]
async fn test_get_contacts_list() {
    let pool = test_pool().await;
    for i in 0..5 {
        let input = CreateContactInput {
            display_name: format!("Contact {}", i),
            emails: vec![EmailInput { email: format!("c{}@test.com", i), r#type: "work".to_string(), is_primary: true }],
            ..Default::default()
        };
        create_contact_inner(&pool, "acc1", input).await.unwrap();
    }

    let list = get_contacts_inner(&pool, "acc1", None, None, 0, 10).await.unwrap();
    assert_eq!(list.len(), 5);
}

#[tokio::test]
async fn test_get_contacts_with_search() {
    let pool = test_pool().await;
    let input1 = CreateContactInput {
        display_name: "Alice Wonder".to_string(),
        company: Some("Wonderland Inc".to_string()),
        emails: vec![EmailInput { email: "alice@wonder.com".to_string(), r#type: "work".to_string(), is_primary: true }],
        ..Default::default()
    };
    create_contact_inner(&pool, "acc1", input1).await.unwrap();

    let input2 = CreateContactInput {
        display_name: "Bob Builder".to_string(),
        emails: vec![EmailInput { email: "bob@build.com".to_string(), r#type: "work".to_string(), is_primary: true }],
        ..Default::default()
    };
    create_contact_inner(&pool, "acc1", input2).await.unwrap();

    let results = get_contacts_inner(&pool, "acc1", Some("alice"), None, 0, 10).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].contact.display_name, "Alice Wonder");
}

#[tokio::test]
async fn test_update_contact() {
    let pool = test_pool().await;
    let input = CreateContactInput {
        display_name: "Old Name".to_string(),
        emails: vec![EmailInput { email: "old@test.com".to_string(), r#type: "work".to_string(), is_primary: true }],
        ..Default::default()
    };
    let created = create_contact_inner(&pool, "acc1", input).await.unwrap();

    let update = UpdateContactInput {
        display_name: Some("New Name".to_string()),
        company: Some("New Corp".to_string()),
        emails: Some(vec![
            EmailInput { email: "new@test.com".to_string(), r#type: "work".to_string(), is_primary: true },
        ]),
        ..Default::default()
    };
    let updated = update_contact_inner(&pool, &created.contact.id, update).await.unwrap();

    assert_eq!(updated.contact.display_name, "New Name");
    assert_eq!(updated.contact.company, Some("New Corp".to_string()));
    assert_eq!(updated.emails.len(), 1);
    assert_eq!(updated.emails[0].email, "new@test.com");
}

#[tokio::test]
async fn test_delete_contact() {
    let pool = test_pool().await;
    let input = CreateContactInput {
        display_name: "To Delete".to_string(),
        emails: vec![EmailInput { email: "del@test.com".to_string(), r#type: "work".to_string(), is_primary: true }],
        ..Default::default()
    };
    let created = create_contact_inner(&pool, "acc1", input).await.unwrap();

    delete_contact_inner(&pool, &created.contact.id).await.unwrap();

    let result = get_contact_inner(&pool, &created.contact.id).await;
    assert!(result.is_err());
}
```

**Step 2: Run test to verify they fail**

Run: `cd src-tauri && cargo test commands::contacts::tests -- --nocapture`
Expected: FAIL — functions don't exist

**Step 3: Write the implementation**

Add to `contacts.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateContactInput {
    pub display_name: Option<String>,
    pub given_name: Option<String>,
    pub surname: Option<String>,
    pub nickname: Option<String>,
    pub company: Option<String>,
    pub job_title: Option<String>,
    pub department: Option<String>,
    pub notes: Option<String>,
    pub birthday: Option<String>,
    pub photo_uri: Option<String>,
    pub phones: Option<Vec<serde_json::Value>>,
    pub addresses: Option<Vec<serde_json::Value>>,
    pub social_profiles: Option<Vec<serde_json::Value>>,
    pub urls: Option<Vec<serde_json::Value>>,
    pub relations: Option<Vec<serde_json::Value>>,
    pub is_starred: Option<bool>,
    pub emails: Option<Vec<EmailInput>>,
    pub groups: Option<Vec<String>>,
}

pub(crate) async fn get_contact_inner(
    pool: &SqlitePool,
    contact_id: &str,
) -> Result<ContactWithEmails, String> {
    let contact: Contact = sqlx::query_as("SELECT * FROM contacts WHERE id = ?")
        .bind(contact_id)
        .fetch_optional(pool).await.map_err(|e| e.to_string())?
        .ok_or_else(|| "Contact not found".to_string())?;

    let emails: Vec<ContactEmail> = sqlx::query_as(
        "SELECT * FROM contact_emails WHERE contact_id = ? ORDER BY is_primary DESC"
    ).bind(contact_id).fetch_all(pool).await.map_err(|e| e.to_string())?;

    let groups: Vec<(String,)> = sqlx::query_as(
        "SELECT g.name FROM contact_groups g JOIN contact_group_members m ON g.id = m.group_id WHERE m.contact_id = ?"
    ).bind(contact_id).fetch_all(pool).await.map_err(|e| e.to_string())?;

    Ok(ContactWithEmails {
        contact,
        emails,
        groups: groups.into_iter().map(|g| g.0).collect(),
    })
}

pub(crate) async fn get_contacts_inner(
    pool: &SqlitePool,
    account_id: &str,
    search: Option<&str>,
    group_id: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<ContactWithEmails>, String> {
    let contacts: Vec<Contact> = if let Some(q) = search {
        // Search via FTS first, fall back to email LIKE
        let fts_ids: Vec<(String,)> = sqlx::query_as(
            "SELECT c.id FROM contacts c JOIN contacts_fts f ON c.rowid = f.rowid WHERE f.contacts_fts MATCH ? AND c.account_id = ? LIMIT ? OFFSET ?"
        ).bind(&format!("{}*", q)).bind(account_id).bind(limit).bind(offset)
        .fetch_all(pool).await.unwrap_or_default();

        let email_ids: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT ce.contact_id FROM contact_emails ce JOIN contacts c ON c.id = ce.contact_id WHERE ce.email LIKE ? AND c.account_id = ? LIMIT ?"
        ).bind(&format!("%{}%", q)).bind(account_id).bind(limit)
        .fetch_all(pool).await.unwrap_or_default();

        let mut all_ids: Vec<String> = fts_ids.into_iter().map(|r| r.0).collect();
        for eid in email_ids {
            if !all_ids.contains(&eid.0) {
                all_ids.push(eid.0);
            }
        }
        all_ids.truncate(limit as usize);

        if all_ids.is_empty() {
            return Ok(vec![]);
        }

        let placeholders = all_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query_str = format!("SELECT * FROM contacts WHERE id IN ({}) ORDER BY display_name", placeholders);
        let mut query = sqlx::query_as::<_, Contact>(&query_str);
        for id in &all_ids {
            query = query.bind(id);
        }
        query.fetch_all(pool).await.map_err(|e| e.to_string())?
    } else if let Some(gid) = group_id {
        sqlx::query_as(
            "SELECT c.* FROM contacts c JOIN contact_group_members m ON c.id = m.contact_id WHERE c.account_id = ? AND m.group_id = ? ORDER BY c.display_name LIMIT ? OFFSET ?"
        ).bind(account_id).bind(gid).bind(limit).bind(offset)
        .fetch_all(pool).await.map_err(|e| e.to_string())?
    } else {
        sqlx::query_as(
            "SELECT * FROM contacts WHERE account_id = ? ORDER BY display_name LIMIT ? OFFSET ?"
        ).bind(account_id).bind(limit).bind(offset)
        .fetch_all(pool).await.map_err(|e| e.to_string())?
    };

    let mut result = Vec::new();
    for contact in contacts {
        let emails: Vec<ContactEmail> = sqlx::query_as(
            "SELECT * FROM contact_emails WHERE contact_id = ? ORDER BY is_primary DESC"
        ).bind(&contact.id).fetch_all(pool).await.unwrap_or_default();

        let groups: Vec<(String,)> = sqlx::query_as(
            "SELECT g.name FROM contact_groups g JOIN contact_group_members m ON g.id = m.group_id WHERE m.contact_id = ?"
        ).bind(&contact.id).fetch_all(pool).await.unwrap_or_default();

        result.push(ContactWithEmails {
            contact,
            emails,
            groups: groups.into_iter().map(|g| g.0).collect(),
        });
    }

    Ok(result)
}

pub(crate) async fn update_contact_inner(
    pool: &SqlitePool,
    contact_id: &str,
    input: UpdateContactInput,
) -> Result<ContactWithEmails, String> {
    let existing = get_contact_inner(pool, contact_id).await?;
    let now = now_epoch();

    let display_name = input.display_name.unwrap_or(existing.contact.display_name);
    let given_name = input.given_name.or(existing.contact.given_name);
    let surname = input.surname.or(existing.contact.surname);
    let nickname = input.nickname.or(existing.contact.nickname);
    let company = input.company.or(existing.contact.company);
    let job_title = input.job_title.or(existing.contact.job_title);
    let department = input.department.or(existing.contact.department);
    let notes = input.notes.or(existing.contact.notes);
    let birthday = input.birthday.or(existing.contact.birthday);
    let photo_uri = input.photo_uri.or(existing.contact.photo_uri);
    let is_starred = input.is_starred.unwrap_or(existing.contact.is_starred);

    let phones = input.phones.map(|p| serde_json::to_string(&p).unwrap()).unwrap_or(existing.contact.phones);
    let addresses = input.addresses.map(|a| serde_json::to_string(&a).unwrap()).unwrap_or(existing.contact.addresses);
    let social_profiles = input.social_profiles.map(|s| serde_json::to_string(&s).unwrap()).unwrap_or(existing.contact.social_profiles);
    let urls = input.urls.map(|u| serde_json::to_string(&u).unwrap()).unwrap_or(existing.contact.urls);
    let relations = input.relations.map(|r| serde_json::to_string(&r).unwrap()).unwrap_or(existing.contact.relations);

    sqlx::query(
        "UPDATE contacts SET display_name=?, given_name=?, surname=?, nickname=?, company=?, job_title=?, department=?, notes=?, birthday=?, photo_uri=?, phones=?, addresses=?, social_profiles=?, urls=?, relations=?, is_starred=?, updated_at=? WHERE id=?"
    )
    .bind(&display_name).bind(&given_name).bind(&surname).bind(&nickname)
    .bind(&company).bind(&job_title).bind(&department).bind(&notes)
    .bind(&birthday).bind(&photo_uri).bind(&phones).bind(&addresses)
    .bind(&social_profiles).bind(&urls).bind(&relations).bind(is_starred).bind(now)
    .bind(contact_id)
    .execute(pool).await.map_err(|e| e.to_string())?;

    // Rebuild emails if provided
    if let Some(new_emails) = input.emails {
        sqlx::query("DELETE FROM contact_emails WHERE contact_id = ?")
            .bind(contact_id).execute(pool).await.map_err(|e| e.to_string())?;

        for e_input in &new_emails {
            let eid = new_id();
            sqlx::query("INSERT INTO contact_emails (id, contact_id, email, type, is_primary) VALUES (?, ?, ?, ?, ?)")
                .bind(&eid).bind(contact_id).bind(&e_input.email).bind(&e_input.r#type).bind(e_input.is_primary)
                .execute(pool).await.map_err(|e| e.to_string())?;
        }
    }

    // Rebuild groups if provided
    if let Some(new_groups) = input.groups {
        sqlx::query("DELETE FROM contact_group_members WHERE contact_id = ?")
            .bind(contact_id).execute(pool).await.map_err(|e| e.to_string())?;
        for gid in &new_groups {
            sqlx::query("INSERT OR IGNORE INTO contact_group_members (contact_id, group_id) VALUES (?, ?)")
                .bind(contact_id).bind(gid).execute(pool).await.map_err(|e| e.to_string())?;
        }
    }

    // Update FTS
    sqlx::query("DELETE FROM contacts_fts WHERE rowid = (SELECT rowid FROM contacts WHERE id = ?)")
        .bind(contact_id).execute(pool).await.ok();
    sqlx::query("INSERT INTO contacts_fts (rowid, display_name, company, job_title, notes) VALUES ((SELECT rowid FROM contacts WHERE id = ?), ?, ?, ?, ?)")
        .bind(contact_id).bind(&display_name).bind(&company).bind(&job_title).bind(&notes)
        .execute(pool).await.ok();

    get_contact_inner(pool, contact_id).await
}

pub(crate) async fn delete_contact_inner(
    pool: &SqlitePool,
    contact_id: &str,
) -> Result<(), String> {
    // FTS cleanup
    sqlx::query("DELETE FROM contacts_fts WHERE rowid = (SELECT rowid FROM contacts WHERE id = ?)")
        .bind(contact_id).execute(pool).await.ok();

    sqlx::query("DELETE FROM contacts WHERE id = ?")
        .bind(contact_id).execute(pool).await.map_err(|e| e.to_string())?;

    Ok(())
}

// --- Tauri Commands ---

#[tauri::command]
pub async fn get_contact(
    app_handle: tauri::AppHandle,
    contact_id: String,
) -> Result<ContactWithEmails, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    get_contact_inner(pool.inner(), &contact_id).await
}

#[tauri::command]
pub async fn get_contacts(
    app_handle: tauri::AppHandle,
    search: Option<String>,
    group_id: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
    account_id: Option<String>,
) -> Result<Vec<ContactWithEmails>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let acc_id = match account_id {
        Some(id) => id,
        None => super::accounts::get_active_account(pool.inner()).await?.id,
    };
    get_contacts_inner(
        pool.inner(), &acc_id,
        search.as_deref(), group_id.as_deref(),
        offset.unwrap_or(0), limit.unwrap_or(50),
    ).await
}

#[tauri::command]
pub async fn update_contact(
    app_handle: tauri::AppHandle,
    contact_id: String,
    input: UpdateContactInput,
) -> Result<ContactWithEmails, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    update_contact_inner(pool.inner(), &contact_id, input).await
}

#[tauri::command]
pub async fn delete_contact(
    app_handle: tauri::AppHandle,
    contact_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    delete_contact_inner(pool.inner(), &contact_id).await
}
```

**Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test commands::contacts::tests -- --nocapture`
Expected: All PASS

**Step 5: Commit**

```bash
git add src-tauri/src/commands/contacts.rs
git commit -m "feat(contacts): add get, list, update, delete commands"
```

---

## Task 5: Contact Search — Replace Compose Autocomplete

**Files:**
- Modify: `src-tauri/src/commands/contacts.rs`
- Modify: `src-tauri/src/commands/compose.rs`

**Step 1: Write the failing test**

Add to `contacts.rs` tests:

```rust
#[tokio::test]
async fn test_search_contacts_autocomplete() {
    let pool = test_pool().await;
    // Create contacts
    let input1 = CreateContactInput {
        display_name: "Alice Anderson".to_string(),
        company: Some("Tech Corp".to_string()),
        emails: vec![EmailInput { email: "alice@techcorp.com".to_string(), r#type: "work".to_string(), is_primary: true }],
        ..Default::default()
    };
    create_contact_inner(&pool, "acc1", input1).await.unwrap();

    let input2 = CreateContactInput {
        display_name: "Bob Builder".to_string(),
        emails: vec![EmailInput { email: "bob@builder.io".to_string(), r#type: "work".to_string(), is_primary: true }],
        ..Default::default()
    };
    create_contact_inner(&pool, "acc1", input2).await.unwrap();

    // Also insert a message from an unknown sender (legacy path)
    sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_html, body_plain) VALUES ('m1', 't1', 'acc1', 'Charlie <charlie@unknown.com>', '', 'Hi', '', 1000, '', '')")
        .execute(&pool).await.unwrap();

    // Search should find Alice from contacts
    let results = search_contacts_autocomplete(&pool, "acc1", "ali").await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].email, "alice@techcorp.com");
    assert_eq!(results[0].name, "Alice Anderson");

    // Search by email domain should work
    let results = search_contacts_autocomplete(&pool, "acc1", "techcorp").await.unwrap();
    assert_eq!(results.len(), 1);

    // Search should also find legacy message senders
    let results = search_contacts_autocomplete(&pool, "acc1", "charlie").await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].email, "charlie@unknown.com");
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test test_search_contacts_autocomplete -- --nocapture`
Expected: FAIL — function doesn't exist

**Step 3: Write the implementation**

Add to `contacts.rs`:

```rust
use crate::commands::compose::ContactSuggestion;

pub(crate) async fn search_contacts_autocomplete(
    pool: &SqlitePool,
    account_id: &str,
    query: &str,
) -> Result<Vec<ContactSuggestion>, String> {
    let mut suggestions: Vec<ContactSuggestion> = Vec::new();
    let mut seen_emails: std::collections::HashSet<String> = std::collections::HashSet::new();
    let pattern = format!("%{}%", query);

    // 1. Search contact_emails table (fast, indexed)
    let email_results: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT c.display_name, ce.email, c.id FROM contact_emails ce JOIN contacts c ON c.id = ce.contact_id WHERE c.account_id = ? AND (ce.email LIKE ? OR c.display_name LIKE ?) ORDER BY ce.is_primary DESC LIMIT 10"
    ).bind(account_id).bind(&pattern).bind(&pattern)
    .fetch_all(pool).await.unwrap_or_default();

    for (name, email, _id) in email_results {
        let lower = email.to_lowercase();
        if seen_emails.insert(lower) {
            let raw = if name.is_empty() { email.clone() } else { format!("{} <{}>", name, email) };
            suggestions.push(ContactSuggestion { name, email, raw });
        }
    }

    // 2. If under 10 results, search FTS
    if suggestions.len() < 10 {
        let fts_results: Vec<(String, String)> = sqlx::query_as(
            "SELECT c.display_name, ce.email FROM contacts c JOIN contacts_fts f ON c.rowid = f.rowid JOIN contact_emails ce ON ce.contact_id = c.id WHERE f.contacts_fts MATCH ? AND c.account_id = ? AND ce.is_primary = 1 LIMIT ?"
        ).bind(&format!("{}*", query)).bind(account_id).bind(10i64 - suggestions.len() as i64)
        .fetch_all(pool).await.unwrap_or_default();

        for (name, email) in fts_results {
            let lower = email.to_lowercase();
            if seen_emails.insert(lower) {
                let raw = if name.is_empty() { email.clone() } else { format!("{} <{}>", name, email) };
                suggestions.push(ContactSuggestion { name, email, raw });
            }
        }
    }

    // 3. Fall back to message history for contacts not yet in store
    if suggestions.len() < 10 {
        let legacy = crate::commands::compose::search_contacts_inner(pool, account_id, query).await.unwrap_or_default();
        for s in legacy {
            let lower = s.email.to_lowercase();
            if seen_emails.insert(lower) {
                suggestions.push(s);
            }
            if suggestions.len() >= 10 { break; }
        }
    }

    suggestions.truncate(10);
    Ok(suggestions)
}

#[tauri::command]
pub async fn search_contacts_v2(
    app_handle: tauri::AppHandle,
    query: String,
    account_id: Option<String>,
) -> Result<Vec<ContactSuggestion>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let acc_id = match account_id {
        Some(id) => id,
        None => super::accounts::get_active_account(pool.inner()).await?.id,
    };
    search_contacts_autocomplete(pool.inner(), &acc_id, &query).await
}
```

In `compose.rs`, make `ContactSuggestion` and `search_contacts_inner` public:
- Change `pub(crate) async fn search_contacts_inner` — already `pub(crate)`, no change needed.
- Ensure `ContactSuggestion` is `pub` (it already is).

Update the existing `search_contacts` command in `compose.rs` to delegate to the new implementation:

```rust
#[tauri::command]
pub async fn search_contacts(
    app_handle: tauri::AppHandle,
    query: String,
) -> Result<Vec<ContactSuggestion>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    super::contacts::search_contacts_autocomplete(pool.inner(), &account.id, &query).await
}
```

**Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test test_search_contacts -- --nocapture`
Expected: All PASS (both old and new tests)

**Step 5: Commit**

```bash
git add src-tauri/src/commands/contacts.rs src-tauri/src/commands/compose.rs
git commit -m "feat(contacts): replace compose autocomplete with contact store search"
```

---

## Task 6: Contact Groups CRUD

**Files:**
- Modify: `src-tauri/src/commands/contacts.rs`

**Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn test_create_and_list_groups() {
    let pool = test_pool().await;

    let group = create_group_inner(&pool, "acc1", "VIP Clients", Some("#ff0000")).await.unwrap();
    assert_eq!(group.name, "VIP Clients");
    assert_eq!(group.color, Some("#ff0000".to_string()));

    create_group_inner(&pool, "acc1", "Friends", None).await.unwrap();

    let groups = get_groups_inner(&pool, "acc1").await.unwrap();
    assert_eq!(groups.len(), 2);
}

#[tokio::test]
async fn test_assign_contact_to_group() {
    let pool = test_pool().await;
    let group = create_group_inner(&pool, "acc1", "Team", None).await.unwrap();
    let contact = create_contact_inner(&pool, "acc1", CreateContactInput {
        display_name: "Member".to_string(),
        emails: vec![EmailInput { email: "member@team.com".to_string(), r#type: "work".to_string(), is_primary: true }],
        ..Default::default()
    }).await.unwrap();

    set_contact_groups_inner(&pool, &contact.contact.id, vec![group.id.clone()]).await.unwrap();

    let fetched = get_contact_inner(&pool, &contact.contact.id).await.unwrap();
    assert_eq!(fetched.groups, vec!["Team"]);

    let in_group = get_contacts_inner(&pool, "acc1", None, Some(&group.id), 0, 10).await.unwrap();
    assert_eq!(in_group.len(), 1);
}

#[tokio::test]
async fn test_delete_group() {
    let pool = test_pool().await;
    let group = create_group_inner(&pool, "acc1", "Temp", None).await.unwrap();
    delete_group_inner(&pool, &group.id).await.unwrap();

    let groups = get_groups_inner(&pool, "acc1").await.unwrap();
    assert_eq!(groups.len(), 0);
}
```

**Step 2: Run test to verify they fail**

Run: `cd src-tauri && cargo test test_create_and_list_groups test_assign_contact test_delete_group -- --nocapture`
Expected: FAIL

**Step 3: Write the implementation**

Add to `contacts.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ContactGroup {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub color: Option<String>,
    pub remote_id: Option<String>,
    pub created_at: i64,
}

pub(crate) async fn create_group_inner(
    pool: &SqlitePool,
    account_id: &str,
    name: &str,
    color: Option<&str>,
) -> Result<ContactGroup, String> {
    let id = new_id();
    let now = now_epoch();
    sqlx::query("INSERT INTO contact_groups (id, account_id, name, color, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(&id).bind(account_id).bind(name).bind(color).bind(now)
        .execute(pool).await.map_err(|e| e.to_string())?;

    Ok(ContactGroup { id, account_id: account_id.to_string(), name: name.to_string(), color: color.map(|c| c.to_string()), remote_id: None, created_at: now })
}

pub(crate) async fn get_groups_inner(
    pool: &SqlitePool,
    account_id: &str,
) -> Result<Vec<ContactGroup>, String> {
    sqlx::query_as("SELECT * FROM contact_groups WHERE account_id = ? ORDER BY name")
        .bind(account_id).fetch_all(pool).await.map_err(|e| e.to_string())
}

pub(crate) async fn update_group_inner(
    pool: &SqlitePool,
    group_id: &str,
    name: Option<&str>,
    color: Option<&str>,
) -> Result<ContactGroup, String> {
    if let Some(n) = name {
        sqlx::query("UPDATE contact_groups SET name = ? WHERE id = ?")
            .bind(n).bind(group_id).execute(pool).await.map_err(|e| e.to_string())?;
    }
    if let Some(c) = color {
        sqlx::query("UPDATE contact_groups SET color = ? WHERE id = ?")
            .bind(c).bind(group_id).execute(pool).await.map_err(|e| e.to_string())?;
    }
    sqlx::query_as("SELECT * FROM contact_groups WHERE id = ?")
        .bind(group_id).fetch_one(pool).await.map_err(|e| e.to_string())
}

pub(crate) async fn delete_group_inner(
    pool: &SqlitePool,
    group_id: &str,
) -> Result<(), String> {
    sqlx::query("DELETE FROM contact_group_members WHERE group_id = ?")
        .bind(group_id).execute(pool).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM contact_groups WHERE id = ?")
        .bind(group_id).execute(pool).await.map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) async fn set_contact_groups_inner(
    pool: &SqlitePool,
    contact_id: &str,
    group_ids: Vec<String>,
) -> Result<(), String> {
    sqlx::query("DELETE FROM contact_group_members WHERE contact_id = ?")
        .bind(contact_id).execute(pool).await.map_err(|e| e.to_string())?;
    for gid in &group_ids {
        sqlx::query("INSERT INTO contact_group_members (contact_id, group_id) VALUES (?, ?)")
            .bind(contact_id).bind(gid).execute(pool).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

// Tauri commands
#[tauri::command]
pub async fn get_contact_groups(
    app_handle: tauri::AppHandle,
    account_id: Option<String>,
) -> Result<Vec<ContactGroup>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let acc_id = match account_id {
        Some(id) => id,
        None => super::accounts::get_active_account(pool.inner()).await?.id,
    };
    get_groups_inner(pool.inner(), &acc_id).await
}

#[tauri::command]
pub async fn create_contact_group(
    app_handle: tauri::AppHandle,
    name: String,
    color: Option<String>,
    account_id: Option<String>,
) -> Result<ContactGroup, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let acc_id = match account_id {
        Some(id) => id,
        None => super::accounts::get_active_account(pool.inner()).await?.id,
    };
    create_group_inner(pool.inner(), &acc_id, &name, color.as_deref()).await
}

#[tauri::command]
pub async fn update_contact_group(
    app_handle: tauri::AppHandle,
    group_id: String,
    name: Option<String>,
    color: Option<String>,
) -> Result<ContactGroup, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    update_group_inner(pool.inner(), &group_id, name.as_deref(), color.as_deref()).await
}

#[tauri::command]
pub async fn delete_contact_group(
    app_handle: tauri::AppHandle,
    group_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    delete_group_inner(pool.inner(), &group_id).await
}

#[tauri::command]
pub async fn set_contact_groups(
    app_handle: tauri::AppHandle,
    contact_id: String,
    group_ids: Vec<String>,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    set_contact_groups_inner(pool.inner(), &contact_id, group_ids).await
}
```

**Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test commands::contacts::tests -- --nocapture`
Expected: All PASS

**Step 5: Commit**

```bash
git add src-tauri/src/commands/contacts.rs
git commit -m "feat(contacts): add group CRUD and contact-group assignment"
```

---

## Task 7: Contact Merge

**Files:**
- Modify: `src-tauri/src/commands/contacts.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_merge_contacts() {
    let pool = test_pool().await;
    let c1 = create_contact_inner(&pool, "acc1", CreateContactInput {
        display_name: "John Doe".to_string(),
        company: Some("Acme".to_string()),
        emails: vec![EmailInput { email: "john@acme.com".to_string(), r#type: "work".to_string(), is_primary: true }],
        ..Default::default()
    }).await.unwrap();

    let c2 = create_contact_inner(&pool, "acc1", CreateContactInput {
        display_name: "J. Doe".to_string(),
        job_title: Some("Engineer".to_string()),
        emails: vec![EmailInput { email: "jdoe@personal.com".to_string(), r#type: "personal".to_string(), is_primary: true }],
        ..Default::default()
    }).await.unwrap();

    // Merge c2 into c1 (c1 is the primary)
    let merged = merge_contacts_inner(&pool, &c1.contact.id, &c2.contact.id).await.unwrap();

    assert_eq!(merged.contact.display_name, "John Doe"); // keeps primary's name
    assert_eq!(merged.contact.company, Some("Acme".to_string()));
    assert_eq!(merged.contact.job_title, Some("Engineer".to_string())); // fills from secondary
    assert_eq!(merged.emails.len(), 2); // both emails preserved

    // Secondary should be deleted
    let result = get_contact_inner(&pool, &c2.contact.id).await;
    assert!(result.is_err());
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test test_merge_contacts -- --nocapture`
Expected: FAIL

**Step 3: Write the implementation**

```rust
pub(crate) async fn merge_contacts_inner(
    pool: &SqlitePool,
    primary_id: &str,
    secondary_id: &str,
) -> Result<ContactWithEmails, String> {
    let primary = get_contact_inner(pool, primary_id).await?;
    let secondary = get_contact_inner(pool, secondary_id).await?;
    let now = now_epoch();

    // Merge fields: primary wins, fill blanks from secondary
    let company = primary.contact.company.or(secondary.contact.company);
    let job_title = primary.contact.job_title.or(secondary.contact.job_title);
    let department = primary.contact.department.or(secondary.contact.department);
    let nickname = primary.contact.nickname.or(secondary.contact.nickname);
    let notes = match (primary.contact.notes, secondary.contact.notes) {
        (Some(a), Some(b)) => Some(format!("{}\n{}", a, b)),
        (a, b) => a.or(b),
    };
    let birthday = primary.contact.birthday.or(secondary.contact.birthday);
    let photo_uri = primary.contact.photo_uri.or(secondary.contact.photo_uri);

    // Merge JSON arrays
    let phones: Vec<serde_json::Value> = {
        let mut p: Vec<serde_json::Value> = serde_json::from_str(&primary.contact.phones).unwrap_or_default();
        let s: Vec<serde_json::Value> = serde_json::from_str(&secondary.contact.phones).unwrap_or_default();
        p.extend(s);
        p
    };
    let addresses: Vec<serde_json::Value> = {
        let mut p: Vec<serde_json::Value> = serde_json::from_str(&primary.contact.addresses).unwrap_or_default();
        let s: Vec<serde_json::Value> = serde_json::from_str(&secondary.contact.addresses).unwrap_or_default();
        p.extend(s);
        p
    };
    let social_profiles: Vec<serde_json::Value> = {
        let mut p: Vec<serde_json::Value> = serde_json::from_str(&primary.contact.social_profiles).unwrap_or_default();
        let s: Vec<serde_json::Value> = serde_json::from_str(&secondary.contact.social_profiles).unwrap_or_default();
        p.extend(s);
        p
    };

    sqlx::query(
        "UPDATE contacts SET company=?, job_title=?, department=?, nickname=?, notes=?, birthday=?, photo_uri=?, phones=?, addresses=?, social_profiles=?, updated_at=? WHERE id=?"
    )
    .bind(&company).bind(&job_title).bind(&department).bind(&nickname)
    .bind(&notes).bind(&birthday).bind(&photo_uri)
    .bind(serde_json::to_string(&phones).unwrap())
    .bind(serde_json::to_string(&addresses).unwrap())
    .bind(serde_json::to_string(&social_profiles).unwrap())
    .bind(now).bind(primary_id)
    .execute(pool).await.map_err(|e| e.to_string())?;

    // Move emails from secondary to primary (skip duplicates)
    for email in &secondary.emails {
        let exists: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM contact_emails WHERE email = ? COLLATE NOCASE"
        ).bind(&email.email).fetch_optional(pool).await.unwrap_or(None);

        if exists.is_none() {
            sqlx::query("INSERT INTO contact_emails (id, contact_id, email, type, is_primary) VALUES (?, ?, ?, ?, 0)")
                .bind(&new_id()).bind(primary_id).bind(&email.email).bind(&email.r#type)
                .execute(pool).await.ok();
        }
    }

    // Move provider links from secondary
    sqlx::query("UPDATE contact_provider_links SET contact_id = ? WHERE contact_id = ?")
        .bind(primary_id).bind(secondary_id)
        .execute(pool).await.ok();

    // Move group memberships
    sqlx::query("UPDATE OR IGNORE contact_group_members SET contact_id = ? WHERE contact_id = ?")
        .bind(primary_id).bind(secondary_id)
        .execute(pool).await.ok();

    // Delete secondary
    delete_contact_inner(pool, secondary_id).await?;

    // Update FTS
    sqlx::query("DELETE FROM contacts_fts WHERE rowid = (SELECT rowid FROM contacts WHERE id = ?)")
        .bind(primary_id).execute(pool).await.ok();
    sqlx::query("INSERT INTO contacts_fts (rowid, display_name, company, job_title, notes) VALUES ((SELECT rowid FROM contacts WHERE id = ?), ?, ?, ?, ?)")
        .bind(primary_id).bind(&primary.contact.display_name).bind(&company).bind(&job_title).bind(&notes)
        .execute(pool).await.ok();

    get_contact_inner(pool, primary_id).await
}

#[tauri::command]
pub async fn merge_contacts(
    app_handle: tauri::AppHandle,
    primary_id: String,
    secondary_id: String,
) -> Result<ContactWithEmails, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    merge_contacts_inner(pool.inner(), &primary_id, &secondary_id).await
}
```

**Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test test_merge_contacts -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/commands/contacts.rs
git commit -m "feat(contacts): add contact merge with field-level resolution"
```

---

## Task 8: Import/Export (vCard + CSV)

**Files:**
- Modify: `src-tauri/src/commands/contacts.rs`
- Modify: `src-tauri/Cargo.toml` (if vCard crate needed)

**Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn test_import_vcard() {
    let pool = test_pool().await;
    let vcard_data = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Jane Smith\r\nN:Smith;Jane;;;\r\nEMAIL;TYPE=WORK:jane@smith.com\r\nTEL;TYPE=CELL:+1234567890\r\nORG:SmithCo\r\nTITLE:CEO\r\nEND:VCARD\r\n";

    let imported = import_vcard_inner(&pool, "acc1", vcard_data).await.unwrap();
    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].contact.display_name, "Jane Smith");
    assert_eq!(imported[0].contact.company, Some("SmithCo".to_string()));
    assert_eq!(imported[0].emails[0].email, "jane@smith.com");
}

#[tokio::test]
async fn test_import_csv() {
    let pool = test_pool().await;
    let csv_data = "Name,Email,Phone,Company,Title\nJohn Doe,john@doe.com,+1111111111,DoeCo,Manager\nJane Roe,jane@roe.com,,RoeCo,";

    let imported = import_csv_inner(&pool, "acc1", csv_data).await.unwrap();
    assert_eq!(imported.len(), 2);
    assert_eq!(imported[0].contact.display_name, "John Doe");
    assert_eq!(imported[1].contact.display_name, "Jane Roe");
}

#[tokio::test]
async fn test_export_vcard() {
    let pool = test_pool().await;
    create_contact_inner(&pool, "acc1", CreateContactInput {
        display_name: "Export Test".to_string(),
        given_name: Some("Export".to_string()),
        surname: Some("Test".to_string()),
        company: Some("TestCo".to_string()),
        emails: vec![EmailInput { email: "export@test.com".to_string(), r#type: "work".to_string(), is_primary: true }],
        ..Default::default()
    }).await.unwrap();

    let vcard = export_vcard_inner(&pool, "acc1", None).await.unwrap();
    assert!(vcard.contains("BEGIN:VCARD"));
    assert!(vcard.contains("FN:Export Test"));
    assert!(vcard.contains("EMAIL;TYPE=WORK:export@test.com"));
    assert!(vcard.contains("ORG:TestCo"));
    assert!(vcard.contains("END:VCARD"));
}

#[tokio::test]
async fn test_export_csv() {
    let pool = test_pool().await;
    create_contact_inner(&pool, "acc1", CreateContactInput {
        display_name: "CSV User".to_string(),
        company: Some("CSVCo".to_string()),
        job_title: Some("Dev".to_string()),
        emails: vec![EmailInput { email: "csv@test.com".to_string(), r#type: "work".to_string(), is_primary: true }],
        ..Default::default()
    }).await.unwrap();

    let csv = export_csv_inner(&pool, "acc1", None).await.unwrap();
    assert!(csv.contains("Name,Email,Phone,Company,Title"));
    assert!(csv.contains("CSV User,csv@test.com,"));
}
```

**Step 2: Run test to verify they fail**

Run: `cd src-tauri && cargo test test_import test_export -- --nocapture`
Expected: FAIL

**Step 3: Write the implementation**

Add to `contacts.rs`:

```rust
pub(crate) async fn import_vcard_inner(
    pool: &SqlitePool,
    account_id: &str,
    data: &str,
) -> Result<Vec<ContactWithEmails>, String> {
    let mut results = Vec::new();

    for card in data.split("END:VCARD") {
        let card = card.trim();
        if !card.contains("BEGIN:VCARD") { continue; }

        let mut display_name = String::new();
        let mut given_name = None;
        let mut surname = None;
        let mut company = None;
        let mut job_title = None;
        let mut emails: Vec<EmailInput> = Vec::new();
        let mut phones: Vec<serde_json::Value> = Vec::new();

        for line in card.lines() {
            let line = line.trim();
            if line.starts_with("FN:") {
                display_name = line[3..].to_string();
            } else if line.starts_with("N:") {
                let parts: Vec<&str> = line[2..].split(';').collect();
                if parts.len() >= 2 {
                    surname = Some(parts[0].to_string()).filter(|s| !s.is_empty());
                    given_name = Some(parts[1].to_string()).filter(|s| !s.is_empty());
                }
            } else if line.contains("EMAIL") {
                let email_type = if line.contains("WORK") { "work" } else if line.contains("HOME") { "personal" } else { "other" };
                if let Some(val) = line.split(':').last() {
                    emails.push(EmailInput { email: val.to_string(), r#type: email_type.to_string(), is_primary: emails.is_empty() });
                }
            } else if line.contains("TEL") {
                let phone_type = if line.contains("CELL") { "mobile" } else if line.contains("WORK") { "work" } else { "other" };
                if let Some(val) = line.split(':').last() {
                    phones.push(serde_json::json!({"type": phone_type, "value": val}));
                }
            } else if line.starts_with("ORG:") {
                company = Some(line[4..].trim_end_matches(';').to_string());
            } else if line.starts_with("TITLE:") {
                job_title = Some(line[6..].to_string());
            }
        }

        if display_name.is_empty() && emails.is_empty() { continue; }
        if display_name.is_empty() {
            display_name = emails.first().map(|e| e.email.clone()).unwrap_or_default();
        }

        let input = CreateContactInput {
            display_name,
            given_name,
            surname,
            company,
            job_title,
            phones,
            emails,
            ..Default::default()
        };

        match create_contact_inner(pool, account_id, input).await {
            Ok(c) => results.push(c),
            Err(_) => continue, // skip duplicates
        }
    }

    Ok(results)
}

pub(crate) async fn import_csv_inner(
    pool: &SqlitePool,
    account_id: &str,
    data: &str,
) -> Result<Vec<ContactWithEmails>, String> {
    let mut lines = data.lines();
    let header = lines.next().ok_or("Empty CSV")?;
    let cols: Vec<&str> = header.split(',').map(|c| c.trim().to_lowercase()).collect::<Vec<_>>().iter().map(|s| s.as_str()).collect();
    // Re-collect as owned for indexing
    let cols: Vec<String> = header.split(',').map(|c| c.trim().to_lowercase()).collect();

    let idx = |name: &str| cols.iter().position(|c| c == name);
    let name_idx = idx("name").or_else(|| idx("full name")).or_else(|| idx("display name"));
    let email_idx = idx("email").or_else(|| idx("e-mail")).or_else(|| idx("email address"));
    let phone_idx = idx("phone").or_else(|| idx("telephone")).or_else(|| idx("mobile"));
    let company_idx = idx("company").or_else(|| idx("organization")).or_else(|| idx("org"));
    let title_idx = idx("title").or_else(|| idx("job title")).or_else(|| idx("position"));

    let mut results = Vec::new();

    for line in lines {
        let fields: Vec<&str> = line.split(',').collect();
        let get = |i: Option<usize>| i.and_then(|idx| fields.get(idx).map(|s| s.trim().to_string())).filter(|s| !s.is_empty());

        let display_name = get(name_idx).unwrap_or_default();
        if display_name.is_empty() { continue; }

        let emails = get(email_idx).map(|e| vec![EmailInput { email: e, r#type: "work".to_string(), is_primary: true }]).unwrap_or_default();
        let phones = get(phone_idx).map(|p| vec![serde_json::json!({"type": "mobile", "value": p})]).unwrap_or_default();

        let input = CreateContactInput {
            display_name,
            company: get(company_idx),
            job_title: get(title_idx),
            emails,
            phones,
            ..Default::default()
        };

        match create_contact_inner(pool, account_id, input).await {
            Ok(c) => results.push(c),
            Err(_) => continue,
        }
    }

    Ok(results)
}

pub(crate) async fn export_vcard_inner(
    pool: &SqlitePool,
    account_id: &str,
    contact_ids: Option<Vec<String>>,
) -> Result<String, String> {
    let contacts = if let Some(ids) = contact_ids {
        let mut result = Vec::new();
        for id in ids {
            result.push(get_contact_inner(pool, &id).await?);
        }
        result
    } else {
        get_contacts_inner(pool, account_id, None, None, 0, 10000).await?
    };

    let mut output = String::new();
    for c in contacts {
        output.push_str("BEGIN:VCARD\r\n");
        output.push_str("VERSION:3.0\r\n");
        output.push_str(&format!("FN:{}\r\n", c.contact.display_name));

        let surname = c.contact.surname.as_deref().unwrap_or("");
        let given = c.contact.given_name.as_deref().unwrap_or("");
        output.push_str(&format!("N:{};{};;;\r\n", surname, given));

        for email in &c.emails {
            let t = email.r#type.to_uppercase();
            output.push_str(&format!("EMAIL;TYPE={}:{}\r\n", t, email.email));
        }

        let phones: Vec<serde_json::Value> = serde_json::from_str(&c.contact.phones).unwrap_or_default();
        for phone in phones {
            let t = phone.get("type").and_then(|v| v.as_str()).unwrap_or("OTHER").to_uppercase();
            let v = phone.get("value").and_then(|v| v.as_str()).unwrap_or("");
            output.push_str(&format!("TEL;TYPE={}:{}\r\n", t, v));
        }

        if let Some(org) = &c.contact.company {
            output.push_str(&format!("ORG:{}\r\n", org));
        }
        if let Some(title) = &c.contact.job_title {
            output.push_str(&format!("TITLE:{}\r\n", title));
        }

        output.push_str("END:VCARD\r\n");
    }

    Ok(output)
}

pub(crate) async fn export_csv_inner(
    pool: &SqlitePool,
    account_id: &str,
    contact_ids: Option<Vec<String>>,
) -> Result<String, String> {
    let contacts = if let Some(ids) = contact_ids {
        let mut result = Vec::new();
        for id in ids {
            result.push(get_contact_inner(pool, &id).await?);
        }
        result
    } else {
        get_contacts_inner(pool, account_id, None, None, 0, 10000).await?
    };

    let mut output = String::from("Name,Email,Phone,Company,Title\n");
    for c in contacts {
        let email = c.emails.first().map(|e| e.email.as_str()).unwrap_or("");
        let phones: Vec<serde_json::Value> = serde_json::from_str(&c.contact.phones).unwrap_or_default();
        let phone = phones.first().and_then(|p| p.get("value")).and_then(|v| v.as_str()).unwrap_or("");
        let company = c.contact.company.as_deref().unwrap_or("");
        let title = c.contact.job_title.as_deref().unwrap_or("");
        output.push_str(&format!("{},{},{},{},{}\n", c.contact.display_name, email, phone, company, title));
    }

    Ok(output)
}

// Tauri commands
#[tauri::command]
pub async fn import_contacts(
    app_handle: tauri::AppHandle,
    data: String,
    format: String,
    account_id: Option<String>,
) -> Result<Vec<ContactWithEmails>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let acc_id = match account_id {
        Some(id) => id,
        None => super::accounts::get_active_account(pool.inner()).await?.id,
    };
    match format.as_str() {
        "vcard" | "vcf" => import_vcard_inner(pool.inner(), &acc_id, &data).await,
        "csv" => import_csv_inner(pool.inner(), &acc_id, &data).await,
        _ => Err("Unsupported format. Use 'vcard' or 'csv'".to_string()),
    }
}

#[tauri::command]
pub async fn export_contacts(
    app_handle: tauri::AppHandle,
    format: String,
    contact_ids: Option<Vec<String>>,
    account_id: Option<String>,
) -> Result<String, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let acc_id = match account_id {
        Some(id) => id,
        None => super::accounts::get_active_account(pool.inner()).await?.id,
    };
    match format.as_str() {
        "vcard" | "vcf" => export_vcard_inner(pool.inner(), &acc_id, contact_ids).await,
        "csv" => export_csv_inner(pool.inner(), &acc_id, contact_ids).await,
        _ => Err("Unsupported format. Use 'vcard' or 'csv'".to_string()),
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test test_import test_export -- --nocapture`
Expected: All PASS

**Step 5: Commit**

```bash
git add src-tauri/src/commands/contacts.rs
git commit -m "feat(contacts): add vCard and CSV import/export"
```

---

## Task 9: Register Commands in lib.rs

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Step 1: Add all contact commands to the generate_handler macro**

After line 163 (`commands::compose::search_contacts,`), add:

```rust
commands::contacts::create_contact,
commands::contacts::get_contact,
commands::contacts::get_contacts,
commands::contacts::update_contact,
commands::contacts::delete_contact,
commands::contacts::search_contacts_v2,
commands::contacts::merge_contacts,
commands::contacts::get_contact_groups,
commands::contacts::create_contact_group,
commands::contacts::update_contact_group,
commands::contacts::delete_contact_group,
commands::contacts::set_contact_groups,
commands::contacts::import_contacts,
commands::contacts::export_contacts,
```

**Step 2: Run cargo check to verify compilation**

Run: `cd src-tauri && cargo check`
Expected: No errors

**Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(contacts): register all contact commands in Tauri handler"
```

---

## Task 10: Frontend — Contact Store

**Files:**
- Create: `src/lib/stores/contacts.ts`

**Step 1: Write the failing test**

Create `src/lib/stores/contacts.test.ts`:

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import { contacts, selectedContactId, contactSearchQuery, contactFilter, isContactsSyncing, loadContacts, searchContacts } from './contacts';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

describe('contacts store', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        contacts.set([]);
        selectedContactId.set(null);
        contactSearchQuery.set('');
    });

    it('initializes with empty state', () => {
        expect(get(contacts)).toEqual([]);
        expect(get(selectedContactId)).toBeNull();
        expect(get(contactSearchQuery)).toBe('');
        expect(get(isContactsSyncing)).toBe(false);
    });

    it('loadContacts fetches and populates store', async () => {
        const mockContacts = [
            { id: '1', display_name: 'Alice', emails: [{ email: 'alice@test.com' }], groups: [] },
            { id: '2', display_name: 'Bob', emails: [{ email: 'bob@test.com' }], groups: [] },
        ];
        vi.mocked(invoke).mockResolvedValue(mockContacts);

        await loadContacts();

        expect(invoke).toHaveBeenCalledWith('get_contacts', { search: null, groupId: null, offset: 0, limit: 50, accountId: null });
        expect(get(contacts)).toEqual(mockContacts);
    });

    it('searchContacts passes query to backend', async () => {
        vi.mocked(invoke).mockResolvedValue([]);

        await searchContacts('alice');

        expect(invoke).toHaveBeenCalledWith('get_contacts', { search: 'alice', groupId: null, offset: 0, limit: 50, accountId: null });
    });
});
```

**Step 2: Run test to verify it fails**

Run: `npx vitest run src/lib/stores/contacts.test.ts`
Expected: FAIL — module doesn't exist

**Step 3: Write the implementation**

Create `src/lib/stores/contacts.ts`:

```typescript
import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export interface ContactEmail {
    id: string;
    contact_id: string;
    email: string;
    type: string;
    is_primary: boolean;
}

export interface ContactWithEmails {
    id: string;
    account_id: string;
    display_name: string;
    given_name: string | null;
    surname: string | null;
    nickname: string | null;
    company: string | null;
    job_title: string | null;
    department: string | null;
    notes: string | null;
    birthday: string | null;
    photo_uri: string | null;
    phones: string;
    addresses: string;
    social_profiles: string;
    urls: string;
    relations: string;
    is_starred: boolean;
    source: string;
    created_at: number;
    updated_at: number;
    emails: ContactEmail[];
    groups: string[];
}

export interface ContactGroup {
    id: string;
    account_id: string;
    name: string;
    color: string | null;
    remote_id: string | null;
    created_at: number;
}

export const contacts = writable<ContactWithEmails[]>([]);
export const selectedContactId = writable<string | null>(null);
export const contactSearchQuery = writable<string>('');
export const contactFilter = writable<{ group?: string; starred?: boolean }>({});
export const isContactsSyncing = writable(false);
export const contactGroups = writable<ContactGroup[]>([]);

export async function loadContacts(search?: string, groupId?: string) {
    const result = await invoke<ContactWithEmails[]>('get_contacts', {
        search: search || null,
        groupId: groupId || null,
        offset: 0,
        limit: 50,
        accountId: null,
    });
    contacts.set(result);
    return result;
}

export async function searchContacts(query: string) {
    return loadContacts(query);
}

export async function loadContactGroups() {
    const result = await invoke<ContactGroup[]>('get_contact_groups', { accountId: null });
    contactGroups.set(result);
    return result;
}

export async function createContact(input: any) {
    const result = await invoke<ContactWithEmails>('create_contact', { input, accountId: null });
    contacts.update(list => [...list, result]);
    return result;
}

export async function updateContact(contactId: string, input: any) {
    const result = await invoke<ContactWithEmails>('update_contact', { contactId, input });
    contacts.update(list => list.map(c => c.id === contactId ? result : c));
    return result;
}

export async function deleteContact(contactId: string) {
    await invoke('delete_contact', { contactId });
    contacts.update(list => list.filter(c => c.id !== contactId));
    selectedContactId.update(id => id === contactId ? null : id);
}

export async function mergeContacts(primaryId: string, secondaryId: string) {
    const result = await invoke<ContactWithEmails>('merge_contacts', { primaryId, secondaryId });
    contacts.update(list => list.filter(c => c.id !== secondaryId).map(c => c.id === primaryId ? result : c));
    return result;
}
```

**Step 4: Run test to verify it passes**

Run: `npx vitest run src/lib/stores/contacts.test.ts`
Expected: PASS

**Step 5: Commit**

```bash
git add src/lib/stores/contacts.ts src/lib/stores/contacts.test.ts
git commit -m "feat(contacts): add frontend contact store with CRUD operations"
```

---

## Task 11: Frontend — Contacts View Component

**Files:**
- Create: `src/lib/components/Contacts.svelte`
- Modify: `src/routes/+page.svelte`

**Step 1: Write the failing test**

Create `src/lib/components/Contacts.test.js`:

```javascript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import Contacts from './Contacts.svelte';

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

describe('Contacts.svelte', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(invoke).mockResolvedValue([]);
    });

    it('renders contacts view with search', () => {
        render(Contacts);
        expect(screen.getByPlaceholderText('Search contacts...')).toBeInTheDocument();
    });

    it('displays contact list', async () => {
        vi.mocked(invoke).mockResolvedValue([
            { id: '1', display_name: 'Alice Anderson', company: 'TechCo', emails: [{ email: 'alice@tech.com', is_primary: true }], groups: [] },
            { id: '2', display_name: 'Bob Builder', company: null, emails: [{ email: 'bob@build.io', is_primary: true }], groups: [] },
        ]);

        render(Contacts);
        await waitFor(() => {
            expect(screen.getByText('Alice Anderson')).toBeInTheDocument();
            expect(screen.getByText('Bob Builder')).toBeInTheDocument();
        });
    });

    it('shows new contact button', () => {
        render(Contacts);
        expect(screen.getByTitle('New contact')).toBeInTheDocument();
    });
});
```

**Step 2: Run test to verify it fails**

Run: `npx vitest run src/lib/components/Contacts.test.js`
Expected: FAIL — component doesn't exist

**Step 3: Write the implementation**

Create `src/lib/components/Contacts.svelte`:

```svelte
<script lang="ts">
    import { onMount } from 'svelte';
    import { invoke } from '@tauri-apps/api/core';
    import {
        contacts,
        selectedContactId,
        contactSearchQuery,
        contactGroups,
        loadContacts,
        loadContactGroups,
        deleteContact,
        type ContactWithEmails,
        type ContactGroup,
    } from '$lib/stores/contacts';
    import { addToast } from '$lib/stores/toast';

    let searchInput = $state('');
    let searchDebounce: ReturnType<typeof setTimeout>;
    let showForm = $state(false);
    let editingContact = $state<ContactWithEmails | null>(null);
    let selectedGroup = $state<string | null>(null);
    let showImportExport = $state(false);

    onMount(async () => {
        await loadContacts();
        await loadContactGroups();
    });

    async function handleSearch(value: string) {
        clearTimeout(searchDebounce);
        searchDebounce = setTimeout(async () => {
            await loadContacts(value || undefined, selectedGroup || undefined);
        }, 200);
    }

    function selectContact(id: string) {
        selectedContactId.set(id);
    }

    async function handleDelete(id: string) {
        if (!confirm('Delete this contact?')) return;
        await deleteContact(id);
        addToast('Contact deleted', 'success');
    }

    function handleNew() {
        editingContact = null;
        showForm = true;
    }

    function handleEdit(contact: ContactWithEmails) {
        editingContact = contact;
        showForm = true;
    }

    async function handleGroupFilter(groupId: string | null) {
        selectedGroup = groupId;
        await loadContacts(searchInput || undefined, groupId || undefined);
    }

    function getInitials(name: string): string {
        return name.split(' ').map(w => w[0]).slice(0, 2).join('').toUpperCase();
    }

    let selectedContact = $derived($contacts.find(c => c.id === $selectedContactId) || null);
</script>

<div class="contacts-container">
    <div class="contacts-sidebar">
        <div class="contacts-header">
            <h2>Contacts</h2>
            <div class="contacts-actions">
                <button class="icon-btn" title="Import/Export" onclick={() => { showImportExport = true; }}>
                    <svg viewBox="0 0 16 16" width="14" height="14"><path fill="currentColor" d="M8 1a.5.5 0 0 1 .5.5v5.793l2.146-2.147a.5.5 0 0 1 .708.708l-3 3a.5.5 0 0 1-.708 0l-3-3a.5.5 0 1 1 .708-.708L7.5 7.293V1.5A.5.5 0 0 1 8 1zM2 10a.5.5 0 0 1 .5.5v2a.5.5 0 0 0 .5.5h10a.5.5 0 0 0 .5-.5v-2a.5.5 0 0 1 1 0v2A1.5 1.5 0 0 1 13 14H3a1.5 1.5 0 0 1-1.5-1.5v-2A.5.5 0 0 1 2 10z"/></svg>
                </button>
                <button class="icon-btn" title="New contact" onclick={handleNew}>
                    <svg viewBox="0 0 16 16" width="14" height="14"><path fill="currentColor" d="M8 2a.5.5 0 0 1 .5.5v5h5a.5.5 0 0 1 0 1h-5v5a.5.5 0 0 1-1 0v-5h-5a.5.5 0 0 1 0-1h5v-5A.5.5 0 0 1 8 2z"/></svg>
                </button>
            </div>
        </div>

        <input
            type="text"
            class="contacts-search"
            placeholder="Search contacts..."
            bind:value={searchInput}
            oninput={(e) => handleSearch(e.currentTarget.value)}
        />

        <div class="group-filter">
            <button class:active={!selectedGroup} onclick={() => handleGroupFilter(null)}>All</button>
            <button class:active={selectedGroup === '__starred'} onclick={() => handleGroupFilter('__starred')}>Starred</button>
            {#each $contactGroups as group}
                <button class:active={selectedGroup === group.id} onclick={() => handleGroupFilter(group.id)}>
                    {#if group.color}<span class="group-dot" style="background:{group.color}"></span>{/if}
                    {group.name}
                </button>
            {/each}
        </div>

        <div class="contacts-list">
            {#each $contacts as contact}
                <button
                    class="contact-item"
                    class:selected={$selectedContactId === contact.id}
                    onclick={() => selectContact(contact.id)}
                >
                    <div class="contact-avatar">{getInitials(contact.display_name)}</div>
                    <div class="contact-info">
                        <div class="contact-name">{contact.display_name}</div>
                        <div class="contact-email">{contact.emails[0]?.email || ''}</div>
                        {#if contact.company}
                            <div class="contact-company">{contact.company}</div>
                        {/if}
                    </div>
                </button>
            {/each}
            {#if $contacts.length === 0}
                <div class="empty-state">No contacts found</div>
            {/if}
        </div>
    </div>

    <div class="contact-detail-panel">
        {#if selectedContact}
            <div class="contact-detail">
                <div class="detail-header">
                    <div class="detail-avatar">{getInitials(selectedContact.display_name)}</div>
                    <div class="detail-name-section">
                        <h3>{selectedContact.display_name}</h3>
                        {#if selectedContact.job_title || selectedContact.company}
                            <p class="detail-subtitle">
                                {selectedContact.job_title || ''}{selectedContact.job_title && selectedContact.company ? ' at ' : ''}{selectedContact.company || ''}
                            </p>
                        {/if}
                    </div>
                    <div class="detail-actions">
                        <button class="icon-btn" title="Edit" onclick={() => handleEdit(selectedContact)}>
                            <svg viewBox="0 0 16 16" width="14" height="14"><path fill="currentColor" d="M12.146.854a.5.5 0 0 1 .708 0l2.292 2.292a.5.5 0 0 1 0 .708l-9.5 9.5a.5.5 0 0 1-.168.11l-4 1.5a.5.5 0 0 1-.65-.65l1.5-4a.5.5 0 0 1 .11-.168l9.5-9.5z"/></svg>
                        </button>
                        <button class="icon-btn danger" title="Delete" onclick={() => handleDelete(selectedContact.id)}>
                            <svg viewBox="0 0 16 16" width="14" height="14"><path fill="currentColor" d="M5.5 5.5A.5.5 0 0 1 6 6v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5zm2.5 0a.5.5 0 0 1 .5.5v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5zm3 .5a.5.5 0 0 0-1 0v6a.5.5 0 0 0 1 0V6z"/><path fill="currentColor" fill-rule="evenodd" d="M14.5 3a1 1 0 0 1-1 1H13v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4h-.5a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1H6a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1h3.5a1 1 0 0 1 1 1v1z"/></svg>
                        </button>
                    </div>
                </div>

                <div class="detail-section">
                    <h4>Email</h4>
                    {#each selectedContact.emails as email}
                        <div class="detail-field">
                            <span class="field-value">{email.email}</span>
                            <span class="field-type">{email.type}</span>
                        </div>
                    {/each}
                </div>

                {#if JSON.parse(selectedContact.phones || '[]').length > 0}
                    <div class="detail-section">
                        <h4>Phone</h4>
                        {#each JSON.parse(selectedContact.phones) as phone}
                            <div class="detail-field">
                                <span class="field-value">{phone.value}</span>
                                <span class="field-type">{phone.type}</span>
                            </div>
                        {/each}
                    </div>
                {/if}

                {#if selectedContact.notes}
                    <div class="detail-section">
                        <h4>Notes</h4>
                        <p class="detail-notes">{selectedContact.notes}</p>
                    </div>
                {/if}

                {#if selectedContact.groups.length > 0}
                    <div class="detail-section">
                        <h4>Groups</h4>
                        <div class="detail-groups">
                            {#each selectedContact.groups as group}
                                <span class="group-badge">{group}</span>
                            {/each}
                        </div>
                    </div>
                {/if}
            </div>
        {:else}
            <div class="empty-detail">
                <p>Select a contact to view details</p>
            </div>
        {/if}
    </div>
</div>

<style>
    .contacts-container {
        display: flex;
        height: 100%;
        background: var(--bg-primary);
    }
    .contacts-sidebar {
        width: 320px;
        border-right: 1px solid var(--border-color);
        display: flex;
        flex-direction: column;
        overflow: hidden;
    }
    .contacts-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 12px 16px;
        border-bottom: 1px solid var(--border-color);
    }
    .contacts-header h2 {
        font-size: 14px;
        font-weight: 600;
        margin: 0;
    }
    .contacts-actions {
        display: flex;
        gap: 4px;
    }
    .icon-btn {
        background: none;
        border: none;
        padding: 4px 6px;
        border-radius: 4px;
        cursor: pointer;
        color: var(--text-secondary);
    }
    .icon-btn:hover { background: var(--hover-bg); }
    .icon-btn.danger:hover { color: var(--danger-color, #e53e3e); }
    .contacts-search {
        margin: 8px 12px;
        padding: 6px 10px;
        border: 1px solid var(--border-color);
        border-radius: 6px;
        font-size: 13px;
        background: var(--input-bg);
        color: var(--text-primary);
        outline: none;
    }
    .contacts-search:focus { border-color: var(--accent-color); }
    .group-filter {
        display: flex;
        gap: 4px;
        padding: 4px 12px 8px;
        flex-wrap: wrap;
    }
    .group-filter button {
        font-size: 11px;
        padding: 2px 8px;
        border-radius: 10px;
        border: 1px solid var(--border-color);
        background: none;
        color: var(--text-secondary);
        cursor: pointer;
    }
    .group-filter button.active {
        background: var(--accent-color);
        color: white;
        border-color: var(--accent-color);
    }
    .group-dot {
        display: inline-block;
        width: 6px;
        height: 6px;
        border-radius: 50%;
        margin-right: 4px;
    }
    .contacts-list {
        flex: 1;
        overflow-y: auto;
    }
    .contact-item {
        display: flex;
        align-items: center;
        gap: 10px;
        padding: 8px 12px;
        width: 100%;
        border: none;
        background: none;
        cursor: pointer;
        text-align: left;
    }
    .contact-item:hover { background: var(--hover-bg); }
    .contact-item.selected { background: var(--selected-bg); }
    .contact-avatar, .detail-avatar {
        width: 32px;
        height: 32px;
        border-radius: 50%;
        background: var(--accent-color);
        color: white;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 12px;
        font-weight: 600;
        flex-shrink: 0;
    }
    .detail-avatar { width: 48px; height: 48px; font-size: 16px; }
    .contact-info { min-width: 0; }
    .contact-name { font-size: 13px; font-weight: 500; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .contact-email { font-size: 11px; color: var(--text-secondary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .contact-company { font-size: 11px; color: var(--text-tertiary); }
    .contact-detail-panel { flex: 1; overflow-y: auto; }
    .contact-detail { padding: 24px; }
    .detail-header { display: flex; align-items: center; gap: 16px; margin-bottom: 24px; }
    .detail-name-section { flex: 1; }
    .detail-name-section h3 { margin: 0; font-size: 18px; }
    .detail-subtitle { margin: 4px 0 0; color: var(--text-secondary); font-size: 13px; }
    .detail-actions { display: flex; gap: 4px; }
    .detail-section { margin-bottom: 16px; }
    .detail-section h4 { font-size: 11px; text-transform: uppercase; color: var(--text-tertiary); margin: 0 0 6px; letter-spacing: 0.5px; }
    .detail-field { display: flex; align-items: center; gap: 8px; padding: 4px 0; }
    .field-value { font-size: 13px; color: var(--text-primary); }
    .field-type { font-size: 11px; color: var(--text-tertiary); background: var(--tag-bg); padding: 1px 6px; border-radius: 4px; }
    .detail-notes { font-size: 13px; color: var(--text-secondary); margin: 0; white-space: pre-wrap; }
    .detail-groups { display: flex; gap: 4px; flex-wrap: wrap; }
    .group-badge { font-size: 11px; padding: 2px 8px; border-radius: 10px; background: var(--tag-bg); color: var(--text-secondary); }
    .empty-state, .empty-detail { display: flex; align-items: center; justify-content: center; height: 100%; color: var(--text-tertiary); font-size: 13px; }
</style>
```

**Step 4: Integrate into +page.svelte**

In `src/routes/+page.svelte`:

1. Update `viewMode` type (line 107):
```typescript
let viewMode = $state<"mail" | "calendar" | "subscriptions" | "contacts">("mail");
```

2. Add the contacts view block after the subscriptions block (around line 2073):
```svelte
{:else if viewMode === "contacts"}
    <Contacts />
```

3. Add import at top of script:
```typescript
import Contacts from '$lib/components/Contacts.svelte';
```

**Step 5: Run test to verify it passes**

Run: `npx vitest run src/lib/components/Contacts.test.js`
Expected: PASS

**Step 6: Commit**

```bash
git add src/lib/components/Contacts.svelte src/lib/components/Contacts.test.js src/routes/+page.svelte
git commit -m "feat(contacts): add contacts view component with list and detail panel"
```

---

## Task 12: Frontend — Sidebar Navigation for Contacts

**Files:**
- Modify: `src/lib/components/Sidebar.svelte`
- Modify: `src/routes/+page.svelte`

**Step 1: Add contacts nav item to Sidebar**

In `Sidebar.svelte`, add a "Contacts" button. The sidebar has callback props — add an `ontogglecontacts` prop and a button that calls it.

In the props interface, add:
```typescript
ontogglecontacts?: () => void;
```

Add a contacts button in the sidebar template (after the subscriptions/calendar toggles):

```svelte
<button
    class="nav-item"
    class:active={viewMode === "contacts"}
    onclick={() => ontogglecontacts?.()}
    title="Contacts"
>
    <svg viewBox="0 0 16 16" width="16" height="16"><path fill="currentColor" d="M8 8a3 3 0 1 0 0-6 3 3 0 0 0 0 6zm2-3a2 2 0 1 1-4 0 2 2 0 0 1 4 0zm4 8c0 1-1 1-1 1H3s-1 0-1-1 1-4 6-4 6 3 6 4zm-1-.004c-.001-.246-.154-.986-.832-1.664C11.516 10.68 10.289 10 8 10c-2.29 0-3.516.68-4.168 1.332-.678.678-.83 1.418-.832 1.664h10z"/></svg>
    <span>Contacts</span>
</button>
```

**Step 2: Wire up in +page.svelte**

Pass the callback from +page.svelte to Sidebar:

```svelte
<Sidebar
    ...existingProps
    ontogglecontacts={() => { viewMode = "contacts"; }}
/>
```

**Step 3: Run the app to verify navigation works**

Run: `npm run tauri dev`
Expected: Clicking "Contacts" in sidebar shows the contacts view

**Step 4: Commit**

```bash
git add src/lib/components/Sidebar.svelte src/routes/+page.svelte
git commit -m "feat(contacts): add contacts navigation to sidebar"
```

---

## Task 13: Frontend — Contact Form (Create/Edit)

**Files:**
- Create: `src/lib/components/ContactForm.svelte`
- Modify: `src/lib/components/Contacts.svelte`

**Step 1: Write the failing test**

Create `src/lib/components/ContactForm.test.js`:

```javascript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import ContactForm from './ContactForm.svelte';

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

describe('ContactForm.svelte', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(invoke).mockResolvedValue({ id: '1', display_name: 'Test', emails: [], groups: [] });
    });

    it('renders create form with empty fields', () => {
        render(ContactForm, { onClose: vi.fn(), onSaved: vi.fn() });
        expect(screen.getByPlaceholderText('Display name')).toBeInTheDocument();
        expect(screen.getByPlaceholderText('Email')).toBeInTheDocument();
        expect(screen.getByText('Save')).toBeInTheDocument();
    });

    it('renders edit form with pre-filled fields', () => {
        const contact = {
            id: '1', display_name: 'Alice', given_name: 'Alice', surname: 'Smith',
            company: 'TechCo', job_title: 'Dev', emails: [{ email: 'alice@tech.com', type: 'work', is_primary: true }],
            phones: '[]', addresses: '[]', social_profiles: '[]', urls: '[]', relations: '[]', groups: [],
        };
        render(ContactForm, { contact, onClose: vi.fn(), onSaved: vi.fn() });
        expect(screen.getByDisplayValue('Alice')).toBeInTheDocument();
    });
});
```

**Step 2: Run test to verify it fails**

Run: `npx vitest run src/lib/components/ContactForm.test.js`
Expected: FAIL

**Step 3: Write the implementation**

Create `src/lib/components/ContactForm.svelte`:

```svelte
<script lang="ts">
    import { invoke } from '@tauri-apps/api/core';
    import { addToast } from '$lib/stores/toast';
    import { createContact, updateContact, type ContactWithEmails } from '$lib/stores/contacts';

    let { contact = null, onClose, onSaved }: {
        contact?: ContactWithEmails | null;
        onClose: () => void;
        onSaved: (c: ContactWithEmails) => void;
    } = $props();

    let displayName = $state(contact?.display_name || '');
    let givenName = $state(contact?.given_name || '');
    let surname = $state(contact?.surname || '');
    let nickname = $state(contact?.nickname || '');
    let company = $state(contact?.company || '');
    let jobTitle = $state(contact?.job_title || '');
    let department = $state(contact?.department || '');
    let notes = $state(contact?.notes || '');
    let birthday = $state(contact?.birthday || '');

    let emails = $state<Array<{ email: string; type: string; is_primary: boolean }>>(
        contact?.emails?.map(e => ({ email: e.email, type: e.type, is_primary: e.is_primary })) || [{ email: '', type: 'work', is_primary: true }]
    );
    let phones = $state<Array<{ type: string; value: string }>>(
        contact ? JSON.parse(contact.phones || '[]') : []
    );

    let isSaving = $state(false);

    function addEmail() {
        emails = [...emails, { email: '', type: 'other', is_primary: false }];
    }

    function removeEmail(idx: number) {
        emails = emails.filter((_, i) => i !== idx);
    }

    function addPhone() {
        phones = [...phones, { type: 'mobile', value: '' }];
    }

    function removePhone(idx: number) {
        phones = phones.filter((_, i) => i !== idx);
    }

    async function handleSubmit() {
        if (!displayName.trim()) {
            addToast('Display name is required', 'error');
            return;
        }
        const validEmails = emails.filter(e => e.email.trim());
        if (validEmails.length === 0) {
            addToast('At least one email is required', 'error');
            return;
        }

        isSaving = true;
        try {
            const input = {
                display_name: displayName.trim(),
                given_name: givenName.trim() || null,
                surname: surname.trim() || null,
                nickname: nickname.trim() || null,
                company: company.trim() || null,
                job_title: jobTitle.trim() || null,
                department: department.trim() || null,
                notes: notes.trim() || null,
                birthday: birthday || null,
                phones: phones.filter(p => p.value.trim()),
                addresses: [],
                social_profiles: [],
                urls: [],
                relations: [],
                emails: validEmails,
                groups: contact?.groups || [],
            };

            let result: ContactWithEmails;
            if (contact) {
                result = await updateContact(contact.id, input);
                addToast('Contact updated', 'success');
            } else {
                result = await createContact(input);
                addToast('Contact created', 'success');
            }
            onSaved(result);
        } catch (e: any) {
            addToast(e?.toString() || 'Failed to save contact', 'error');
        } finally {
            isSaving = false;
        }
    }
</script>

<div class="form-overlay" onclick={onClose}>
    <div class="form-modal" onclick={(e) => e.stopPropagation()}>
        <div class="form-header">
            <h3>{contact ? 'Edit Contact' : 'New Contact'}</h3>
            <button class="close-btn" onclick={onClose}>&times;</button>
        </div>

        <form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
            <div class="form-row">
                <input type="text" placeholder="Display name" bind:value={displayName} required />
            </div>
            <div class="form-row split">
                <input type="text" placeholder="First name" bind:value={givenName} />
                <input type="text" placeholder="Last name" bind:value={surname} />
            </div>
            <div class="form-row split">
                <input type="text" placeholder="Company" bind:value={company} />
                <input type="text" placeholder="Job title" bind:value={jobTitle} />
            </div>

            <div class="form-section">
                <label>Emails</label>
                {#each emails as email, i}
                    <div class="multi-row">
                        <input type="email" placeholder="Email" bind:value={email.email} />
                        <select bind:value={email.type}>
                            <option value="work">Work</option>
                            <option value="personal">Personal</option>
                            <option value="other">Other</option>
                        </select>
                        {#if emails.length > 1}
                            <button type="button" class="remove-btn" onclick={() => removeEmail(i)}>&times;</button>
                        {/if}
                    </div>
                {/each}
                <button type="button" class="add-btn" onclick={addEmail}>+ Add email</button>
            </div>

            <div class="form-section">
                <label>Phones</label>
                {#each phones as phone, i}
                    <div class="multi-row">
                        <input type="tel" placeholder="Phone" bind:value={phone.value} />
                        <select bind:value={phone.type}>
                            <option value="mobile">Mobile</option>
                            <option value="work">Work</option>
                            <option value="home">Home</option>
                        </select>
                        <button type="button" class="remove-btn" onclick={() => removePhone(i)}>&times;</button>
                    </div>
                {/each}
                <button type="button" class="add-btn" onclick={addPhone}>+ Add phone</button>
            </div>

            <div class="form-row">
                <textarea placeholder="Notes" bind:value={notes} rows="3"></textarea>
            </div>

            <div class="form-footer">
                <button type="button" class="cancel-btn" onclick={onClose}>Cancel</button>
                <button type="submit" class="save-btn" disabled={isSaving}>
                    {isSaving ? 'Saving...' : 'Save'}
                </button>
            </div>
        </form>
    </div>
</div>

<style>
    .form-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.4); display: flex; align-items: center; justify-content: center; z-index: 1000; }
    .form-modal { background: var(--bg-primary); border-radius: 12px; padding: 24px; width: 480px; max-height: 80vh; overflow-y: auto; box-shadow: 0 20px 60px rgba(0,0,0,0.3); }
    .form-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; }
    .form-header h3 { margin: 0; font-size: 16px; }
    .close-btn { background: none; border: none; font-size: 20px; cursor: pointer; color: var(--text-secondary); }
    .form-row { margin-bottom: 10px; }
    .form-row.split { display: flex; gap: 8px; }
    .form-row input, .form-row textarea { width: 100%; padding: 8px 10px; border: 1px solid var(--border-color); border-radius: 6px; font-size: 13px; background: var(--input-bg); color: var(--text-primary); }
    .form-row textarea { resize: vertical; }
    .form-section { margin-bottom: 12px; }
    .form-section label { font-size: 11px; text-transform: uppercase; color: var(--text-tertiary); letter-spacing: 0.5px; display: block; margin-bottom: 6px; }
    .multi-row { display: flex; gap: 6px; margin-bottom: 6px; }
    .multi-row input { flex: 1; padding: 6px 8px; border: 1px solid var(--border-color); border-radius: 6px; font-size: 13px; background: var(--input-bg); color: var(--text-primary); }
    .multi-row select { padding: 6px; border: 1px solid var(--border-color); border-radius: 6px; font-size: 12px; background: var(--input-bg); color: var(--text-primary); }
    .remove-btn { background: none; border: none; color: var(--text-tertiary); cursor: pointer; font-size: 16px; padding: 0 4px; }
    .add-btn { background: none; border: none; color: var(--accent-color); font-size: 12px; cursor: pointer; padding: 4px 0; }
    .form-footer { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; padding-top: 16px; border-top: 1px solid var(--border-color); }
    .cancel-btn { padding: 6px 14px; border: 1px solid var(--border-color); border-radius: 6px; background: none; color: var(--text-primary); cursor: pointer; font-size: 13px; }
    .save-btn { padding: 6px 14px; border: none; border-radius: 6px; background: var(--accent-color); color: white; cursor: pointer; font-size: 13px; }
    .save-btn:disabled { opacity: 0.5; }
</style>
```

Wire into `Contacts.svelte` — add import and conditional render:

```svelte
<!-- At top of script -->
import ContactForm from './ContactForm.svelte';

<!-- In template, after the container div -->
{#if showForm}
    <ContactForm
        contact={editingContact}
        onClose={() => { showForm = false; editingContact = null; }}
        onSaved={(c) => { showForm = false; editingContact = null; loadContacts(); }}
    />
{/if}
```

**Step 4: Run test to verify it passes**

Run: `npx vitest run src/lib/components/ContactForm.test.js`
Expected: PASS

**Step 5: Commit**

```bash
git add src/lib/components/ContactForm.svelte src/lib/components/ContactForm.test.js src/lib/components/Contacts.svelte
git commit -m "feat(contacts): add create/edit contact form modal"
```

---

## Task 14: Frontend — Contact Hover Card

**Files:**
- Create: `src/lib/components/ContactHoverCard.svelte`
- Modify: `src/lib/components/ThreadList.svelte` (add hover trigger on sender name)

**Step 1: Write the failing test**

Create `src/lib/components/ContactHoverCard.test.js`:

```javascript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import ContactHoverCard from './ContactHoverCard.svelte';

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

describe('ContactHoverCard.svelte', () => {
    it('displays contact info when shown', () => {
        const contact = {
            display_name: 'Alice Anderson',
            company: 'TechCo',
            job_title: 'Engineer',
            emails: [{ email: 'alice@tech.com', type: 'work', is_primary: true }],
        };
        render(ContactHoverCard, { contact, x: 100, y: 200 });
        expect(screen.getByText('Alice Anderson')).toBeInTheDocument();
        expect(screen.getByText('Engineer at TechCo')).toBeInTheDocument();
        expect(screen.getByText('alice@tech.com')).toBeInTheDocument();
    });
});
```

**Step 2: Run test to verify it fails**

Run: `npx vitest run src/lib/components/ContactHoverCard.test.js`
Expected: FAIL

**Step 3: Write the implementation**

Create `src/lib/components/ContactHoverCard.svelte`:

```svelte
<script lang="ts">
    import type { ContactWithEmails } from '$lib/stores/contacts';

    let { contact, x, y }: { contact: Partial<ContactWithEmails>; x: number; y: number } = $props();

    function getInitials(name: string): string {
        return name.split(' ').map(w => w[0]).slice(0, 2).join('').toUpperCase();
    }

    let subtitle = $derived(
        [contact.job_title, contact.company].filter(Boolean).join(' at ')
    );
</script>

<div class="hover-card" style="left:{x}px; top:{y}px">
    <div class="hc-header">
        <div class="hc-avatar">{getInitials(contact.display_name || '?')}</div>
        <div class="hc-info">
            <div class="hc-name">{contact.display_name}</div>
            {#if subtitle}
                <div class="hc-subtitle">{subtitle}</div>
            {/if}
        </div>
    </div>
    {#if contact.emails?.length}
        <div class="hc-email">{contact.emails[0].email}</div>
    {/if}
</div>

<style>
    .hover-card {
        position: fixed;
        background: var(--bg-primary);
        border: 1px solid var(--border-color);
        border-radius: 8px;
        padding: 12px;
        box-shadow: 0 4px 12px rgba(0,0,0,0.15);
        z-index: 9999;
        min-width: 200px;
        max-width: 280px;
    }
    .hc-header { display: flex; align-items: center; gap: 10px; margin-bottom: 6px; }
    .hc-avatar { width: 28px; height: 28px; border-radius: 50%; background: var(--accent-color); color: white; display: flex; align-items: center; justify-content: center; font-size: 10px; font-weight: 600; }
    .hc-name { font-size: 13px; font-weight: 600; color: var(--text-primary); }
    .hc-subtitle { font-size: 11px; color: var(--text-secondary); }
    .hc-email { font-size: 11px; color: var(--text-secondary); padding-left: 38px; }
</style>
```

**Step 4: Run test to verify it passes**

Run: `npx vitest run src/lib/components/ContactHoverCard.test.js`
Expected: PASS

**Step 5: Commit**

```bash
git add src/lib/components/ContactHoverCard.svelte src/lib/components/ContactHoverCard.test.js
git commit -m "feat(contacts): add contact hover card component"
```

---

## Task 15: Frontend — Import/Export Modal

**Files:**
- Create: `src/lib/components/ContactImportExport.svelte`
- Modify: `src/lib/components/Contacts.svelte`

**Step 1: Write the implementation** (simpler component, test alongside)

Create `src/lib/components/ContactImportExport.svelte`:

```svelte
<script lang="ts">
    import { invoke } from '@tauri-apps/api/core';
    import { open, save } from '@tauri-apps/plugin-dialog';
    import { addToast } from '$lib/stores/toast';
    import { loadContacts } from '$lib/stores/contacts';

    let { onClose }: { onClose: () => void } = $props();

    let isImporting = $state(false);
    let isExporting = $state(false);

    async function handleImport() {
        const file = await open({
            filters: [{ name: 'Contacts', extensions: ['vcf', 'csv'] }],
        });
        if (!file) return;

        isImporting = true;
        try {
            const content = await invoke<string>('read_file_content', { path: file });
            const format = (file as string).endsWith('.csv') ? 'csv' : 'vcard';
            const result = await invoke<any[]>('import_contacts', { data: content, format, accountId: null });
            addToast(`Imported ${result.length} contacts`, 'success');
            await loadContacts();
            onClose();
        } catch (e: any) {
            addToast(e?.toString() || 'Import failed', 'error');
        } finally {
            isImporting = false;
        }
    }

    async function handleExport(format: 'vcard' | 'csv') {
        const ext = format === 'vcard' ? 'vcf' : 'csv';
        const path = await save({
            defaultPath: `contacts.${ext}`,
            filters: [{ name: 'Contacts', extensions: [ext] }],
        });
        if (!path) return;

        isExporting = true;
        try {
            const data = await invoke<string>('export_contacts', { format, contactIds: null, accountId: null });
            await invoke('write_file_content', { path, content: data });
            addToast(`Exported contacts as ${ext.toUpperCase()}`, 'success');
            onClose();
        } catch (e: any) {
            addToast(e?.toString() || 'Export failed', 'error');
        } finally {
            isExporting = false;
        }
    }
</script>

<div class="ie-overlay" onclick={onClose}>
    <div class="ie-modal" onclick={(e) => e.stopPropagation()}>
        <div class="ie-header">
            <h3>Import / Export Contacts</h3>
            <button class="close-btn" onclick={onClose}>&times;</button>
        </div>

        <div class="ie-section">
            <h4>Import</h4>
            <p>Import contacts from a vCard (.vcf) or CSV file.</p>
            <button class="action-btn" onclick={handleImport} disabled={isImporting}>
                {isImporting ? 'Importing...' : 'Choose File...'}
            </button>
        </div>

        <div class="ie-section">
            <h4>Export</h4>
            <p>Export all contacts to a file.</p>
            <div class="export-buttons">
                <button class="action-btn" onclick={() => handleExport('vcard')} disabled={isExporting}>Export as vCard</button>
                <button class="action-btn" onclick={() => handleExport('csv')} disabled={isExporting}>Export as CSV</button>
            </div>
        </div>
    </div>
</div>

<style>
    .ie-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.4); display: flex; align-items: center; justify-content: center; z-index: 1000; }
    .ie-modal { background: var(--bg-primary); border-radius: 12px; padding: 24px; width: 400px; box-shadow: 0 20px 60px rgba(0,0,0,0.3); }
    .ie-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; }
    .ie-header h3 { margin: 0; font-size: 16px; }
    .close-btn { background: none; border: none; font-size: 20px; cursor: pointer; color: var(--text-secondary); }
    .ie-section { margin-bottom: 20px; }
    .ie-section h4 { font-size: 13px; font-weight: 600; margin: 0 0 4px; }
    .ie-section p { font-size: 12px; color: var(--text-secondary); margin: 0 0 8px; }
    .action-btn { padding: 6px 14px; border: 1px solid var(--border-color); border-radius: 6px; background: var(--bg-secondary); color: var(--text-primary); cursor: pointer; font-size: 13px; }
    .action-btn:hover { background: var(--hover-bg); }
    .action-btn:disabled { opacity: 0.5; }
    .export-buttons { display: flex; gap: 8px; }
</style>
```

Wire into `Contacts.svelte`:

```svelte
import ContactImportExport from './ContactImportExport.svelte';

<!-- In template -->
{#if showImportExport}
    <ContactImportExport onClose={() => { showImportExport = false; }} />
{/if}
```

**Step 2: Run app to verify**

Run: `npm run tauri dev`
Expected: Import/Export button in contacts view opens modal

**Step 3: Commit**

```bash
git add src/lib/components/ContactImportExport.svelte src/lib/components/Contacts.svelte
git commit -m "feat(contacts): add import/export modal for vCard and CSV"
```

---

## Task 16: Google Contacts Sync Module

**Files:**
- Create: `src-tauri/src/contacts/mod.rs`
- Create: `src-tauri/src/contacts/google_contacts.rs`
- Modify: `src-tauri/src/lib.rs` (add module)

**Step 1: Write the failing test**

In `src-tauri/src/contacts/google_contacts.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_google_person_to_contact_input() {
        let person_json = serde_json::json!({
            "resourceName": "people/c123456",
            "etag": "abc123",
            "names": [{"displayName": "John Doe", "givenName": "John", "familyName": "Doe"}],
            "emailAddresses": [
                {"value": "john@work.com", "type": "work", "metadata": {"primary": true}},
                {"value": "john@home.com", "type": "home"}
            ],
            "phoneNumbers": [{"value": "+1234567890", "type": "mobile"}],
            "organizations": [{"name": "WorkCo", "title": "Dev", "department": "Eng"}],
            "photos": [{"url": "https://photo.url/pic.jpg"}],
        });

        let input = parse_google_person(&person_json).unwrap();
        assert_eq!(input.display_name, "John Doe");
        assert_eq!(input.given_name, Some("John".to_string()));
        assert_eq!(input.surname, Some("Doe".to_string()));
        assert_eq!(input.company, Some("WorkCo".to_string()));
        assert_eq!(input.job_title, Some("Dev".to_string()));
        assert_eq!(input.emails.len(), 2);
        assert_eq!(input.emails[0].email, "john@work.com");
        assert!(input.emails[0].is_primary);
    }

    #[test]
    fn test_parse_google_person_minimal() {
        let person_json = serde_json::json!({
            "resourceName": "people/c789",
            "etag": "xyz",
            "emailAddresses": [{"value": "minimal@test.com"}],
        });

        let input = parse_google_person(&person_json).unwrap();
        assert_eq!(input.display_name, "minimal@test.com");
        assert_eq!(input.emails.len(), 1);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test contacts::google_contacts::tests -- --nocapture`
Expected: FAIL

**Step 3: Write the implementation**

Create `src-tauri/src/contacts/mod.rs`:

```rust
pub mod google_contacts;
```

Create `src-tauri/src/contacts/google_contacts.rs`:

```rust
use crate::commands::contacts::{CreateContactInput, EmailInput};
use serde_json::Value;

pub fn parse_google_person(person: &Value) -> Result<CreateContactInput, String> {
    let names = person.get("names").and_then(|n| n.as_array());
    let display_name = names
        .and_then(|n| n.first())
        .and_then(|n| n.get("displayName"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let given_name = names.and_then(|n| n.first()).and_then(|n| n.get("givenName")).and_then(|v| v.as_str()).map(|s| s.to_string());
    let surname = names.and_then(|n| n.first()).and_then(|n| n.get("familyName")).and_then(|v| v.as_str()).map(|s| s.to_string());

    let emails_arr = person.get("emailAddresses").and_then(|e| e.as_array());
    let emails: Vec<EmailInput> = emails_arr.map(|arr| {
        arr.iter().enumerate().map(|(i, e)| {
            let email = e.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let t = e.get("type").and_then(|v| v.as_str()).unwrap_or("other").to_string();
            let is_primary = e.get("metadata").and_then(|m| m.get("primary")).and_then(|v| v.as_bool()).unwrap_or(i == 0);
            EmailInput { email, r#type: t, is_primary }
        }).collect()
    }).unwrap_or_default();

    let orgs = person.get("organizations").and_then(|o| o.as_array());
    let company = orgs.and_then(|o| o.first()).and_then(|o| o.get("name")).and_then(|v| v.as_str()).map(|s| s.to_string());
    let job_title = orgs.and_then(|o| o.first()).and_then(|o| o.get("title")).and_then(|v| v.as_str()).map(|s| s.to_string());
    let department = orgs.and_then(|o| o.first()).and_then(|o| o.get("department")).and_then(|v| v.as_str()).map(|s| s.to_string());

    let phones: Vec<serde_json::Value> = person.get("phoneNumbers").and_then(|p| p.as_array()).map(|arr| {
        arr.iter().map(|p| {
            serde_json::json!({
                "type": p.get("type").and_then(|v| v.as_str()).unwrap_or("other"),
                "value": p.get("value").and_then(|v| v.as_str()).unwrap_or("")
            })
        }).collect()
    }).unwrap_or_default();

    let photo_uri = person.get("photos").and_then(|p| p.as_array()).and_then(|p| p.first()).and_then(|p| p.get("url")).and_then(|v| v.as_str()).map(|s| s.to_string());

    let final_display_name = if display_name.is_empty() {
        emails.first().map(|e| e.email.clone()).unwrap_or_else(|| "Unknown".to_string())
    } else {
        display_name.to_string()
    };

    Ok(CreateContactInput {
        display_name: final_display_name,
        given_name,
        surname,
        company,
        job_title,
        department,
        photo_uri,
        phones,
        emails,
        ..Default::default()
    })
}

pub fn get_remote_id(person: &Value) -> Option<String> {
    person.get("resourceName").and_then(|v| v.as_str()).map(|s| s.to_string())
}

pub fn get_etag(person: &Value) -> Option<String> {
    person.get("etag").and_then(|v| v.as_str()).map(|s| s.to_string())
}

pub async fn fetch_google_contacts(
    access_token: &str,
    sync_token: Option<&str>,
) -> Result<(Vec<Value>, Option<String>), String> {
    let client = reqwest::Client::new();
    let mut all_persons = Vec::new();
    let mut page_token: Option<String> = None;
    let mut next_sync_token: Option<String> = None;

    loop {
        let mut url = "https://people.googleapis.com/v1/people/me/connections?personFields=names,emailAddresses,phoneNumbers,organizations,photos,birthdays,addresses,urls,biographies&pageSize=100".to_string();

        if let Some(ref token) = sync_token {
            url.push_str(&format!("&syncToken={}", token));
        }
        if let Some(ref pt) = page_token {
            url.push_str(&format!("&pageToken={}", pt));
        }

        let resp = client.get(&url)
            .bearer_auth(access_token)
            .send().await.map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if status.as_u16() == 410 {
                return Err("SYNC_TOKEN_EXPIRED".to_string());
            }
            return Err(format!("Google People API error {}: {}", status, body));
        }

        let body: Value = resp.json().await.map_err(|e| e.to_string())?;

        if let Some(connections) = body.get("connections").and_then(|c| c.as_array()) {
            all_persons.extend(connections.clone());
        }

        next_sync_token = body.get("nextSyncToken").and_then(|v| v.as_str()).map(|s| s.to_string());
        page_token = body.get("nextPageToken").and_then(|v| v.as_str()).map(|s| s.to_string());

        if page_token.is_none() { break; }
    }

    Ok((all_persons, next_sync_token))
}

pub async fn sync_google_contacts(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    access_token: &str,
) -> Result<usize, String> {
    use crate::commands::contacts::{create_contact_inner, get_contact_inner, update_contact_inner, UpdateContactInput};
    use std::time::{SystemTime, UNIX_EPOCH};

    let existing_token: Option<String> = sqlx::query_scalar(
        "SELECT sync_token FROM contacts_sync_state WHERE account_id = ? AND provider = 'google'"
    ).bind(account_id).fetch_optional(pool).await.unwrap_or(None);

    let (persons, new_token) = match fetch_google_contacts(access_token, existing_token.as_deref()).await {
        Ok(result) => result,
        Err(e) if e == "SYNC_TOKEN_EXPIRED" => {
            sqlx::query("DELETE FROM contacts_sync_state WHERE account_id = ? AND provider = 'google'")
                .bind(account_id).execute(pool).await.ok();
            fetch_google_contacts(access_token, None).await?
        }
        Err(e) => return Err(e),
    };

    let mut synced_count = 0;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;

    for person in &persons {
        let remote_id = match get_remote_id(person) {
            Some(id) => id,
            None => continue,
        };
        let etag = get_etag(person);
        let input = match parse_google_person(person) {
            Ok(i) => i,
            Err(_) => continue,
        };

        if input.emails.is_empty() { continue; }

        // Check if already linked
        let existing_link: Option<(String,)> = sqlx::query_as(
            "SELECT contact_id FROM contact_provider_links WHERE account_id = ? AND provider = 'google' AND remote_id = ?"
        ).bind(account_id).bind(&remote_id).fetch_optional(pool).await.unwrap_or(None);

        if let Some((contact_id,)) = existing_link {
            // Check etag — skip if unchanged
            let stored_etag: Option<String> = sqlx::query_scalar(
                "SELECT etag FROM contact_provider_links WHERE contact_id = ? AND provider = 'google'"
            ).bind(&contact_id).fetch_optional(pool).await.unwrap_or(None);

            if stored_etag.as_deref() == etag.as_deref() { continue; }

            // Update existing
            let update = UpdateContactInput {
                display_name: Some(input.display_name),
                given_name: input.given_name,
                surname: input.surname,
                company: input.company,
                job_title: input.job_title,
                department: input.department,
                phones: Some(input.phones),
                emails: Some(input.emails),
                ..Default::default()
            };
            update_contact_inner(pool, &contact_id, update).await.ok();

            sqlx::query("UPDATE contact_provider_links SET etag = ?, last_synced_at = ? WHERE contact_id = ? AND provider = 'google'")
                .bind(&etag).bind(now).bind(&contact_id).execute(pool).await.ok();
        } else {
            // Check for email match (dedup)
            let mut matched_contact_id: Option<String> = None;
            for e in &input.emails {
                let existing: Option<(String,)> = sqlx::query_as(
                    "SELECT contact_id FROM contact_emails WHERE email = ? COLLATE NOCASE"
                ).bind(&e.email).fetch_optional(pool).await.unwrap_or(None);
                if let Some((cid,)) = existing {
                    matched_contact_id = Some(cid);
                    break;
                }
            }

            let contact_id = if let Some(cid) = matched_contact_id {
                cid
            } else {
                let created = create_contact_inner(pool, account_id, input).await?;
                created.contact.id
            };

            // Create provider link
            let link_id = uuid::Uuid::new_v4().to_string();
            sqlx::query("INSERT OR IGNORE INTO contact_provider_links (id, contact_id, account_id, provider, remote_id, etag, last_synced_at) VALUES (?, ?, ?, 'google', ?, ?, ?)")
                .bind(&link_id).bind(&contact_id).bind(account_id).bind(&remote_id).bind(&etag).bind(now)
                .execute(pool).await.ok();
        }

        synced_count += 1;
    }

    // Store sync token
    if let Some(token) = new_token {
        sqlx::query(
            "INSERT OR REPLACE INTO contacts_sync_state (account_id, provider, sync_token, last_full_sync) VALUES (?, 'google', ?, ?)"
        ).bind(account_id).bind(&token).bind(now).execute(pool).await.ok();
    }

    Ok(synced_count)
}
```

Add `pub mod contacts;` to `src-tauri/src/lib.rs` (module declaration section).

**Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test contacts::google_contacts::tests -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/contacts/ src-tauri/src/lib.rs
git commit -m "feat(contacts): add Google People API sync module"
```

---

## Task 17: Outlook Contacts Sync Module

**Files:**
- Create: `src-tauri/src/contacts/outlook_contacts.rs`
- Modify: `src-tauri/src/contacts/mod.rs`

**Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_outlook_contact() {
        let contact_json = serde_json::json!({
            "id": "AAMkAD123",
            "@odata.etag": "W/\"etag123\"",
            "displayName": "Jane Doe",
            "givenName": "Jane",
            "surname": "Doe",
            "emailAddresses": [
                {"name": "Jane", "address": "jane@outlook.com"},
                {"name": "Jane Work", "address": "jane@work.com"}
            ],
            "mobilePhone": "+9876543210",
            "businessPhones": ["+1112223333"],
            "companyName": "OutlookCo",
            "jobTitle": "PM",
            "department": "Product"
        });

        let input = parse_outlook_contact(&contact_json).unwrap();
        assert_eq!(input.display_name, "Jane Doe");
        assert_eq!(input.given_name, Some("Jane".to_string()));
        assert_eq!(input.company, Some("OutlookCo".to_string()));
        assert_eq!(input.emails.len(), 2);
        assert_eq!(input.emails[0].email, "jane@outlook.com");
    }
}
```

**Step 2: Run test, verify failure, implement, verify pass** — same TDD cycle.

**Step 3: Write the implementation**

Create `src-tauri/src/contacts/outlook_contacts.rs`:

```rust
use crate::commands::contacts::{CreateContactInput, EmailInput};
use serde_json::Value;

pub fn parse_outlook_contact(contact: &Value) -> Result<CreateContactInput, String> {
    let display_name = contact.get("displayName").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let given_name = contact.get("givenName").and_then(|v| v.as_str()).map(|s| s.to_string());
    let surname = contact.get("surname").and_then(|v| v.as_str()).map(|s| s.to_string());
    let company = contact.get("companyName").and_then(|v| v.as_str()).map(|s| s.to_string());
    let job_title = contact.get("jobTitle").and_then(|v| v.as_str()).map(|s| s.to_string());
    let department = contact.get("department").and_then(|v| v.as_str()).map(|s| s.to_string());

    let emails: Vec<EmailInput> = contact.get("emailAddresses").and_then(|e| e.as_array()).map(|arr| {
        arr.iter().enumerate().map(|(i, e)| {
            let email = e.get("address").and_then(|v| v.as_str()).unwrap_or("").to_string();
            EmailInput { email, r#type: "other".to_string(), is_primary: i == 0 }
        }).filter(|e| !e.email.is_empty()).collect()
    }).unwrap_or_default();

    let mut phones: Vec<serde_json::Value> = Vec::new();
    if let Some(mobile) = contact.get("mobilePhone").and_then(|v| v.as_str()) {
        if !mobile.is_empty() {
            phones.push(serde_json::json!({"type": "mobile", "value": mobile}));
        }
    }
    if let Some(biz_phones) = contact.get("businessPhones").and_then(|v| v.as_array()) {
        for p in biz_phones {
            if let Some(val) = p.as_str() {
                phones.push(serde_json::json!({"type": "work", "value": val}));
            }
        }
    }

    let final_name = if display_name.is_empty() {
        emails.first().map(|e| e.email.clone()).unwrap_or_else(|| "Unknown".to_string())
    } else {
        display_name
    };

    Ok(CreateContactInput {
        display_name: final_name,
        given_name,
        surname,
        company,
        job_title,
        department,
        phones,
        emails,
        ..Default::default()
    })
}

pub fn get_remote_id(contact: &Value) -> Option<String> {
    contact.get("id").and_then(|v| v.as_str()).map(|s| s.to_string())
}

pub fn get_etag(contact: &Value) -> Option<String> {
    contact.get("@odata.etag").and_then(|v| v.as_str()).map(|s| s.to_string())
}

pub async fn fetch_outlook_contacts(
    access_token: &str,
    delta_link: Option<&str>,
) -> Result<(Vec<Value>, Option<String>), String> {
    let client = reqwest::Client::new();
    let mut all_contacts = Vec::new();

    let initial_url = delta_link.unwrap_or(
        "https://graph.microsoft.com/v1.0/me/contacts/delta?$select=displayName,givenName,surname,emailAddresses,mobilePhone,businessPhones,companyName,jobTitle,department"
    ).to_string();

    let mut url = Some(initial_url);
    let mut new_delta_link: Option<String> = None;

    while let Some(current_url) = url {
        let resp = client.get(&current_url)
            .bearer_auth(access_token)
            .header("Prefer", "IdType=\"ImmutableId\"")
            .send().await.map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            let status = resp.status();
            if status.as_u16() == 410 {
                return Err("DELTA_TOKEN_EXPIRED".to_string());
            }
            return Err(format!("Microsoft Graph error: {}", status));
        }

        let body: Value = resp.json().await.map_err(|e| e.to_string())?;

        if let Some(values) = body.get("value").and_then(|v| v.as_array()) {
            all_contacts.extend(values.clone());
        }

        url = body.get("@odata.nextLink").and_then(|v| v.as_str()).map(|s| s.to_string());
        if url.is_none() {
            new_delta_link = body.get("@odata.deltaLink").and_then(|v| v.as_str()).map(|s| s.to_string());
        }
    }

    Ok((all_contacts, new_delta_link))
}

pub async fn sync_outlook_contacts(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    access_token: &str,
) -> Result<usize, String> {
    use crate::commands::contacts::{create_contact_inner, update_contact_inner, UpdateContactInput};
    use std::time::{SystemTime, UNIX_EPOCH};

    let existing_token: Option<String> = sqlx::query_scalar(
        "SELECT sync_token FROM contacts_sync_state WHERE account_id = ? AND provider = 'outlook'"
    ).bind(account_id).fetch_optional(pool).await.unwrap_or(None);

    let (contacts, new_delta) = match fetch_outlook_contacts(access_token, existing_token.as_deref()).await {
        Ok(result) => result,
        Err(e) if e == "DELTA_TOKEN_EXPIRED" => {
            sqlx::query("DELETE FROM contacts_sync_state WHERE account_id = ? AND provider = 'outlook'")
                .bind(account_id).execute(pool).await.ok();
            fetch_outlook_contacts(access_token, None).await?
        }
        Err(e) => return Err(e),
    };

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let mut synced_count = 0;

    for contact in &contacts {
        let remote_id = match get_remote_id(contact) {
            Some(id) => id,
            None => continue,
        };
        let etag = get_etag(contact);
        let input = match parse_outlook_contact(contact) {
            Ok(i) => i,
            Err(_) => continue,
        };

        if input.emails.is_empty() { continue; }

        let existing_link: Option<(String,)> = sqlx::query_as(
            "SELECT contact_id FROM contact_provider_links WHERE account_id = ? AND provider = 'outlook' AND remote_id = ?"
        ).bind(account_id).bind(&remote_id).fetch_optional(pool).await.unwrap_or(None);

        if let Some((contact_id,)) = existing_link {
            let stored_etag: Option<String> = sqlx::query_scalar(
                "SELECT etag FROM contact_provider_links WHERE contact_id = ? AND provider = 'outlook'"
            ).bind(&contact_id).fetch_optional(pool).await.unwrap_or(None);

            if stored_etag.as_deref() == etag.as_deref() { continue; }

            let update = UpdateContactInput {
                display_name: Some(input.display_name),
                given_name: input.given_name,
                surname: input.surname,
                company: input.company,
                job_title: input.job_title,
                department: input.department,
                phones: Some(input.phones),
                emails: Some(input.emails),
                ..Default::default()
            };
            update_contact_inner(pool, &contact_id, update).await.ok();

            sqlx::query("UPDATE contact_provider_links SET etag = ?, last_synced_at = ? WHERE contact_id = ? AND provider = 'outlook'")
                .bind(&etag).bind(now).bind(&contact_id).execute(pool).await.ok();
        } else {
            let mut matched_contact_id: Option<String> = None;
            for e in &input.emails {
                let existing: Option<(String,)> = sqlx::query_as(
                    "SELECT contact_id FROM contact_emails WHERE email = ? COLLATE NOCASE"
                ).bind(&e.email).fetch_optional(pool).await.unwrap_or(None);
                if let Some((cid,)) = existing {
                    matched_contact_id = Some(cid);
                    break;
                }
            }

            let contact_id = if let Some(cid) = matched_contact_id {
                cid
            } else {
                let created = create_contact_inner(pool, account_id, input).await?;
                created.contact.id
            };

            let link_id = uuid::Uuid::new_v4().to_string();
            sqlx::query("INSERT OR IGNORE INTO contact_provider_links (id, contact_id, account_id, provider, remote_id, etag, last_synced_at) VALUES (?, ?, ?, 'outlook', ?, ?, ?)")
                .bind(&link_id).bind(&contact_id).bind(account_id).bind(&remote_id).bind(&etag).bind(now)
                .execute(pool).await.ok();
        }

        synced_count += 1;
    }

    if let Some(delta) = new_delta {
        sqlx::query(
            "INSERT OR REPLACE INTO contacts_sync_state (account_id, provider, sync_token, last_full_sync) VALUES (?, 'outlook', ?, ?)"
        ).bind(account_id).bind(&delta).bind(now).execute(pool).await.ok();
    }

    Ok(synced_count)
}
```

Update `src-tauri/src/contacts/mod.rs`:
```rust
pub mod google_contacts;
pub mod outlook_contacts;
```

**Step 4: Run tests**

Run: `cd src-tauri && cargo test contacts::outlook_contacts::tests -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/contacts/outlook_contacts.rs src-tauri/src/contacts/mod.rs
git commit -m "feat(contacts): add Microsoft Graph contacts sync module"
```

---

## Task 18: Contact Sync Integration with Email Sync

**Files:**
- Modify: `src-tauri/src/commands/sync.rs`
- Modify: `src-tauri/src/commands/contacts.rs` (add sync command)

**Step 1: Add sync_contacts command**

In `contacts.rs`:

```rust
#[tauri::command]
pub async fn sync_contacts(
    app_handle: tauri::AppHandle,
    account_id: Option<String>,
) -> Result<usize, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let acc_id = match account_id {
        Some(id) => id,
        None => super::accounts::get_active_account(pool.inner()).await?.id,
    };

    let provider_type: String = sqlx::query_scalar(
        "SELECT provider_type FROM accounts WHERE id = ?"
    ).bind(&acc_id).fetch_one(pool.inner()).await.map_err(|e| e.to_string())?;

    // Check throttle (15 min)
    let last_sync: Option<i64> = sqlx::query_scalar(
        "SELECT last_full_sync FROM contacts_sync_state WHERE account_id = ?"
    ).bind(&acc_id).fetch_optional(pool.inner()).await.unwrap_or(None);

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    if let Some(last) = last_sync {
        if now - last < 900 { return Ok(0); } // 15 min throttle
    }

    let access_token = crate::credentials::get_access_token(pool.inner(), &acc_id).await?;

    match provider_type.as_str() {
        "gmail" => crate::contacts::google_contacts::sync_google_contacts(pool.inner(), &acc_id, &access_token).await,
        "outlook" => crate::contacts::outlook_contacts::sync_outlook_contacts(pool.inner(), &acc_id, &access_token).await,
        _ => Ok(0),
    }
}
```

**Step 2: Hook into email sync**

In `sync.rs`, after the main email sync completes, spawn the contact sync:

```rust
// At the end of sync_gmail_data / sync_outlook_data, add:
let pool_clone = pool.inner().clone();
let acc_clone = account_id.clone();
tauri::async_runtime::spawn(async move {
    if let Err(e) = super::contacts::sync_contacts_inner(&pool_clone, &acc_clone).await {
        tracing::warn!("Contact sync failed: {}", e);
    }
});
```

Add `sync_contacts_inner` (non-command version) to `contacts.rs`:

```rust
pub(crate) async fn sync_contacts_inner(
    pool: &SqlitePool,
    account_id: &str,
) -> Result<usize, String> {
    let provider_type: String = sqlx::query_scalar(
        "SELECT provider_type FROM accounts WHERE id = ?"
    ).bind(account_id).fetch_one(pool).await.map_err(|e| e.to_string())?;

    let last_sync: Option<i64> = sqlx::query_scalar(
        "SELECT last_full_sync FROM contacts_sync_state WHERE account_id = ?"
    ).bind(account_id).fetch_optional(pool).await.unwrap_or(None);

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    if let Some(last) = last_sync {
        if now - last < 900 { return Ok(0); }
    }

    let access_token = crate::credentials::get_access_token(pool, account_id).await?;

    match provider_type.as_str() {
        "gmail" => crate::contacts::google_contacts::sync_google_contacts(pool, account_id, &access_token).await,
        "outlook" => crate::contacts::outlook_contacts::sync_outlook_contacts(pool, account_id, &access_token).await,
        _ => Ok(0),
    }
}
```

**Step 3: Register sync_contacts in lib.rs**

Add to generate_handler: `commands::contacts::sync_contacts,`

**Step 4: Run cargo check**

Run: `cd src-tauri && cargo check`
Expected: No errors

**Step 5: Commit**

```bash
git add src-tauri/src/commands/contacts.rs src-tauri/src/commands/sync.rs src-tauri/src/lib.rs
git commit -m "feat(contacts): integrate contact sync with email sync (15-min throttle)"
```

---

## Task 19: Full Integration Test & Cleanup

**Step 1: Run all Rust tests**

Run: `cd src-tauri && cargo test -- --nocapture`
Expected: All tests pass, including new contact tests

**Step 2: Run all frontend tests**

Run: `npx vitest run`
Expected: All tests pass

**Step 3: Run cargo clippy**

Run: `cd src-tauri && cargo clippy -- -D warnings`
Fix any warnings.

**Step 4: Run the app end-to-end**

Run: `npm run tauri dev`

Verify:
- [ ] Contacts nav item appears in sidebar
- [ ] Clicking it shows the contacts view
- [ ] Create a contact (name + email)
- [ ] Contact appears in list
- [ ] Search finds the contact
- [ ] Edit the contact (change company)
- [ ] Delete the contact
- [ ] Compose new email — autocomplete finds contacts from store
- [ ] Hover card appears on sender names (if integrated)

**Step 5: Final commit**

```bash
git add -A
git commit -m "feat(contacts): complete Phase 1 contact management system"
```

---

## Summary of Commits

| # | Message |
|---|---------|
| 1 | `feat(contacts): add database schema for contact management` |
| 2 | `feat(contacts): add migration 018 for existing databases` |
| 3 | `feat(contacts): add contact types and create command` |
| 4 | `feat(contacts): add get, list, update, delete commands` |
| 5 | `feat(contacts): replace compose autocomplete with contact store search` |
| 6 | `feat(contacts): add group CRUD and contact-group assignment` |
| 7 | `feat(contacts): add contact merge with field-level resolution` |
| 8 | `feat(contacts): add vCard and CSV import/export` |
| 9 | `feat(contacts): register all contact commands in Tauri handler` |
| 10 | `feat(contacts): add frontend contact store with CRUD operations` |
| 11 | `feat(contacts): add contacts view component with list and detail panel` |
| 12 | `feat(contacts): add contacts navigation to sidebar` |
| 13 | `feat(contacts): add create/edit contact form modal` |
| 14 | `feat(contacts): add contact hover card component` |
| 15 | `feat(contacts): add import/export modal for vCard and CSV` |
| 16 | `feat(contacts): add Google People API sync module` |
| 17 | `feat(contacts): add Microsoft Graph contacts sync module` |
| 18 | `feat(contacts): integrate contact sync with email sync (15-min throttle)` |
| 19 | `feat(contacts): complete Phase 1 contact management system` |
