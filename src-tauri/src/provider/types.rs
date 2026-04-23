#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderCapabilities {
    pub has_labels: bool,
    pub has_categories: bool,
    pub has_superstars: bool,
    pub has_important: bool,
    pub has_server_threading: bool,
    pub has_drive_upload: bool,
    pub has_calendar: bool,
}

impl ProviderCapabilities {
    pub fn gmail() -> Self {
        Self {
            has_labels: true,
            has_categories: true,
            has_superstars: true,
            has_important: true,
            has_server_threading: true,
            has_drive_upload: true,
            has_calendar: true,
        }
    }

    pub fn imap() -> Self {
        Self {
            has_labels: false,
            has_categories: false,
            has_superstars: false,
            has_important: false,
            has_server_threading: false,
            has_drive_upload: false,
            has_calendar: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SpecialUse {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Junk,
    Archive,
    Flagged,
    All,
}

#[derive(Debug, Clone)]
pub struct Folder {
    pub name: String,
    pub delimiter: String,
    pub special_use: Option<SpecialUse>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderType {
    Gmail,
    Imap,
}

impl ProviderType {
    pub fn as_str(&self) -> &str {
        match self {
            ProviderType::Gmail => "gmail",
            ProviderType::Imap => "imap",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "imap" => ProviderType::Imap,
            _ => ProviderType::Gmail,
        }
    }

    pub fn capabilities(&self) -> ProviderCapabilities {
        match self {
            ProviderType::Gmail => ProviderCapabilities::gmail(),
            ProviderType::Imap => ProviderCapabilities::imap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gmail_capabilities() {
        let caps = ProviderCapabilities::gmail();
        assert!(caps.has_labels);
        assert!(caps.has_categories);
        assert!(caps.has_superstars);
        assert!(caps.has_important);
        assert!(caps.has_server_threading);
        assert!(caps.has_drive_upload);
        assert!(caps.has_calendar);
    }

    #[test]
    fn test_imap_capabilities() {
        let caps = ProviderCapabilities::imap();
        assert!(!caps.has_labels);
        assert!(!caps.has_categories);
        assert!(!caps.has_superstars);
        assert!(!caps.has_important);
        assert!(!caps.has_server_threading);
        assert!(!caps.has_drive_upload);
        assert!(!caps.has_calendar);
    }

    #[test]
    fn test_provider_type_roundtrip() {
        assert_eq!(ProviderType::parse("gmail"), ProviderType::Gmail);
        assert_eq!(ProviderType::parse("imap"), ProviderType::Imap);
        assert_eq!(ProviderType::parse("unknown"), ProviderType::Gmail);
        assert_eq!(ProviderType::Gmail.as_str(), "gmail");
        assert_eq!(ProviderType::Imap.as_str(), "imap");
    }

    #[test]
    fn test_provider_type_capabilities_dispatch() {
        let gmail_caps = ProviderType::Gmail.capabilities();
        assert!(gmail_caps.has_categories);

        let imap_caps = ProviderType::Imap.capabilities();
        assert!(!imap_caps.has_categories);
    }

    #[test]
    fn test_capabilities_serialization() {
        let caps = ProviderCapabilities::gmail();
        let json = serde_json::to_string(&caps).unwrap();
        assert!(json.contains("has_labels"));
        let deserialized: ProviderCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.has_labels, caps.has_labels);
        assert_eq!(deserialized.has_categories, caps.has_categories);
    }
}
