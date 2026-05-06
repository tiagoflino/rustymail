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

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

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
    .execute(&mut *tx).await.map_err(|e| e.to_string())?;

    // Insert emails
    let mut emails = Vec::new();
    for e_input in &input.emails {
        let eid = new_id();
        sqlx::query(
            "INSERT INTO contact_emails (id, contact_id, email, type, is_primary) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&eid).bind(&id).bind(&e_input.email).bind(&e_input.r#type).bind(e_input.is_primary)
        .execute(&mut *tx).await.map_err(|_| format!("Email '{}' already exists for another contact", e_input.email))?;

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
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    // Update FTS
    sqlx::query(
        "INSERT INTO contacts_fts (rowid, display_name, company, job_title, notes) VALUES ((SELECT rowid FROM contacts WHERE id = ?), ?, ?, ?, ?)"
    )
    .bind(&id).bind(&input.display_name).bind(&input.company).bind(&input.job_title).bind(&input.notes)
    .execute(&mut *tx).await.map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

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
        let fts_query = format!("\"{}\"*", sanitize_fts_query(query));
        let fts_ids: Result<Vec<(String,)>, _> = sqlx::query_as(
            "SELECT c.id FROM contacts c WHERE c.account_id = ? AND (c.is_promoted = 1 OR c.source != 'discovered') AND c.rowid IN (SELECT rowid FROM contacts_fts WHERE contacts_fts MATCH ?) ORDER BY c.display_name ASC LIMIT ? OFFSET ?"
        )
        .bind(account_id)
        .bind(fts_query)
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
                    "SELECT id FROM contacts WHERE account_id = ? AND (is_promoted = 1 OR source != 'discovered') AND display_name LIKE ? ORDER BY display_name ASC LIMIT ? OFFSET ?"
                )
                .bind(account_id)
                .bind(like_pattern)
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
            "SELECT DISTINCT ce.contact_id FROM contact_emails ce JOIN contacts c ON c.id = ce.contact_id WHERE c.account_id = ? AND (c.is_promoted = 1 OR c.source != 'discovered') AND ce.email LIKE ? LIMIT ?"
        )
        .bind(account_id)
        .bind(email_pattern)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        for (id,) in email_ids {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }

        ids.truncate(limit as usize);
        ids
    } else if let Some(gid) = group_id {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT c.id FROM contacts c JOIN contact_group_members m ON c.id = m.contact_id WHERE c.account_id = ? AND (c.is_promoted = 1 OR c.source != 'discovered') AND m.group_id = ? ORDER BY c.display_name ASC LIMIT ? OFFSET ?"
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
            "SELECT id FROM contacts WHERE account_id = ? AND (is_promoted = 1 OR source != 'discovered') ORDER BY display_name ASC LIMIT ? OFFSET ?"
        )
        .bind(account_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
        rows.into_iter().map(|(id,)| id).collect()
    };

    if contact_ids.is_empty() {
        return Ok(vec![]);
    }

    let placeholders = contact_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    // Batch fetch contacts
    let contacts_query = format!("SELECT * FROM contacts WHERE id IN ({})", placeholders);
    let mut contacts_q = sqlx::query_as::<_, Contact>(&contacts_query);
    for id in &contact_ids {
        contacts_q = contacts_q.bind(id.as_str());
    }
    let contacts: Vec<Contact> = contacts_q.fetch_all(pool).await.map_err(|e| e.to_string())?;

    // Batch fetch emails
    let email_query = format!("SELECT * FROM contact_emails WHERE contact_id IN ({}) ORDER BY is_primary DESC", placeholders);
    let mut email_q = sqlx::query_as::<_, ContactEmail>(&email_query);
    for id in &contact_ids {
        email_q = email_q.bind(id.as_str());
    }
    let all_emails: Vec<ContactEmail> = email_q.fetch_all(pool).await.unwrap_or_default();

    // Batch fetch groups
    let group_query = format!("SELECT m.contact_id, g.name FROM contact_groups g JOIN contact_group_members m ON g.id = m.group_id WHERE m.contact_id IN ({})", placeholders);
    let mut group_q = sqlx::query_as::<_, (String, String)>(&group_query);
    for id in &contact_ids {
        group_q = group_q.bind(id.as_str());
    }
    let all_groups: Vec<(String, String)> = group_q.fetch_all(pool).await.unwrap_or_default();

    // Assemble results preserving original order
    let mut result = Vec::new();
    for cid in &contact_ids {
        if let Some(contact) = contacts.iter().find(|c| &c.id == cid) {
            let emails: Vec<ContactEmail> = all_emails.iter().filter(|e| e.contact_id == *cid).cloned().collect();
            let groups: Vec<String> = all_groups.iter().filter(|g| g.0 == *cid).map(|g| g.1.clone()).collect();
            result.push(ContactWithEmails { contact: contact.clone(), emails, groups });
        }
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

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    sqlx::query(
        "UPDATE contacts SET display_name = ?, given_name = ?, surname = ?, nickname = ?, company = ?, job_title = ?, department = ?, notes = ?, birthday = ?, photo_uri = ?, phones = ?, addresses = ?, social_profiles = ?, urls = ?, relations = ?, is_starred = ?, updated_at = ? WHERE id = ?"
    )
    .bind(&display_name).bind(&given_name).bind(&surname).bind(&nickname)
    .bind(&company).bind(&job_title).bind(&department).bind(&notes)
    .bind(&birthday).bind(&photo_uri)
    .bind(&phones_json).bind(&addresses_json).bind(&social_json)
    .bind(&urls_json).bind(&relations_json).bind(is_starred).bind(now)
    .bind(contact_id)
    .execute(&mut *tx).await.map_err(|e| e.to_string())?;

    // Rebuild emails if provided
    if let Some(new_emails) = input.emails {
        sqlx::query("DELETE FROM contact_emails WHERE contact_id = ?")
            .bind(contact_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        for e_input in &new_emails {
            let eid = new_id();
            sqlx::query(
                "INSERT INTO contact_emails (id, contact_id, email, type, is_primary) VALUES (?, ?, ?, ?, ?)"
            )
            .bind(&eid).bind(contact_id).bind(&e_input.email).bind(&e_input.r#type).bind(e_input.is_primary)
            .execute(&mut *tx).await.map_err(|_| format!("Email '{}' already exists for another contact", e_input.email))?;
        }
    }

    // Rebuild groups if provided
    if let Some(new_groups) = input.groups {
        sqlx::query("DELETE FROM contact_group_members WHERE contact_id = ?")
            .bind(contact_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        for group_id in &new_groups {
            sqlx::query(
                "INSERT OR IGNORE INTO contact_group_members (contact_id, group_id) VALUES (?, ?)",
            )
            .bind(contact_id)
            .bind(group_id)
            .execute(&mut *tx)
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
    .execute(&mut *tx).await.map_err(|e| e.to_string())?;

    sqlx::query(
        "INSERT INTO contacts_fts (rowid, display_name, company, job_title, notes) VALUES ((SELECT rowid FROM contacts WHERE id = ?), ?, ?, ?, ?)"
    )
    .bind(contact_id).bind(&display_name).bind(&company).bind(&job_title).bind(&notes)
    .execute(&mut *tx).await.map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

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

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    // Delete FTS entry using special delete command for content-synced FTS5
    sqlx::query(
        "INSERT INTO contacts_fts (contacts_fts, rowid, display_name, company, job_title, notes) VALUES ('delete', (SELECT rowid FROM contacts WHERE id = ?), ?, ?, ?, ?)"
    )
    .bind(contact_id)
    .bind(&contact.display_name)
    .bind(&contact.company)
    .bind(&contact.job_title)
    .bind(&contact.notes)
    .execute(&mut *tx).await.map_err(|e| e.to_string())?;

    // Delete contact (ON DELETE CASCADE handles emails/groups)
    sqlx::query("DELETE FROM contacts WHERE id = ?")
        .bind(contact_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(())
}

// --- Merge contacts ---

pub(crate) async fn merge_contacts_inner(
    pool: &SqlitePool,
    primary_id: &str,
    secondary_id: &str,
) -> Result<ContactWithEmails, String> {
    if primary_id == secondary_id {
        return Err("Cannot merge a contact with itself".to_string());
    }

    let primary = get_contact_inner(pool, primary_id).await?;
    let secondary = get_contact_inner(pool, secondary_id).await?;

    // Save old FTS values before merge
    let old_display_name = primary.contact.display_name.clone();
    let old_company = primary.contact.company.clone();
    let old_job_title = primary.contact.job_title.clone();
    let old_notes = primary.contact.notes.clone();

    // Merge text fields: primary wins if non-null, else fill from secondary
    let company = primary.contact.company.or(secondary.contact.company);
    let job_title = primary.contact.job_title.or(secondary.contact.job_title);
    let department = primary.contact.department.or(secondary.contact.department);
    let nickname = primary.contact.nickname.or(secondary.contact.nickname);
    let birthday = primary.contact.birthday.or(secondary.contact.birthday);
    let photo_uri = primary.contact.photo_uri.or(secondary.contact.photo_uri);

    // Notes: concatenate if both exist
    let notes = match (primary.contact.notes, secondary.contact.notes) {
        (Some(p), Some(s)) => Some(format!("{}\n{}", p, s)),
        (Some(p), None) => Some(p),
        (None, Some(s)) => Some(s),
        (None, None) => None,
    };

    // JSON array fields: parse both, concatenate
    let phones = merge_json_arrays(&primary.contact.phones, &secondary.contact.phones);
    let addresses = merge_json_arrays(&primary.contact.addresses, &secondary.contact.addresses);
    let social_profiles =
        merge_json_arrays(&primary.contact.social_profiles, &secondary.contact.social_profiles);
    let urls = merge_json_arrays(&primary.contact.urls, &secondary.contact.urls);
    let relations = merge_json_arrays(&primary.contact.relations, &secondary.contact.relations);

    let now = now_epoch();

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    // Update primary contact with merged fields
    sqlx::query(
        "UPDATE contacts SET company = ?, job_title = ?, department = ?, nickname = ?, notes = ?, birthday = ?, photo_uri = ?, phones = ?, addresses = ?, social_profiles = ?, urls = ?, relations = ?, updated_at = ? WHERE id = ?"
    )
    .bind(&company).bind(&job_title).bind(&department).bind(&nickname)
    .bind(&notes).bind(&birthday).bind(&photo_uri)
    .bind(&phones).bind(&addresses).bind(&social_profiles)
    .bind(&urls).bind(&relations).bind(now)
    .bind(primary_id)
    .execute(&mut *tx).await.map_err(|e| e.to_string())?;

    // Move secondary's emails to primary (skip duplicates based on case-insensitive match)
    let primary_emails: Vec<String> = primary
        .emails
        .iter()
        .map(|e| e.email.to_lowercase())
        .collect();

    for email in &secondary.emails {
        if !primary_emails.contains(&email.email.to_lowercase()) {
            let new_eid = new_id();
            // Delete the old email row (unique constraint on email) then insert with new contact_id
            sqlx::query("DELETE FROM contact_emails WHERE id = ?")
                .bind(&email.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(
                "INSERT INTO contact_emails (id, contact_id, email, type, is_primary) VALUES (?, ?, ?, ?, ?)"
            )
            .bind(&new_eid).bind(primary_id).bind(&email.email).bind(&email.r#type).bind(false)
            .execute(&mut *tx).await.map_err(|e| e.to_string())?;
        }
    }

    // Move provider links from secondary to primary
    sqlx::query("UPDATE OR IGNORE contact_provider_links SET contact_id = ? WHERE contact_id = ?")
        .bind(primary_id)
        .bind(secondary_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    // Move group memberships from secondary to primary
    sqlx::query(
        "INSERT OR IGNORE INTO contact_group_members (contact_id, group_id) SELECT ?, group_id FROM contact_group_members WHERE contact_id = ?"
    )
    .bind(primary_id).bind(secondary_id)
    .execute(&mut *tx).await.map_err(|e| e.to_string())?;

    // Delete the secondary contact's FTS entry
    let sec_contact: Contact = sqlx::query_as("SELECT * FROM contacts WHERE id = ?")
        .bind(secondary_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query(
        "INSERT INTO contacts_fts (contacts_fts, rowid, display_name, company, job_title, notes) VALUES ('delete', (SELECT rowid FROM contacts WHERE id = ?), ?, ?, ?, ?)"
    )
    .bind(secondary_id)
    .bind(&sec_contact.display_name)
    .bind(&sec_contact.company)
    .bind(&sec_contact.job_title)
    .bind(&sec_contact.notes)
    .execute(&mut *tx).await.map_err(|e| e.to_string())?;

    // Delete the secondary contact (cascade deletes remaining emails, groups, provider links)
    sqlx::query("DELETE FROM contacts WHERE id = ?")
        .bind(secondary_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    // Update FTS for primary: delete old entry then insert new
    sqlx::query(
        "INSERT INTO contacts_fts (contacts_fts, rowid, display_name, company, job_title, notes) VALUES ('delete', (SELECT rowid FROM contacts WHERE id = ?), ?, ?, ?, ?)"
    )
    .bind(primary_id)
    .bind(&old_display_name)
    .bind(&old_company)
    .bind(&old_job_title)
    .bind(&old_notes)
    .execute(&mut *tx).await.map_err(|e| e.to_string())?;

    sqlx::query(
        "INSERT INTO contacts_fts (rowid, display_name, company, job_title, notes) VALUES ((SELECT rowid FROM contacts WHERE id = ?), ?, ?, ?, ?)"
    )
    .bind(primary_id)
    .bind(&primary.contact.display_name)
    .bind(&company)
    .bind(&job_title)
    .bind(&notes)
    .execute(&mut *tx).await.map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

    get_contact_inner(pool, primary_id).await
}

fn escape_csv_field(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' if !in_quotes => {
                in_quotes = true;
            }
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            }
            ',' if !in_quotes => {
                fields.push(current.trim().to_string());
                current = String::new();
            }
            _ => {
                current.push(c);
            }
        }
    }
    fields.push(current.trim().to_string());
    fields
}

fn sanitize_fts_query(query: &str) -> String {
    query
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '@' || *c == '.' || *c == '-' || *c == '_')
        .collect()
}

fn merge_json_arrays(a: &str, b: &str) -> String {
    let mut arr_a: Vec<serde_json::Value> =
        serde_json::from_str(a).unwrap_or_default();
    let arr_b: Vec<serde_json::Value> =
        serde_json::from_str(b).unwrap_or_default();
    arr_a.extend(arr_b);
    serde_json::to_string(&arr_a).unwrap_or_else(|_| "[]".to_string())
}

// --- ContactGroup type ---

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ContactGroup {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub color: Option<String>,
    pub remote_id: Option<String>,
    pub created_at: i64,
}

// --- Group inner functions ---

pub(crate) async fn create_group_inner(
    pool: &SqlitePool,
    account_id: &str,
    name: &str,
    color: Option<&str>,
) -> Result<ContactGroup, String> {
    let id = new_id();
    let now = now_epoch();

    sqlx::query(
        "INSERT INTO contact_groups (id, account_id, name, color, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(account_id)
    .bind(name)
    .bind(color)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(ContactGroup {
        id,
        account_id: account_id.to_string(),
        name: name.to_string(),
        color: color.map(|c| c.to_string()),
        remote_id: None,
        created_at: now,
    })
}

pub(crate) async fn get_groups_inner(
    pool: &SqlitePool,
    account_id: &str,
) -> Result<Vec<ContactGroup>, String> {
    sqlx::query_as::<_, ContactGroup>(
        "SELECT id, account_id, name, color, remote_id, created_at FROM contact_groups WHERE account_id = ? ORDER BY name ASC",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

pub(crate) async fn update_group_inner(
    pool: &SqlitePool,
    group_id: &str,
    name: Option<&str>,
    color: Option<&str>,
) -> Result<ContactGroup, String> {
    let existing: ContactGroup = sqlx::query_as(
        "SELECT id, account_id, name, color, remote_id, created_at FROM contact_groups WHERE id = ?",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("Group not found: {}", group_id))?;

    let new_name = name.unwrap_or(&existing.name);
    let new_color = if color.is_some() { color } else { existing.color.as_deref() };

    sqlx::query("UPDATE contact_groups SET name = ?, color = ? WHERE id = ?")
        .bind(new_name)
        .bind(new_color)
        .bind(group_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ContactGroup {
        id: existing.id,
        account_id: existing.account_id,
        name: new_name.to_string(),
        color: new_color.map(|c| c.to_string()),
        remote_id: existing.remote_id,
        created_at: existing.created_at,
    })
}

pub(crate) async fn delete_group_inner(
    pool: &SqlitePool,
    group_id: &str,
) -> Result<(), String> {
    sqlx::query("DELETE FROM contact_group_members WHERE group_id = ?")
        .bind(group_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query("DELETE FROM contact_groups WHERE id = ?")
        .bind(group_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub(crate) async fn set_contact_groups_inner(
    pool: &SqlitePool,
    contact_id: &str,
    group_ids: Vec<String>,
) -> Result<(), String> {
    sqlx::query("DELETE FROM contact_group_members WHERE contact_id = ?")
        .bind(contact_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    for group_id in &group_ids {
        sqlx::query(
            "INSERT OR IGNORE INTO contact_group_members (contact_id, group_id) VALUES (?, ?)",
        )
        .bind(contact_id)
        .bind(group_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

// --- Autocomplete search ---

pub(crate) async fn search_contacts_autocomplete(
    pool: &SqlitePool,
    account_id: &str,
    query: &str,
) -> Result<Vec<super::compose::ContactSuggestion>, String> {
    use super::compose::ContactSuggestion;
    let mut suggestions: Vec<ContactSuggestion> = Vec::new();
    let mut seen_emails: std::collections::HashSet<String> = std::collections::HashSet::new();
    let pattern = format!("%{}%", query);

    // 1. Search contact_emails + contact name (fast indexed path)
    let email_results: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.display_name, ce.email FROM contact_emails ce JOIN contacts c ON c.id = ce.contact_id WHERE c.account_id = ? AND (ce.email LIKE ? OR c.display_name LIKE ?) ORDER BY ce.is_primary DESC LIMIT 10"
    )
    .bind(account_id)
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for (name, email) in email_results {
        let lower = email.to_lowercase();
        if seen_emails.insert(lower) {
            let raw = if name.is_empty() {
                email.clone()
            } else {
                format!("{} <{}>", name, email)
            };
            suggestions.push(ContactSuggestion { name, email, raw });
        }
    }

    // 2. If under 10 results, search FTS for partial name/company matches
    if suggestions.len() < 10 {
        let remaining = 10 - suggestions.len() as i64;
        let fts_results: Vec<(String, String)> = sqlx::query_as(
            "SELECT c.display_name, ce.email FROM contacts c JOIN contacts_fts f ON c.rowid = f.rowid JOIN contact_emails ce ON ce.contact_id = c.id WHERE f.contacts_fts MATCH ? AND c.account_id = ? AND ce.is_primary = 1 LIMIT ?"
        )
        .bind(format!("\"{}\"*", sanitize_fts_query(query)))
        .bind(account_id)
        .bind(remaining)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for (name, email) in fts_results {
            let lower = email.to_lowercase();
            if seen_emails.insert(lower) {
                let raw = if name.is_empty() {
                    email.clone()
                } else {
                    format!("{} <{}>", name, email)
                };
                suggestions.push(ContactSuggestion { name, email, raw });
            }
        }
    }

    // 3. Fall back to message history for contacts not yet in store
    if suggestions.len() < 10 {
        let legacy = super::compose::search_contacts_inner(pool, account_id, query)
            .await
            .unwrap_or_default();
        for s in legacy {
            let lower = s.email.to_lowercase();
            if seen_emails.insert(lower) {
                suggestions.push(s);
            }
            if suggestions.len() >= 10 {
                break;
            }
        }
    }

    suggestions.truncate(10);
    Ok(suggestions)
}

// --- Import/Export ---

pub(crate) async fn import_vcard_inner(
    pool: &SqlitePool,
    account_id: &str,
    data: &str,
) -> Result<Vec<ContactWithEmails>, String> {
    let mut imported = Vec::new();

    let cards: Vec<&str> = data.split("END:VCARD").collect();
    for card in cards {
        let card = card.trim();
        if card.is_empty() || !card.contains("BEGIN:VCARD") {
            continue;
        }

        let mut display_name = String::new();
        let mut given_name: Option<String> = None;
        let mut surname: Option<String> = None;
        let mut emails: Vec<EmailInput> = Vec::new();
        let mut phones: Vec<serde_json::Value> = Vec::new();
        let mut company: Option<String> = None;
        let mut job_title: Option<String> = None;

        for line in card.lines() {
            let line = line.trim_end_matches('\r');
            if let Some(val) = line.strip_prefix("FN:") {
                display_name = val.to_string();
            } else if let Some(val) = line.strip_prefix("N:") {
                let parts: Vec<&str> = val.splitn(5, ';').collect();
                if parts.len() >= 2 {
                    let fam = parts[0].trim();
                    let giv = parts[1].trim();
                    if !fam.is_empty() {
                        surname = Some(fam.to_string());
                    }
                    if !giv.is_empty() {
                        given_name = Some(giv.to_string());
                    }
                }
            } else if line.contains("EMAIL") && line.contains(':') {
                let colon_pos = line.find(':').unwrap();
                let email_value = line[colon_pos + 1..].trim().to_string();
                let prefix = &line[..colon_pos].to_uppercase();
                let email_type = if prefix.contains("WORK") {
                    "work"
                } else if prefix.contains("HOME") {
                    "home"
                } else {
                    "other"
                };
                let is_primary = emails.is_empty();
                emails.push(EmailInput {
                    email: email_value,
                    r#type: email_type.to_string(),
                    is_primary,
                });
            } else if line.contains("TEL") && line.contains(':') {
                let colon_pos = line.find(':').unwrap();
                let phone_value = line[colon_pos + 1..].trim().to_string();
                let prefix = &line[..colon_pos].to_uppercase();
                let phone_type = if prefix.contains("CELL") || prefix.contains("MOBILE") {
                    "mobile"
                } else if prefix.contains("WORK") {
                    "work"
                } else if prefix.contains("HOME") {
                    "home"
                } else {
                    "other"
                };
                phones.push(serde_json::json!({
                    "number": phone_value,
                    "type": phone_type,
                }));
            } else if let Some(val) = line.strip_prefix("ORG:") {
                let val = val.trim_end_matches(';').trim().to_string();
                if !val.is_empty() {
                    company = Some(val);
                }
            } else if let Some(val) = line.strip_prefix("TITLE:") {
                let val = val.trim().to_string();
                if !val.is_empty() {
                    job_title = Some(val);
                }
            }
        }

        // Fallback display name to first email
        if display_name.is_empty() {
            if let Some(first_email) = emails.first() {
                display_name = first_email.email.clone();
            }
        }

        // Skip entries with no display name and no emails
        if display_name.is_empty() && emails.is_empty() {
            continue;
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
            Ok(contact) => imported.push(contact),
            Err(_) => continue, // Skip duplicates
        }
    }

    Ok(imported)
}

pub(crate) async fn import_csv_inner(
    pool: &SqlitePool,
    account_id: &str,
    data: &str,
) -> Result<Vec<ContactWithEmails>, String> {
    let mut lines = data.lines();
    let header_line = match lines.next() {
        Some(h) => h,
        None => return Ok(Vec::new()),
    };

    // Parse headers with case-insensitive matching
    let headers: Vec<String> = parse_csv_line(header_line).iter().map(|h| h.to_lowercase()).collect();

    let name_col = headers.iter().position(|h| {
        h == "name" || h == "full name" || h == "display name" || h == "display_name"
    });
    let email_col = headers.iter().position(|h| {
        h == "email" || h == "e-mail" || h == "email address"
    });
    let phone_col = headers.iter().position(|h| {
        h == "phone" || h == "telephone" || h == "mobile" || h == "phone number"
    });
    let company_col = headers.iter().position(|h| {
        h == "company" || h == "organization" || h == "org" || h == "organisation"
    });
    let title_col = headers.iter().position(|h| {
        h == "title" || h == "job title" || h == "job_title" || h == "position"
    });

    let mut imported = Vec::new();

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let fields = parse_csv_line(line);

        let display_name = name_col
            .and_then(|i| fields.get(i))
            .map(|s| s.to_string())
            .unwrap_or_default();

        if display_name.is_empty() {
            continue;
        }

        let email = email_col
            .and_then(|i| fields.get(i))
            .map(|s| s.to_string())
            .unwrap_or_default();

        let phone = phone_col
            .and_then(|i| fields.get(i))
            .map(|s| s.to_string())
            .unwrap_or_default();

        let company = company_col
            .and_then(|i| fields.get(i))
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        let job_title = title_col
            .and_then(|i| fields.get(i))
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        let emails = if email.is_empty() {
            Vec::new()
        } else {
            vec![EmailInput {
                email,
                r#type: "other".to_string(),
                is_primary: true,
            }]
        };

        let phones = if phone.is_empty() {
            Vec::new()
        } else {
            vec![serde_json::json!({ "number": phone, "type": "other" })]
        };

        let input = CreateContactInput {
            display_name,
            company,
            job_title,
            emails,
            phones,
            ..Default::default()
        };

        match create_contact_inner(pool, account_id, input).await {
            Ok(contact) => imported.push(contact),
            Err(_) => continue,
        }
    }

    Ok(imported)
}

pub(crate) async fn export_vcard_inner(
    pool: &SqlitePool,
    account_id: &str,
    contact_ids: Option<Vec<String>>,
) -> Result<String, String> {
    let contacts = match contact_ids {
        Some(ids) => {
            let mut results = Vec::new();
            for id in ids {
                results.push(get_contact_inner(pool, &id).await?);
            }
            results
        }
        None => get_contacts_inner(pool, account_id, None, None, 0, 10000).await?,
    };

    let mut output = String::new();

    for c in &contacts {
        output.push_str("BEGIN:VCARD\r\n");
        output.push_str("VERSION:3.0\r\n");
        output.push_str(&format!("FN:{}\r\n", c.contact.display_name));

        let surname = c.contact.surname.as_deref().unwrap_or("");
        let given = c.contact.given_name.as_deref().unwrap_or("");
        output.push_str(&format!("N:{};{};;;\r\n", surname, given));

        for email in &c.emails {
            output.push_str(&format!(
                "EMAIL;TYPE={}:{}\r\n",
                email.r#type.to_uppercase(),
                email.email
            ));
        }

        let phones: Vec<serde_json::Value> =
            serde_json::from_str(&c.contact.phones).unwrap_or_default();
        for phone in &phones {
            let number = phone.get("number").and_then(|v| v.as_str()).unwrap_or("");
            let ptype = phone
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("OTHER")
                .to_uppercase();
            if !number.is_empty() {
                output.push_str(&format!("TEL;TYPE={}:{}\r\n", ptype, number));
            }
        }

        if let Some(ref org) = c.contact.company {
            output.push_str(&format!("ORG:{}\r\n", org));
        }

        if let Some(ref title) = c.contact.job_title {
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
    let contacts = match contact_ids {
        Some(ids) => {
            let mut results = Vec::new();
            for id in ids {
                results.push(get_contact_inner(pool, &id).await?);
            }
            results
        }
        None => get_contacts_inner(pool, account_id, None, None, 0, 10000).await?,
    };

    let mut output = String::from("Name,Email,Phone,Company,Title\n");

    for c in &contacts {
        let email = c
            .emails
            .iter()
            .find(|e| e.is_primary)
            .or_else(|| c.emails.first())
            .map(|e| e.email.as_str())
            .unwrap_or("");

        let phones: Vec<serde_json::Value> =
            serde_json::from_str(&c.contact.phones).unwrap_or_default();
        let phone = phones
            .first()
            .and_then(|p| p.get("number"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let company = c.contact.company.as_deref().unwrap_or("");
        let title = c.contact.job_title.as_deref().unwrap_or("");

        output.push_str(&format!(
            "{},{},{},{},{}\n",
            escape_csv_field(&c.contact.display_name),
            escape_csv_field(email),
            escape_csv_field(phone),
            escape_csv_field(company),
            escape_csv_field(title)
        ));
    }

    Ok(output)
}

// --- Sync ---

pub(crate) async fn sync_contacts_inner(
    pool: &SqlitePool,
    account_id: &str,
) -> Result<usize, String> {
    // Get provider type
    let provider_type: String = sqlx::query_scalar(
        "SELECT provider_type FROM accounts WHERE id = ?",
    )
    .bind(account_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    // Check throttle (15 min = 900 seconds)
    let last_sync: Option<i64> = sqlx::query_scalar(
        "SELECT last_full_sync FROM contacts_sync_state WHERE account_id = ?",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let now = now_epoch();
    if let Some(last) = last_sync {
        if now - last < 900 {
            return Ok(0); // Throttled
        }
    }

    // Get access token via account lookup (handles token refresh)
    let account = super::accounts::get_account_by_id(pool, account_id).await?;

    match provider_type.as_str() {
        "gmail" => {
            crate::contacts::google_contacts::sync_google_contacts(
                pool,
                account_id,
                &account.access_token,
            )
            .await
        }
        "outlook" => {
            crate::contacts::outlook_contacts::sync_outlook_contacts(
                pool,
                account_id,
                &account.access_token,
            )
            .await
        }
        _ => Ok(0), // IMAP/CardDAV not yet implemented
    }
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

#[tauri::command]
pub async fn search_contacts_v2(
    app_handle: tauri::AppHandle,
    query: String,
    account_id: Option<String>,
) -> Result<Vec<super::compose::ContactSuggestion>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let acc_id = match account_id {
        Some(id) => id,
        None => super::accounts::get_active_account(pool.inner()).await?.id,
    };
    search_contacts_autocomplete(pool.inner(), &acc_id, &query).await
}

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

#[tauri::command]
pub async fn merge_contacts(
    app_handle: tauri::AppHandle,
    primary_id: String,
    secondary_id: String,
) -> Result<ContactWithEmails, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    merge_contacts_inner(pool.inner(), &primary_id, &secondary_id).await
}

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
    match format.to_lowercase().as_str() {
        "vcard" | "vcf" => import_vcard_inner(pool.inner(), &acc_id, &data).await,
        "csv" => import_csv_inner(pool.inner(), &acc_id, &data).await,
        _ => Err(format!("Unsupported format: {}", format)),
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
    match format.to_lowercase().as_str() {
        "vcard" | "vcf" => export_vcard_inner(pool.inner(), &acc_id, contact_ids).await,
        "csv" => export_csv_inner(pool.inner(), &acc_id, contact_ids).await,
        _ => Err(format!("Unsupported format: {}", format)),
    }
}

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
    sync_contacts_inner(pool.inner(), &acc_id).await
}

#[tauri::command]
pub async fn backfill_contacts(
    app_handle: tauri::AppHandle,
    account_id: Option<String>,
) -> Result<usize, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let acc = match account_id {
        Some(id) => super::accounts::get_account_by_id(pool.inner(), &id).await?,
        None => super::accounts::get_active_account(pool.inner()).await?,
    };
    let email: String = sqlx::query_scalar("SELECT email FROM accounts WHERE id = ?")
        .bind(&acc.id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
    crate::contacts::discovery::backfill_discovered_contacts(pool.inner(), &acc.id, &email).await
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

    #[tokio::test]
    async fn test_search_contacts_autocomplete() {
        let pool = test_pool().await;

        // Create contacts in store
        create_contact_inner(
            &pool,
            "acc1",
            CreateContactInput {
                display_name: "Alice Anderson".to_string(),
                company: Some("Tech Corp".to_string()),
                emails: vec![EmailInput {
                    email: "alice@techcorp.com".to_string(),
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
                    email: "bob@builder.io".to_string(),
                    r#type: "work".to_string(),
                    is_primary: true,
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // Also insert a message from an unknown sender (legacy fallback)
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id) VALUES ('t1', 'acc1', '', '')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_html, body_plain) VALUES ('m1', 't1', 'acc1', 'Charlie <charlie@unknown.com>', '', 'Hi', '', 1000, '', '')")
            .execute(&pool)
            .await
            .unwrap();

        // Search should find Alice from contacts
        let results = search_contacts_autocomplete(&pool, "acc1", "ali").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].email, "alice@techcorp.com");
        assert_eq!(results[0].name, "Alice Anderson");

        // Search by email domain
        let results = search_contacts_autocomplete(&pool, "acc1", "techcorp").await.unwrap();
        assert_eq!(results.len(), 1);

        // Search should also find legacy message senders
        let results = search_contacts_autocomplete(&pool, "acc1", "charlie").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].email, "charlie@unknown.com");
    }

    #[tokio::test]
    async fn test_create_and_list_groups() {
        let pool = test_pool().await;
        let group = create_group_inner(&pool, "acc1", "VIP Clients", Some("#ff0000"))
            .await
            .unwrap();
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
        let contact = create_contact_inner(
            &pool,
            "acc1",
            CreateContactInput {
                display_name: "Member".to_string(),
                emails: vec![EmailInput {
                    email: "member@team.com".to_string(),
                    r#type: "work".to_string(),
                    is_primary: true,
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();

        set_contact_groups_inner(&pool, &contact.contact.id, vec![group.id.clone()])
            .await
            .unwrap();

        let fetched = get_contact_inner(&pool, &contact.contact.id).await.unwrap();
        assert_eq!(fetched.groups, vec!["Team"]);

        // Also test that group filter works in get_contacts_inner
        let in_group = get_contacts_inner(&pool, "acc1", None, Some(&group.id), 0, 10)
            .await
            .unwrap();
        assert_eq!(in_group.len(), 1);
    }

    #[tokio::test]
    async fn test_update_group() {
        let pool = test_pool().await;
        let group = create_group_inner(&pool, "acc1", "Old Name", Some("#000000"))
            .await
            .unwrap();

        let updated = update_group_inner(&pool, &group.id, Some("New Name"), Some("#ffffff"))
            .await
            .unwrap();
        assert_eq!(updated.name, "New Name");
        assert_eq!(updated.color, Some("#ffffff".to_string()));
    }

    #[tokio::test]
    async fn test_delete_group() {
        let pool = test_pool().await;
        let group = create_group_inner(&pool, "acc1", "Temp", None).await.unwrap();
        delete_group_inner(&pool, &group.id).await.unwrap();
        let groups = get_groups_inner(&pool, "acc1").await.unwrap();
        assert_eq!(groups.len(), 0);
    }

    #[tokio::test]
    async fn test_merge_contacts() {
        let pool = test_pool().await;
        let c1 = create_contact_inner(
            &pool,
            "acc1",
            CreateContactInput {
                display_name: "John Doe".to_string(),
                company: Some("Acme".to_string()),
                emails: vec![EmailInput {
                    email: "john@acme.com".to_string(),
                    r#type: "work".to_string(),
                    is_primary: true,
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let c2 = create_contact_inner(
            &pool,
            "acc1",
            CreateContactInput {
                display_name: "J. Doe".to_string(),
                job_title: Some("Engineer".to_string()),
                emails: vec![EmailInput {
                    email: "jdoe@personal.com".to_string(),
                    r#type: "personal".to_string(),
                    is_primary: true,
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let merged = merge_contacts_inner(&pool, &c1.contact.id, &c2.contact.id)
            .await
            .unwrap();

        assert_eq!(merged.contact.display_name, "John Doe");
        assert_eq!(merged.contact.company, Some("Acme".to_string()));
        assert_eq!(merged.contact.job_title, Some("Engineer".to_string()));
        assert_eq!(merged.emails.len(), 2);

        // Secondary should be deleted
        let result = get_contact_inner(&pool, &c2.contact.id).await;
        assert!(result.is_err());
    }

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
        assert!(csv.contains("\"CSV User\",\"csv@test.com\","));
    }

    #[tokio::test]
    async fn test_get_contacts_hides_unpromoted_discovered() {
        let pool = test_pool().await;
        // Create a normal contact (visible)
        create_contact_inner(&pool, "acc1", CreateContactInput {
            display_name: "Visible".to_string(),
            emails: vec![EmailInput { email: "visible@test.com".into(), r#type: "work".into(), is_primary: true }],
            ..Default::default()
        }).await.unwrap();

        // Create a discovered unpromoted contact (hidden from list)
        sqlx::query("INSERT INTO contacts (id, account_id, display_name, phones, addresses, social_profiles, urls, relations, source, is_promoted, email_count_sent, email_count_received, created_at, updated_at) VALUES ('d1', 'acc1', 'Hidden', '[]', '[]', '[]', '[]', '[]', 'discovered', 0, 1, 0, 1000, 1000)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO contact_emails (id, contact_id, email, type, is_primary) VALUES ('de1', 'd1', 'hidden@test.com', 'other', 1)")
            .execute(&pool).await.unwrap();

        let list = get_contacts_inner(&pool, "acc1", None, None, 0, 50).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].contact.display_name, "Visible");
    }

    #[tokio::test]
    async fn test_autocomplete_includes_unpromoted_discovered() {
        let pool = test_pool().await;
        // Create a discovered unpromoted contact
        sqlx::query("INSERT INTO contacts (id, account_id, display_name, phones, addresses, social_profiles, urls, relations, source, is_promoted, email_count_sent, email_count_received, created_at, updated_at) VALUES ('d1', 'acc1', 'Discovered Person', '[]', '[]', '[]', '[]', '[]', 'discovered', 0, 1, 0, 1000, 1000)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO contact_emails (id, contact_id, email, type, is_primary) VALUES ('de1', 'd1', 'discovered@test.com', 'other', 1)")
            .execute(&pool).await.unwrap();

        let results = search_contacts_autocomplete(&pool, "acc1", "discovered").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].email, "discovered@test.com");
    }
}
