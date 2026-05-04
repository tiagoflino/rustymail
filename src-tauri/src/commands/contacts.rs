use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
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
    #[sqlx(rename = "type")]
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

    let phones_json =
        serde_json::to_string(&input.phones).unwrap_or_else(|_| "[]".to_string());
    let addresses_json =
        serde_json::to_string(&input.addresses).unwrap_or_else(|_| "[]".to_string());
    let social_json =
        serde_json::to_string(&input.social_profiles).unwrap_or_else(|_| "[]".to_string());
    let urls_json =
        serde_json::to_string(&input.urls).unwrap_or_else(|_| "[]".to_string());
    let relations_json =
        serde_json::to_string(&input.relations).unwrap_or_else(|_| "[]".to_string());

    sqlx::query(
        "INSERT INTO contacts (id, account_id, display_name, given_name, surname, nickname, company, job_title, department, notes, birthday, photo_uri, phones, addresses, social_profiles, urls, relations, source, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'local', ?, ?)"
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
        .execute(pool).await.map_err(|_| format!("Email '{}' already exists for another contact", e_input.email))?;

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
        sqlx::query(
            "INSERT OR IGNORE INTO contact_group_members (contact_id, group_id) VALUES (?, ?)",
        )
        .bind(&id)
        .bind(group_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
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

    Ok(ContactWithEmails {
        contact,
        emails,
        groups: input.groups,
    })
}

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
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Contact not found: {}", contact_id))?;

    let emails: Vec<ContactEmail> = sqlx::query_as(
        "SELECT * FROM contact_emails WHERE contact_id = ? ORDER BY is_primary DESC",
    )
    .bind(contact_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let groups: Vec<(String,)> = sqlx::query_as(
        "SELECT g.name FROM contact_groups g JOIN contact_group_members m ON g.id = m.group_id WHERE m.contact_id = ?",
    )
    .bind(contact_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(ContactWithEmails {
        contact,
        emails,
        groups: groups.into_iter().map(|(name,)| name).collect(),
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
    let contact_ids: Vec<String> = if let Some(query) = search {
        // FTS search with prefix match, fallback to LIKE on failure
        let fts_query = format!("{}*", query);
        let fts_ids: Result<Vec<(String,)>, _> = sqlx::query_as(
            "SELECT c.id FROM contacts c WHERE c.account_id = ? AND c.rowid IN (SELECT rowid FROM contacts_fts WHERE contacts_fts MATCH ?) ORDER BY c.display_name ASC LIMIT ? OFFSET ?"
        )
        .bind(account_id)
        .bind(&fts_query)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await;

        let mut ids: Vec<String> = match fts_ids {
            Ok(rows) => rows.into_iter().map(|(id,)| id).collect(),
            Err(_) => {
                // Fallback to LIKE search on display_name
                let like_pattern = format!("%{}%", query);
                let rows: Vec<(String,)> = sqlx::query_as(
                    "SELECT id FROM contacts WHERE account_id = ? AND display_name LIKE ? ORDER BY display_name ASC LIMIT ? OFFSET ?"
                )
                .bind(account_id)
                .bind(&like_pattern)
                .bind(limit)
                .bind(offset)
                .fetch_all(pool)
                .await
                .map_err(|e| e.to_string())?;
                rows.into_iter().map(|(id,)| id).collect()
            }
        };

        // Also search contact_emails with LIKE
        let email_pattern = format!("%{}%", query);
        let email_ids: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT ce.contact_id FROM contact_emails ce JOIN contacts c ON c.id = ce.contact_id WHERE c.account_id = ? AND ce.email LIKE ? LIMIT ?"
        )
        .bind(account_id)
        .bind(&email_pattern)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        for (id,) in email_ids {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }

        ids
    } else if let Some(gid) = group_id {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT c.id FROM contacts c JOIN contact_group_members m ON c.id = m.contact_id WHERE c.account_id = ? AND m.group_id = ? ORDER BY c.display_name ASC LIMIT ? OFFSET ?"
        )
        .bind(account_id)
        .bind(gid)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
        rows.into_iter().map(|(id,)| id).collect()
    } else {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM contacts WHERE account_id = ? ORDER BY display_name ASC LIMIT ? OFFSET ?"
        )
        .bind(account_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
        rows.into_iter().map(|(id,)| id).collect()
    };

    let mut results = Vec::with_capacity(contact_ids.len());
    for id in contact_ids {
        results.push(get_contact_inner(pool, &id).await?);
    }
    Ok(results)
}

pub(crate) async fn update_contact_inner(
    pool: &SqlitePool,
    contact_id: &str,
    input: UpdateContactInput,
) -> Result<ContactWithEmails, String> {
    let existing = get_contact_inner(pool, contact_id).await?;
    let now = now_epoch();

    // Save old FTS values before they get consumed
    let old_display_name = existing.contact.display_name.clone();
    let old_company = existing.contact.company.clone();
    let old_job_title = existing.contact.job_title.clone();
    let old_notes = existing.contact.notes.clone();

    let display_name = input.display_name.unwrap_or(existing.contact.display_name);
    let given_name = if input.given_name.is_some() { input.given_name } else { existing.contact.given_name };
    let surname = if input.surname.is_some() { input.surname } else { existing.contact.surname };
    let nickname = if input.nickname.is_some() { input.nickname } else { existing.contact.nickname };
    let company = if input.company.is_some() { input.company } else { existing.contact.company };
    let job_title = if input.job_title.is_some() { input.job_title } else { existing.contact.job_title };
    let department = if input.department.is_some() { input.department } else { existing.contact.department };
    let notes = if input.notes.is_some() { input.notes } else { existing.contact.notes };
    let birthday = if input.birthday.is_some() { input.birthday } else { existing.contact.birthday };
    let photo_uri = if input.photo_uri.is_some() { input.photo_uri } else { existing.contact.photo_uri };
    let is_starred = input.is_starred.unwrap_or(existing.contact.is_starred);

    let phones_json = match input.phones {
        Some(p) => serde_json::to_string(&p).unwrap_or_else(|_| "[]".to_string()),
        None => existing.contact.phones,
    };
    let addresses_json = match input.addresses {
        Some(a) => serde_json::to_string(&a).unwrap_or_else(|_| "[]".to_string()),
        None => existing.contact.addresses,
    };
    let social_json = match input.social_profiles {
        Some(s) => serde_json::to_string(&s).unwrap_or_else(|_| "[]".to_string()),
        None => existing.contact.social_profiles,
    };
    let urls_json = match input.urls {
        Some(u) => serde_json::to_string(&u).unwrap_or_else(|_| "[]".to_string()),
        None => existing.contact.urls,
    };
    let relations_json = match input.relations {
        Some(r) => serde_json::to_string(&r).unwrap_or_else(|_| "[]".to_string()),
        None => existing.contact.relations,
    };

    sqlx::query(
        "UPDATE contacts SET display_name = ?, given_name = ?, surname = ?, nickname = ?, company = ?, job_title = ?, department = ?, notes = ?, birthday = ?, photo_uri = ?, phones = ?, addresses = ?, social_profiles = ?, urls = ?, relations = ?, is_starred = ?, updated_at = ? WHERE id = ?"
    )
    .bind(&display_name).bind(&given_name).bind(&surname).bind(&nickname)
    .bind(&company).bind(&job_title).bind(&department).bind(&notes)
    .bind(&birthday).bind(&photo_uri)
    .bind(&phones_json).bind(&addresses_json).bind(&social_json)
    .bind(&urls_json).bind(&relations_json).bind(is_starred).bind(now)
    .bind(contact_id)
    .execute(pool).await.map_err(|e| e.to_string())?;

    // Rebuild emails if provided
    if let Some(new_emails) = input.emails {
        sqlx::query("DELETE FROM contact_emails WHERE contact_id = ?")
            .bind(contact_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        for e_input in &new_emails {
            let eid = new_id();
            sqlx::query(
                "INSERT INTO contact_emails (id, contact_id, email, type, is_primary) VALUES (?, ?, ?, ?, ?)"
            )
            .bind(&eid).bind(contact_id).bind(&e_input.email).bind(&e_input.r#type).bind(e_input.is_primary)
            .execute(pool).await.map_err(|_| format!("Email '{}' already exists for another contact", e_input.email))?;
        }
    }

    // Rebuild groups if provided
    if let Some(new_groups) = input.groups {
        sqlx::query("DELETE FROM contact_group_members WHERE contact_id = ?")
            .bind(contact_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        for group_id in &new_groups {
            sqlx::query(
                "INSERT OR IGNORE INTO contact_group_members (contact_id, group_id) VALUES (?, ?)",
            )
            .bind(contact_id)
            .bind(group_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    // Update FTS: use special delete command for content-synced FTS5
    sqlx::query(
        "INSERT INTO contacts_fts (contacts_fts, rowid, display_name, company, job_title, notes) VALUES ('delete', (SELECT rowid FROM contacts WHERE id = ?), ?, ?, ?, ?)"
    )
    .bind(contact_id)
    .bind(&old_display_name)
    .bind(&old_company)
    .bind(&old_job_title)
    .bind(&old_notes)
    .execute(pool).await.map_err(|e| e.to_string())?;

    sqlx::query(
        "INSERT INTO contacts_fts (rowid, display_name, company, job_title, notes) VALUES ((SELECT rowid FROM contacts WHERE id = ?), ?, ?, ?, ?)"
    )
    .bind(contact_id).bind(&display_name).bind(&company).bind(&job_title).bind(&notes)
    .execute(pool).await.map_err(|e| e.to_string())?;

    get_contact_inner(pool, contact_id).await
}

pub(crate) async fn delete_contact_inner(
    pool: &SqlitePool,
    contact_id: &str,
) -> Result<(), String> {
    // Fetch the contact to get old values for FTS delete
    let contact: Contact = sqlx::query_as("SELECT * FROM contacts WHERE id = ?")
        .bind(contact_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Contact not found: {}", contact_id))?;

    // Delete FTS entry using special delete command for content-synced FTS5
    sqlx::query(
        "INSERT INTO contacts_fts (contacts_fts, rowid, display_name, company, job_title, notes) VALUES ('delete', (SELECT rowid FROM contacts WHERE id = ?), ?, ?, ?, ?)"
    )
    .bind(contact_id)
    .bind(&contact.display_name)
    .bind(&contact.company)
    .bind(&contact.job_title)
    .bind(&contact.notes)
    .execute(pool).await.map_err(|e| e.to_string())?;

    // Delete contact (ON DELETE CASCADE handles emails/groups)
    sqlx::query("DELETE FROM contacts WHERE id = ?")
        .bind(contact_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

// --- Tauri command wrappers ---

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
        None => {
            let account = super::accounts::get_active_account(pool.inner()).await?;
            account.id
        }
    };
    get_contacts_inner(
        pool.inner(),
        &acc_id,
        search.as_deref(),
        group_id.as_deref(),
        offset.unwrap_or(0),
        limit.unwrap_or(50),
    )
    .await
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::SqlitePool;
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

    #[tokio::test]
    async fn test_create_contact_basic() {
        let pool = test_pool().await;
        let input = CreateContactInput {
            display_name: "John Doe".to_string(),
            given_name: Some("John".to_string()),
            surname: Some("Doe".to_string()),
            company: Some("Acme Corp".to_string()),
            job_title: Some("Engineer".to_string()),
            emails: vec![
                EmailInput {
                    email: "john@acme.com".to_string(),
                    r#type: "work".to_string(),
                    is_primary: true,
                },
                EmailInput {
                    email: "john.doe@gmail.com".to_string(),
                    r#type: "personal".to_string(),
                    is_primary: false,
                },
            ],
            ..Default::default()
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
            emails: vec![EmailInput {
                email: "john@acme.com".to_string(),
                r#type: "work".to_string(),
                is_primary: true,
            }],
            ..Default::default()
        };
        create_contact_inner(&pool, "acc1", input1).await.unwrap();

        let input2 = CreateContactInput {
            display_name: "Johnny".to_string(),
            emails: vec![EmailInput {
                email: "john@acme.com".to_string(),
                r#type: "work".to_string(),
                is_primary: true,
            }],
            ..Default::default()
        };
        let result = create_contact_inner(&pool, "acc1", input2).await;
        assert!(result.is_err(), "Should reject duplicate email");
    }

    #[tokio::test]
    async fn test_get_contact_by_id() {
        let pool = test_pool().await;
        let input = CreateContactInput {
            display_name: "Jane Smith".to_string(),
            company: Some("BigCo".to_string()),
            emails: vec![EmailInput {
                email: "jane@bigco.com".to_string(),
                r#type: "work".to_string(),
                is_primary: true,
            }],
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
                emails: vec![EmailInput {
                    email: format!("c{}@test.com", i),
                    r#type: "work".to_string(),
                    is_primary: true,
                }],
                ..Default::default()
            };
            create_contact_inner(&pool, "acc1", input).await.unwrap();
        }
        let list = get_contacts_inner(&pool, "acc1", None, None, 0, 10)
            .await
            .unwrap();
        assert_eq!(list.len(), 5);
    }

    #[tokio::test]
    async fn test_get_contacts_with_search() {
        let pool = test_pool().await;
        create_contact_inner(
            &pool,
            "acc1",
            CreateContactInput {
                display_name: "Alice Wonder".to_string(),
                company: Some("Wonderland Inc".to_string()),
                emails: vec![EmailInput {
                    email: "alice@wonder.com".to_string(),
                    r#type: "work".to_string(),
                    is_primary: true,
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        create_contact_inner(
            &pool,
            "acc1",
            CreateContactInput {
                display_name: "Bob Builder".to_string(),
                emails: vec![EmailInput {
                    email: "bob@build.com".to_string(),
                    r#type: "work".to_string(),
                    is_primary: true,
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let results = get_contacts_inner(&pool, "acc1", Some("alice"), None, 0, 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].contact.display_name, "Alice Wonder");
    }

    #[tokio::test]
    async fn test_update_contact() {
        let pool = test_pool().await;
        let created = create_contact_inner(
            &pool,
            "acc1",
            CreateContactInput {
                display_name: "Old Name".to_string(),
                emails: vec![EmailInput {
                    email: "old@test.com".to_string(),
                    r#type: "work".to_string(),
                    is_primary: true,
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let updated = update_contact_inner(
            &pool,
            &created.contact.id,
            UpdateContactInput {
                display_name: Some("New Name".to_string()),
                company: Some("New Corp".to_string()),
                emails: Some(vec![EmailInput {
                    email: "new@test.com".to_string(),
                    r#type: "work".to_string(),
                    is_primary: true,
                }]),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(updated.contact.display_name, "New Name");
        assert_eq!(updated.contact.company, Some("New Corp".to_string()));
        assert_eq!(updated.emails.len(), 1);
        assert_eq!(updated.emails[0].email, "new@test.com");
    }

    #[tokio::test]
    async fn test_delete_contact() {
        let pool = test_pool().await;
        let created = create_contact_inner(
            &pool,
            "acc1",
            CreateContactInput {
                display_name: "To Delete".to_string(),
                emails: vec![EmailInput {
                    email: "del@test.com".to_string(),
                    r#type: "work".to_string(),
                    is_primary: true,
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();

        delete_contact_inner(&pool, &created.contact.id).await.unwrap();
        let result = get_contact_inner(&pool, &created.contact.id).await;
        assert!(result.is_err());
    }
}
