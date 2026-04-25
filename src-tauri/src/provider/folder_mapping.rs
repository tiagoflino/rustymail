use super::types::{Folder, SpecialUse};

pub fn imap_folder_to_label_id(folder_name: &str, special_use: Option<&SpecialUse>) -> String {
    match special_use {
        Some(SpecialUse::Inbox) => "INBOX".to_string(),
        Some(SpecialUse::Sent) => "SENT".to_string(),
        Some(SpecialUse::Drafts) => "DRAFT".to_string(),
        Some(SpecialUse::Trash) => "TRASH".to_string(),
        Some(SpecialUse::Junk) => "SPAM".to_string(),
        Some(SpecialUse::Flagged) => "STARRED".to_string(),
        Some(SpecialUse::Archive) => "imap:Archive".to_string(),
        Some(SpecialUse::All) => "imap:All Mail".to_string(),
        None => format!("imap:{}", folder_name),
    }
}

pub fn label_id_to_imap_folder<'a>(label_id: &str, folders: &'a [Folder]) -> Option<&'a str> {
    let target_special_use = match label_id {
        "INBOX" => Some(SpecialUse::Inbox),
        "SENT" => Some(SpecialUse::Sent),
        "DRAFT" => Some(SpecialUse::Drafts),
        "TRASH" => Some(SpecialUse::Trash),
        "SPAM" => Some(SpecialUse::Junk),
        "STARRED" => Some(SpecialUse::Flagged),
        _ => None,
    };

    if let Some(ref target) = target_special_use {
        for folder in folders {
            if folder.special_use.as_ref() == Some(target) {
                return Some(&folder.name);
            }
        }
    }

    if let Some(folder_name) = label_id.strip_prefix("imap:") {
        for folder in folders {
            if folder.name == folder_name {
                return Some(&folder.name);
            }
        }
    }

    None
}

