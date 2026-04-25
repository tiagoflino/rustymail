use super::connection::ImapSession;
use crate::provider::folder_mapping::{detect_special_use_from_name, imap_folder_to_label_id};
use crate::provider::types::{Folder, SpecialUse};
use futures::StreamExt;

pub async fn discover_folders(session: &mut ImapSession) -> Result<Vec<Folder>, String> {
    let list_stream = session
        .list(Some(""), Some("*"))
        .await
        .map_err(|e| format!("LIST failed: {}", e))?;

    let entries: Vec<_> = list_stream
        .filter_map(|r| async { r.ok() })
        .collect()
        .await;

    let mut folders = Vec::new();
    for entry in &entries {
        let name = entry.name().to_string();
        let delimiter = entry
            .delimiter()
            .map(|d| d.to_string())
            .unwrap_or_else(|| "/".to_string());

        let special_use = detect_special_use_from_attributes(entry.attributes())
            .or_else(|| detect_special_use_from_name(&name));

        folders.push(Folder {
            name,
            delimiter,
            special_use,
        });
    }

    Ok(folders)
}

fn detect_special_use_from_attributes(attrs: &[async_imap::types::NameAttribute<'_>]) -> Option<SpecialUse> {
    use async_imap::types::NameAttribute;
    for attr in attrs {
        match attr {
            NameAttribute::All => return Some(SpecialUse::All),
            NameAttribute::Archive => return Some(SpecialUse::Archive),
            NameAttribute::Drafts => return Some(SpecialUse::Drafts),
            NameAttribute::Flagged => return Some(SpecialUse::Flagged),
            NameAttribute::Junk => return Some(SpecialUse::Junk),
            NameAttribute::Sent => return Some(SpecialUse::Sent),
            NameAttribute::Trash => return Some(SpecialUse::Trash),
            NameAttribute::Extension(ext) => {
                let lower = ext.to_ascii_lowercase();
                if lower == "\\inbox" {
                    return Some(SpecialUse::Inbox);
                }
            }
            _ => {}
        }
    }
    None
}

pub async fn sync_folders_to_labels(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    folders: &[Folder],
) -> Result<(), String> {
    for folder in folders {
        let label_id = imap_folder_to_label_id(&folder.name, folder.special_use.as_ref());
        let label_type = if folder.special_use.is_some() {
            "system"
        } else {
            "user"
        };

        sqlx::query(
            "INSERT INTO labels (id, account_id, name, type, unread_count, threads_total, threads_unread)
             VALUES (?, ?, ?, ?, 0, 0, 0)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name",
        )
        .bind(&label_id)
        .bind(account_id)
        .bind(&folder.name)
        .bind(label_type)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to sync folder {}: {}", folder.name, e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[tokio::test]
    async fn test_sync_folders_to_labels() {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        crate::db::apply_schema(&pool).await.unwrap();

        let folders = vec![
            Folder {
                name: "INBOX".to_string(),
                delimiter: "/".to_string(),
                special_use: Some(SpecialUse::Inbox),
            },
            Folder {
                name: "Sent".to_string(),
                delimiter: "/".to_string(),
                special_use: Some(SpecialUse::Sent),
            },
            Folder {
                name: "Work/Projects".to_string(),
                delimiter: "/".to_string(),
                special_use: None,
            },
        ];

        sync_folders_to_labels(&pool, "acc1", &folders).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM labels WHERE account_id = 'acc1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 3);

        let inbox_label: (String,) = sqlx::query_as("SELECT id FROM labels WHERE name = 'INBOX' AND account_id = 'acc1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(inbox_label.0, "INBOX");

        let work_label: (String,) = sqlx::query_as("SELECT id FROM labels WHERE name = 'Work/Projects' AND account_id = 'acc1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(work_label.0, "imap:Work/Projects");
    }

    #[tokio::test]
    async fn test_sync_folders_idempotent() {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        crate::db::apply_schema(&pool).await.unwrap();

        let folders = vec![Folder {
            name: "INBOX".to_string(),
            delimiter: "/".to_string(),
            special_use: Some(SpecialUse::Inbox),
        }];

        sync_folders_to_labels(&pool, "acc1", &folders).await.unwrap();
        sync_folders_to_labels(&pool, "acc1", &folders).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM labels WHERE account_id = 'acc1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 1);
    }
}
