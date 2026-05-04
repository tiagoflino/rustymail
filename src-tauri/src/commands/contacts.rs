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
}
