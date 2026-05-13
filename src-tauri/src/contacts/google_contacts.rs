use crate::commands::contacts::{CreateContactInput, EmailInput, UpdateContactInput};
use serde_json::Value;
use sqlx::SqlitePool;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Extract the remote resource name from a Google People API person object.
pub fn get_remote_id(person: &Value) -> Option<String> {
    person.get("resourceName")?.as_str().map(|s| s.to_string())
}

/// Extract the etag from a Google People API person object.
pub fn get_etag(person: &Value) -> Option<String> {
    person.get("etag")?.as_str().map(|s| s.to_string())
}

/// Parse a Google People API person JSON object into a CreateContactInput.
///
/// Extracts names, emails, phone numbers, organization info, and photo URL.
/// Falls back to using the first email address as the display name if no name is present.
/// Returns Err if neither a name nor any email address is found.
pub fn parse_google_person(person: &Value) -> Result<CreateContactInput, String> {
    // Names
    let names = person.get("names").and_then(|v| v.as_array());
    let display_name = names
        .and_then(|arr| arr.first())
        .and_then(|n| n.get("displayName"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let given_name = names
        .and_then(|arr| arr.first())
        .and_then(|n| n.get("givenName"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let family_name = names
        .and_then(|arr| arr.first())
        .and_then(|n| n.get("familyName"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Emails
    let email_addresses = person.get("emailAddresses").and_then(|v| v.as_array());
    let emails: Vec<EmailInput> = email_addresses
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let email = e.get("value")?.as_str()?.to_string();
                    let email_type = e
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("other")
                        .to_string();
                    let is_primary = e
                        .get("metadata")
                        .and_then(|m| m.get("primary"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    Some(EmailInput {
                        email,
                        r#type: email_type,
                        is_primary,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Use first email as display name fallback
    let display_name = match display_name {
        Some(name) if !name.is_empty() => name,
        _ => {
            if let Some(first_email) = emails.first() {
                first_email.email.clone()
            } else {
                return Err("No name and no emails found in person object".to_string());
            }
        }
    };

    // Phone numbers
    let phones: Vec<serde_json::Value> = person
        .get("phoneNumbers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let number = p.get("value")?.as_str()?.to_string();
                    let phone_type = p
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("other")
                        .to_string();
                    Some(serde_json::json!({
                        "number": number,
                        "type": phone_type,
                    }))
                })
                .collect()
        })
        .unwrap_or_default();

    // Organization
    let org = person
        .get("organizations")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first());
    let company = org
        .and_then(|o| o.get("name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let job_title = org
        .and_then(|o| o.get("title"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let department = org
        .and_then(|o| o.get("department"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Photo
    let photo_uri = person
        .get("photos")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|p| p.get("url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(CreateContactInput {
        display_name,
        given_name,
        surname: family_name,
        company,
        job_title,
        department,
        photo_uri,
        phones,
        emails,
        ..Default::default()
    })
}

/// Fetch contacts from Google People API with pagination and optional sync token.
///
/// Returns a tuple of (all person objects, next sync token).
/// If sync token is expired (HTTP 410), returns Err("SYNC_TOKEN_EXPIRED").
pub async fn fetch_google_contacts(
    access_token: &str,
    sync_token: Option<&str>,
) -> Result<(Vec<Value>, Option<String>), String> {
    let client = reqwest::Client::new();
    let base_url = "https://people.googleapis.com/v1/people/me/connections";
    let person_fields =
        "names,emailAddresses,phoneNumbers,organizations,photos,birthdays,addresses,urls,biographies";

    let mut all_persons: Vec<Value> = Vec::new();
    let mut page_token: Option<String> = None;
    #[allow(unused_assignments)]
    let mut next_sync_token: Option<String> = None;

    loop {
        let mut url = format!(
            "{}?personFields={}&pageSize=100",
            base_url, person_fields
        );

        if let Some(token) = sync_token {
            url.push_str(&format!("&syncToken={}", token));
        }

        if let Some(ref pt) = page_token {
            url.push_str(&format!("&pageToken={}", pt));
        }

        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| format!("Failed to fetch contacts: {}", e))?;

        let status = response.status();
        if status.as_u16() == 410 {
            return Err("SYNC_TOKEN_EXPIRED".to_string());
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!(
                "Google People API error ({}): {}",
                status.as_u16(),
                body
            ));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if let Some(connections) = body.get("connections").and_then(|v| v.as_array()) {
            all_persons.extend(connections.iter().cloned());
        }

        next_sync_token = body
            .get("nextSyncToken")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        page_token = body
            .get("nextPageToken")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if page_token.is_none() {
            break;
        }

        // Small delay between pagination requests to avoid rate limits
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    Ok((all_persons, next_sync_token))
}

/// Sync contacts from Google People API into the local database.
///
/// Uses incremental sync via sync tokens when available.
/// Handles deduplication by checking provider links and email matches.
/// Returns the number of contacts synced.
pub async fn sync_google_contacts(
    pool: &SqlitePool,
    account_id: &str,
    access_token: &str,
) -> Result<usize, String> {
    // Get existing sync token
    let existing_token: Option<String> = sqlx::query_scalar(
        "SELECT sync_token FROM contacts_sync_state WHERE account_id = ? AND provider = 'google'",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .flatten();

    // Fetch contacts (retry without token if expired)
    let (persons, new_sync_token) =
        match fetch_google_contacts(access_token, existing_token.as_deref()).await {
            Ok(result) => result,
            Err(e) if e == "SYNC_TOKEN_EXPIRED" => {
                // Clear expired token and do full sync
                sqlx::query(
                    "DELETE FROM contacts_sync_state WHERE account_id = ? AND provider = 'google'",
                )
                .bind(account_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;

                fetch_google_contacts(access_token, None).await?
            }
            Err(e) => return Err(e),
        };

    let mut synced_count: usize = 0;

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

        // Skip contacts with no emails
        if input.emails.is_empty() {
            continue;
        }

        // Check if we already have a provider link for this remote_id
        let existing_link: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT contact_id, etag FROM contact_provider_links WHERE account_id = ? AND provider = 'google' AND remote_id = ?",
        )
        .bind(account_id)
        .bind(&remote_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

        if let Some((contact_id, existing_etag)) = existing_link {
            // Already linked — check if update needed
            if existing_etag.as_deref() == etag.as_deref() {
                continue; // No changes
            }

            // Update the existing contact
            let update_input = UpdateContactInput {
                display_name: Some(input.display_name),
                given_name: input.given_name,
                surname: input.surname,
                company: input.company,
                job_title: input.job_title,
                department: input.department,
                photo_uri: input.photo_uri,
                phones: Some(input.phones),
                emails: Some(input.emails),
                ..Default::default()
            };

            if crate::commands::contacts::update_contact_inner(pool, &contact_id, update_input)
                .await
                .is_ok()
            {
                // Update etag on the link
                sqlx::query(
                    "UPDATE contact_provider_links SET etag = ?, last_synced_at = ? WHERE account_id = ? AND provider = 'google' AND remote_id = ?",
                )
                .bind(&etag)
                .bind(now_epoch())
                .bind(account_id)
                .bind(&remote_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;

                synced_count += 1;
            }
        } else {
            // No existing link — check for email dedup
            let mut found_contact_id: Option<String> = None;
            for email_input in &input.emails {
                let existing: Option<(String,)> = sqlx::query_as(
                    "SELECT ce.contact_id FROM contact_emails ce JOIN contacts c ON c.id = ce.contact_id WHERE c.account_id = ? AND LOWER(ce.email) = LOWER(?)",
                )
                .bind(account_id)
                .bind(&email_input.email)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;

                if let Some((cid,)) = existing {
                    found_contact_id = Some(cid);
                    break;
                }
            }

            let contact_id = if let Some(cid) = found_contact_id {
                // Existing contact found by email — just link it
                cid
            } else {
                // Create new contact
                let created =
                    crate::commands::contacts::create_contact_inner(pool, account_id, input).await;
                match created {
                    Ok(c) => c.contact.id,
                    Err(_) => continue,
                }
            };

            // Create provider link
            let link_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT OR IGNORE INTO contact_provider_links (id, contact_id, account_id, provider, remote_id, etag, last_synced_at) VALUES (?, ?, ?, 'google', ?, ?, ?)",
            )
            .bind(&link_id)
            .bind(&contact_id)
            .bind(account_id)
            .bind(&remote_id)
            .bind(&etag)
            .bind(now_epoch())
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

            synced_count += 1;
        }
    }

    // Store new sync token
    if let Some(token) = new_sync_token {
        sqlx::query(
            "INSERT INTO contacts_sync_state (account_id, provider, sync_token, last_full_sync) VALUES (?, 'google', ?, ?) ON CONFLICT(account_id, provider) DO UPDATE SET sync_token = excluded.sync_token, last_full_sync = excluded.last_full_sync",
        )
        .bind(account_id)
        .bind(&token)
        .bind(now_epoch())
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(synced_count)
}

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
        assert_eq!(input.department, Some("Eng".to_string()));
        assert_eq!(input.photo_uri, Some("https://photo.url/pic.jpg".to_string()));
        assert_eq!(input.emails.len(), 2);
        assert_eq!(input.emails[0].email, "john@work.com");
        assert!(input.emails[0].is_primary);
        assert_eq!(input.emails[1].email, "john@home.com");
        assert!(!input.emails[1].is_primary);
        assert_eq!(input.phones.len(), 1);
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
        assert_eq!(input.emails[0].email, "minimal@test.com");
        assert!(!input.emails[0].is_primary);
        assert_eq!(input.emails[0].r#type, "other");
    }

    #[test]
    fn test_parse_google_person_no_name_no_email_fails() {
        let person_json = serde_json::json!({
            "resourceName": "people/c000",
            "etag": "empty",
        });

        let result = parse_google_person(&person_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_remote_id_and_etag() {
        let person = serde_json::json!({
            "resourceName": "people/c123",
            "etag": "etag456"
        });
        assert_eq!(get_remote_id(&person), Some("people/c123".to_string()));
        assert_eq!(get_etag(&person), Some("etag456".to_string()));
    }

    #[test]
    fn test_get_remote_id_missing() {
        let person = serde_json::json!({});
        assert_eq!(get_remote_id(&person), None);
        assert_eq!(get_etag(&person), None);
    }
}
