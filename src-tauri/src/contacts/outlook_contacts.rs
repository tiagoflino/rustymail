use crate::commands::contacts::{CreateContactInput, EmailInput};
use reqwest::Client;
use serde_json::Value;
use sqlx::SqlitePool;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

const CONTACTS_DELTA_SELECT: &str = "displayName,givenName,surname,emailAddresses,mobilePhone,businessPhones,companyName,jobTitle,department";

fn contacts_delta_url() -> String {
    format!(
        "https://graph.microsoft.com/v1.0/me/contacts/delta?$select={}",
        CONTACTS_DELTA_SELECT
    )
}

/// Extract the remote ID from an Outlook contact JSON object.
pub fn get_remote_id(contact: &Value) -> Option<String> {
    contact.get("id").and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// Extract the ETag from an Outlook contact JSON object.
pub fn get_etag(contact: &Value) -> Option<String> {
    contact
        .get("@odata.etag")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Parse a Microsoft Graph contact JSON object into a CreateContactInput.
pub fn parse_outlook_contact(contact: &Value) -> Result<CreateContactInput, String> {
    let display_name = contact
        .get("displayName")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let given_name = contact
        .get("givenName")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let surname = contact
        .get("surname")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let company = contact
        .get("companyName")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let job_title = contact
        .get("jobTitle")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let department = contact
        .get("department")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // Parse email addresses — Outlook uses "address" field
    let mut emails: Vec<EmailInput> = Vec::new();
    if let Some(email_arr) = contact.get("emailAddresses").and_then(|v| v.as_array()) {
        for (i, entry) in email_arr.iter().enumerate() {
            if let Some(address) = entry.get("address").and_then(|v| v.as_str()) {
                if !address.is_empty() {
                    emails.push(EmailInput {
                        email: address.to_string(),
                        r#type: "other".to_string(),
                        is_primary: i == 0,
                    });
                }
            }
        }
    }

    // Parse phones — mobilePhone is a single string, businessPhones is an array
    let mut phones: Vec<serde_json::Value> = Vec::new();
    if let Some(mobile) = contact.get("mobilePhone").and_then(|v| v.as_str()) {
        if !mobile.is_empty() {
            phones.push(serde_json::json!({
                "number": mobile,
                "type": "mobile",
            }));
        }
    }
    if let Some(biz_phones) = contact.get("businessPhones").and_then(|v| v.as_array()) {
        for phone_val in biz_phones {
            if let Some(number) = phone_val.as_str() {
                if !number.is_empty() {
                    phones.push(serde_json::json!({
                        "number": number,
                        "type": "work",
                    }));
                }
            }
        }
    }

    // Fallback display name to first email if empty
    let final_display_name = if display_name.is_empty() {
        emails
            .first()
            .map(|e| e.email.clone())
            .ok_or_else(|| "Contact has no display name or email".to_string())?
    } else {
        display_name
    };

    Ok(CreateContactInput {
        display_name: final_display_name,
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

/// Fetch contacts from Microsoft Graph using delta queries for incremental sync.
/// Returns (contacts, delta_link) where delta_link can be stored for next sync.
pub async fn fetch_outlook_contacts(
    access_token: &str,
    delta_link: Option<&str>,
) -> Result<(Vec<Value>, Option<String>), String> {
    let client = Client::new();
    let mut all_contacts: Vec<Value> = Vec::new();

    let initial_url = match delta_link {
        Some(link) => link.to_string(),
        None => contacts_delta_url(),
    };

    let mut next_url = Some(initial_url);

    while let Some(url) = next_url.take() {
        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Prefer", "IdType=\"ImmutableId\"")
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let status = response.status();
        if status == reqwest::StatusCode::GONE {
            return Err("DELTA_TOKEN_EXPIRED".to_string());
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Graph API error {}: {}", status.as_u16(), body));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if let Some(values) = body.get("value").and_then(|v| v.as_array()) {
            all_contacts.extend(values.iter().cloned());
        }

        // Check for next page
        next_url = body
            .get("@odata.nextLink")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // If no nextLink, check for deltaLink — otherwise add a small delay before next page
        if next_url.is_some() {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        if next_url.is_none() {
            let new_delta = body
                .get("@odata.deltaLink")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            return Ok((all_contacts, new_delta));
        }
    }

    Ok((all_contacts, None))
}

/// Sync contacts from Outlook for a given account.
/// Fetches contacts via delta query, deduplicates, and creates/updates local contacts.
/// Returns the number of contacts synced.
pub async fn sync_outlook_contacts(
    pool: &SqlitePool,
    account_id: &str,
    access_token: &str,
) -> Result<usize, String> {
    // Get stored delta link from contacts_sync_state
    let delta_link: Option<String> = sqlx::query_scalar(
        "SELECT sync_token FROM contacts_sync_state WHERE account_id = ? AND provider = 'outlook'",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .flatten();

    // Fetch contacts from Graph API
    let (contacts, new_delta_link) =
        match fetch_outlook_contacts(access_token, delta_link.as_deref()).await {
            Ok(result) => result,
            Err(e) if e == "DELTA_TOKEN_EXPIRED" => {
                // Clear stored delta link and retry full sync
                sqlx::query(
                    "DELETE FROM contacts_sync_state WHERE account_id = ? AND provider = 'outlook'",
                )
                .bind(account_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;

                fetch_outlook_contacts(access_token, None).await?
            }
            Err(e) => return Err(e),
        };

    let mut synced_count = 0;

    for contact_json in &contacts {
        let remote_id = match get_remote_id(contact_json) {
            Some(id) => id,
            None => continue,
        };

        let etag = get_etag(contact_json);

        // Check if contact was deleted (delta sync returns @removed annotation)
        if contact_json.get("@removed").is_some() {
            let existing_contact_id: Option<String> = sqlx::query_scalar(
                "SELECT contact_id FROM contact_provider_links WHERE remote_id = ? AND account_id = ? AND provider = 'outlook'",
            )
            .bind(&remote_id)
            .bind(account_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;

            if let Some(contact_id) = existing_contact_id {
                crate::commands::contacts::delete_contact_inner(pool, &contact_id)
                    .await
                    .ok();
            }
            continue;
        }

        let input = match parse_outlook_contact(contact_json) {
            Ok(i) => i,
            Err(_) => continue,
        };

        // Check if we already have this contact linked
        let existing: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT contact_id, etag FROM contact_provider_links WHERE remote_id = ? AND account_id = ? AND provider = 'outlook'",
        )
        .bind(&remote_id)
        .bind(account_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

        match existing {
            Some((contact_id, stored_etag)) => {
                // Update if etag changed
                if etag.as_deref() != stored_etag.as_deref() {
                    let update_input = crate::commands::contacts::UpdateContactInput {
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
                    crate::commands::contacts::update_contact_inner(pool, &contact_id, update_input)
                        .await
                        .ok();

                    // Update etag in provider link
                    sqlx::query(
                        "UPDATE contact_provider_links SET etag = ? WHERE remote_id = ? AND account_id = ? AND provider = 'outlook'",
                    )
                    .bind(&etag)
                    .bind(&remote_id)
                    .bind(account_id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;

                    synced_count += 1;
                }
            }
            None => {
                // Check for email match (dedup) scoped to same account
                let mut matched_contact_id: Option<String> = None;
                for e in &input.emails {
                    let existing_email: Option<(String,)> = sqlx::query_as(
                        "SELECT ce.contact_id FROM contact_emails ce JOIN contacts c ON c.id = ce.contact_id WHERE ce.email = ? COLLATE NOCASE AND c.account_id = ?"
                    ).bind(&e.email).bind(account_id).fetch_optional(pool).await.unwrap_or(None);
                    if let Some((cid,)) = existing_email {
                        matched_contact_id = Some(cid);
                        break;
                    }
                }

                let contact_id = if let Some(cid) = matched_contact_id {
                    cid
                } else {
                    match crate::commands::contacts::create_contact_inner(pool, account_id, input).await {
                        Ok(created) => created.contact.id,
                        Err(_) => continue,
                    }
                };

                // Create provider link
                let link_id = uuid::Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO contact_provider_links (id, contact_id, account_id, provider, remote_id, etag) VALUES (?, ?, ?, 'outlook', ?, ?)",
                )
                .bind(&link_id)
                .bind(&contact_id)
                .bind(account_id)
                .bind(&remote_id)
                .bind(&etag)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;

                synced_count += 1;
            }
        }
    }

    // Store new delta link in contacts_sync_state
    if let Some(new_link) = new_delta_link {
        let now = now_epoch();
        sqlx::query(
            "INSERT OR REPLACE INTO contacts_sync_state (account_id, provider, sync_token, last_full_sync) VALUES (?, 'outlook', ?, ?)",
        )
        .bind(account_id)
        .bind(&new_link)
        .bind(now)
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
        assert_eq!(input.surname, Some("Doe".to_string()));
        assert_eq!(input.company, Some("OutlookCo".to_string()));
        assert_eq!(input.job_title, Some("PM".to_string()));
        assert_eq!(input.department, Some("Product".to_string()));
        assert_eq!(input.emails.len(), 2);
        assert_eq!(input.emails[0].email, "jane@outlook.com");
        assert!(input.emails[0].is_primary);
        assert_eq!(input.emails[1].email, "jane@work.com");
        assert!(!input.emails[1].is_primary);
        assert_eq!(input.phones.len(), 2);
    }

    #[test]
    fn test_parse_outlook_contact_minimal() {
        let contact_json = serde_json::json!({
            "id": "AAMk789",
            "displayName": "Simple User",
            "emailAddresses": [{"address": "simple@test.com"}],
        });

        let input = parse_outlook_contact(&contact_json).unwrap();
        assert_eq!(input.display_name, "Simple User");
        assert_eq!(input.emails.len(), 1);
        assert_eq!(input.emails[0].email, "simple@test.com");
        assert!(input.emails[0].is_primary);
        assert!(input.given_name.is_none());
        assert!(input.surname.is_none());
        assert!(input.company.is_none());
    }

    #[test]
    fn test_parse_outlook_contact_no_display_name_uses_email() {
        let contact_json = serde_json::json!({
            "id": "AAMk000",
            "emailAddresses": [{"address": "fallback@test.com"}],
        });

        let input = parse_outlook_contact(&contact_json).unwrap();
        assert_eq!(input.display_name, "fallback@test.com");
    }

    #[test]
    fn test_parse_outlook_contact_no_name_no_email_fails() {
        let contact_json = serde_json::json!({
            "id": "AAMk111",
        });

        let result = parse_outlook_contact(&contact_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_outlook_remote_id_and_etag() {
        let contact = serde_json::json!({
            "id": "AAMkAD123",
            "@odata.etag": "W/\"abc\""
        });
        assert_eq!(get_remote_id(&contact), Some("AAMkAD123".to_string()));
        assert_eq!(get_etag(&contact), Some("W/\"abc\"".to_string()));
    }

    #[test]
    fn test_get_remote_id_missing() {
        let contact = serde_json::json!({});
        assert_eq!(get_remote_id(&contact), None);
    }

    #[test]
    fn test_get_etag_missing() {
        let contact = serde_json::json!({"id": "x"});
        assert_eq!(get_etag(&contact), None);
    }
}