pub fn detect_special_use_from_name(name: &str) -> Option<SpecialUse> {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "inbox" => Some(SpecialUse::Inbox),
        "sent" | "sent items" | "sent messages" => Some(SpecialUse::Sent),
        "drafts" | "draft" => Some(SpecialUse::Drafts),
        "trash" | "deleted items" | "deleted messages" | "bin" => Some(SpecialUse::Trash),
        "junk" | "spam" | "junk e-mail" | "bulk mail" => Some(SpecialUse::Junk),
        "archive" | "archives" => Some(SpecialUse::Archive),
        "flagged" | "starred" => Some(SpecialUse::Flagged),
        "all mail" | "all" => Some(SpecialUse::All),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_folders() -> Vec<Folder> {
        vec![
            Folder { name: "INBOX".to_string(), delimiter: "/".to_string(), special_use: Some(SpecialUse::Inbox) },
            Folder { name: "Sent".to_string(), delimiter: "/".to_string(), special_use: Some(SpecialUse::Sent) },
            Folder { name: "Drafts".to_string(), delimiter: "/".to_string(), special_use: Some(SpecialUse::Drafts) },
            Folder { name: "Trash".to_string(), delimiter: "/".to_string(), special_use: Some(SpecialUse::Trash) },
            Folder { name: "Junk".to_string(), delimiter: "/".to_string(), special_use: Some(SpecialUse::Junk) },
            Folder { name: "Archive".to_string(), delimiter: "/".to_string(), special_use: Some(SpecialUse::Archive) },
            Folder { name: "Work/Projects".to_string(), delimiter: "/".to_string(), special_use: None },
            Folder { name: "Personal".to_string(), delimiter: "/".to_string(), special_use: None },
        ]
    }

    #[test]
    fn test_inbox_maps_to_inbox_label() {
        assert_eq!(imap_folder_to_label_id("INBOX", Some(&SpecialUse::Inbox)), "INBOX");
    }

    #[test]
    fn test_sent_maps_to_sent_label() {
        assert_eq!(imap_folder_to_label_id("Sent", Some(&SpecialUse::Sent)), "SENT");
    }

    #[test]
    fn test_drafts_maps_to_draft_label() {
        assert_eq!(imap_folder_to_label_id("Drafts", Some(&SpecialUse::Drafts)), "DRAFT");
    }

    #[test]
    fn test_trash_maps_to_trash_label() {
        assert_eq!(imap_folder_to_label_id("Trash", Some(&SpecialUse::Trash)), "TRASH");
    }

    #[test]
    fn test_junk_maps_to_spam_label() {
        assert_eq!(imap_folder_to_label_id("Junk", Some(&SpecialUse::Junk)), "SPAM");
    }

    #[test]
    fn test_flagged_maps_to_starred_label() {
        assert_eq!(imap_folder_to_label_id("Flagged", Some(&SpecialUse::Flagged)), "STARRED");
    }

    #[test]
    fn test_archive_maps_to_imap_archive_label() {
        assert_eq!(imap_folder_to_label_id("Archive", Some(&SpecialUse::Archive)), "imap:Archive");
    }

    #[test]
    fn test_all_maps_to_imap_all_mail_label() {
        assert_eq!(imap_folder_to_label_id("All Mail", Some(&SpecialUse::All)), "imap:All Mail");
    }

    #[test]
    fn test_regular_folder_maps_to_prefixed_label() {
        assert_eq!(imap_folder_to_label_id("Work/Projects", None), "imap:Work/Projects");
        assert_eq!(imap_folder_to_label_id("Personal", None), "imap:Personal");
    }

    #[test]
    fn test_nested_folder_preserves_path() {
        assert_eq!(imap_folder_to_label_id("Work/Projects/2024", None), "imap:Work/Projects/2024");
    }

    #[test]
    fn test_reverse_mapping_system_folders() {
        let folders = test_folders();
        assert_eq!(label_id_to_imap_folder("INBOX", &folders), Some("INBOX"));
        assert_eq!(label_id_to_imap_folder("SENT", &folders), Some("Sent"));
        assert_eq!(label_id_to_imap_folder("DRAFT", &folders), Some("Drafts"));
        assert_eq!(label_id_to_imap_folder("TRASH", &folders), Some("Trash"));
        assert_eq!(label_id_to_imap_folder("SPAM", &folders), Some("Junk"));
    }

    #[test]
    fn test_reverse_mapping_user_folders() {
        let folders = test_folders();
        assert_eq!(label_id_to_imap_folder("imap:Work/Projects", &folders), Some("Work/Projects"));
        assert_eq!(label_id_to_imap_folder("imap:Personal", &folders), Some("Personal"));
    }

    #[test]
    fn test_reverse_mapping_nonexistent_returns_none() {
        let folders = test_folders();
        assert_eq!(label_id_to_imap_folder("IMPORTANT", &folders), None);
        assert_eq!(label_id_to_imap_folder("imap:DoesNotExist", &folders), None);
        assert_eq!(label_id_to_imap_folder("random_string", &folders), None);
    }

    #[test]
    fn test_reverse_mapping_archive() {
        let folders = test_folders();
        assert_eq!(label_id_to_imap_folder("imap:Archive", &folders), Some("Archive"));
    }

    #[test]
    fn test_detect_special_use_common_names() {
        assert_eq!(detect_special_use_from_name("INBOX"), Some(SpecialUse::Inbox));
        assert_eq!(detect_special_use_from_name("Sent"), Some(SpecialUse::Sent));
        assert_eq!(detect_special_use_from_name("Sent Items"), Some(SpecialUse::Sent));
        assert_eq!(detect_special_use_from_name("Sent Messages"), Some(SpecialUse::Sent));
        assert_eq!(detect_special_use_from_name("Drafts"), Some(SpecialUse::Drafts));
        assert_eq!(detect_special_use_from_name("Draft"), Some(SpecialUse::Drafts));
        assert_eq!(detect_special_use_from_name("Trash"), Some(SpecialUse::Trash));
        assert_eq!(detect_special_use_from_name("Deleted Items"), Some(SpecialUse::Trash));
        assert_eq!(detect_special_use_from_name("Bin"), Some(SpecialUse::Trash));
        assert_eq!(detect_special_use_from_name("Junk"), Some(SpecialUse::Junk));
        assert_eq!(detect_special_use_from_name("Spam"), Some(SpecialUse::Junk));
        assert_eq!(detect_special_use_from_name("Junk E-mail"), Some(SpecialUse::Junk));
        assert_eq!(detect_special_use_from_name("Bulk Mail"), Some(SpecialUse::Junk));
        assert_eq!(detect_special_use_from_name("Archive"), Some(SpecialUse::Archive));
        assert_eq!(detect_special_use_from_name("Archives"), Some(SpecialUse::Archive));
        assert_eq!(detect_special_use_from_name("All Mail"), Some(SpecialUse::All));
    }

    #[test]
    fn test_detect_special_use_case_insensitive() {
        assert_eq!(detect_special_use_from_name("SENT"), Some(SpecialUse::Sent));
        assert_eq!(detect_special_use_from_name("trash"), Some(SpecialUse::Trash));
        assert_eq!(detect_special_use_from_name("JUNK"), Some(SpecialUse::Junk));
    }

    #[test]
    fn test_detect_special_use_unknown_returns_none() {
        assert_eq!(detect_special_use_from_name("Work"), None);
        assert_eq!(detect_special_use_from_name("Personal"), None);
        assert_eq!(detect_special_use_from_name("Projects/2024"), None);
    }

    #[test]
    fn test_roundtrip_system_folders() {
        let folders = test_folders();
        for folder in &folders {
            if folder.special_use.is_some() {
                let label = imap_folder_to_label_id(&folder.name, folder.special_use.as_ref());
                let back = label_id_to_imap_folder(&label, &folders);
                assert_eq!(back, Some(folder.name.as_str()), "Roundtrip failed for {}", folder.name);
            }
        }
    }

    #[test]
    fn test_roundtrip_user_folders() {
        let folders = test_folders();
        for folder in &folders {
            if folder.special_use.is_none() {
                let label = imap_folder_to_label_id(&folder.name, None);
                let back = label_id_to_imap_folder(&label, &folders);
                assert_eq!(back, Some(folder.name.as_str()), "Roundtrip failed for {}", folder.name);
            }
        }
    }
}
