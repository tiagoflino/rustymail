use sha2::{Sha256, Digest};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ParsedMessageHeaders {
    pub uid: u32,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    pub subject: String,
    pub sender: String,
    pub recipients: String,
    pub date: i64,
}

#[derive(Debug, Clone)]
pub struct ThreadGroup {
    pub thread_id: String,
    pub message_uids: Vec<u32>,
    pub subject: String,
    pub latest_date: i64,
}

pub fn generate_thread_id(account_id: &str, root_message_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(root_message_id.as_bytes());
    let hash = hex::encode(hasher.finalize());
    format!("imap:{}:{}", account_id, &hash[..16])
}

pub fn group_into_threads(account_id: &str, messages: &[ParsedMessageHeaders]) -> Vec<ThreadGroup> {
    if messages.is_empty() {
        return vec![];
    }

    let mut msg_id_to_idx: HashMap<String, usize> = HashMap::new();
    for (i, msg) in messages.iter().enumerate() {
        if let Some(ref mid) = msg.message_id {
            let normalized = normalize_message_id(mid);
            msg_id_to_idx.insert(normalized, i);
        }
    }

    let mut parent_of: Vec<Option<usize>> = vec![None; messages.len()];

    for (i, msg) in messages.iter().enumerate() {
        if let Some(ref irt) = msg.in_reply_to {
            let normalized = normalize_message_id(irt);
            if let Some(&parent_idx) = msg_id_to_idx.get(&normalized) {
                if parent_idx != i {
                    parent_of[i] = Some(parent_idx);
                    continue;
                }
            }
        }

        if let Some(ref refs) = msg.references {
            let ref_ids: Vec<&str> = refs.split_whitespace().collect();
            for ref_id in ref_ids.iter().rev() {
                let normalized = normalize_message_id(ref_id);
                if let Some(&parent_idx) = msg_id_to_idx.get(&normalized) {
                    if parent_idx != i {
                        parent_of[i] = Some(parent_idx);
                        break;
                    }
                }
            }
        }
    }

    fn find_root(parent_of: &[Option<usize>], mut idx: usize) -> usize {
        let mut visited = std::collections::HashSet::new();
        while let Some(parent) = parent_of[idx] {
            if !visited.insert(idx) {
                break;
            }
            idx = parent;
        }
        idx
    }

    let mut root_to_members: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..messages.len() {
        let root = find_root(&parent_of, i);
        root_to_members.entry(root).or_default().push(i);
    }

    let mut groups: Vec<ThreadGroup> = Vec::new();
    for (root_idx, member_indices) in root_to_members {
        let root_msg = &messages[root_idx];
        let fallback_id = format!("uid-{}", root_msg.uid);
        let root_message_id = root_msg
            .message_id
            .as_deref()
            .unwrap_or(&fallback_id);

        let thread_id = generate_thread_id(account_id, root_message_id);

        let mut uids: Vec<u32> = member_indices.iter().map(|&i| messages[i].uid).collect();
        uids.sort();

        let latest_date = member_indices
            .iter()
            .map(|&i| messages[i].date)
            .max()
            .unwrap_or(0);

        let subject = root_msg.subject.clone();

        groups.push(ThreadGroup {
            thread_id,
            message_uids: uids,
            subject,
            latest_date,
        });
    }

    groups.sort_by(|a, b| b.latest_date.cmp(&a.latest_date));
    groups
}

fn normalize_message_id(raw: &str) -> String {
    raw.trim().trim_matches('<').trim_matches('>').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(uid: u32, mid: &str, irt: Option<&str>, refs: Option<&str>, subject: &str, date: i64) -> ParsedMessageHeaders {
        ParsedMessageHeaders {
            uid,
            message_id: if mid.is_empty() { None } else { Some(mid.to_string()) },
            in_reply_to: irt.map(|s| s.to_string()),
            references: refs.map(|s| s.to_string()),
            subject: subject.to_string(),
            sender: "sender@test.com".to_string(),
            recipients: "recipient@test.com".to_string(),
            date,
        }
    }

    #[test]
    fn test_generate_thread_id_format() {
        let id = generate_thread_id("user@test.com", "<abc123@mail.com>");
        assert!(id.starts_with("imap:user@test.com:"));
        assert_eq!(id.len(), "imap:user@test.com:".len() + 16);
    }

    #[test]
    fn test_generate_thread_id_deterministic() {
        let id1 = generate_thread_id("acc1", "<msg@test.com>");
        let id2 = generate_thread_id("acc1", "<msg@test.com>");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_generate_thread_id_different_accounts() {
        let id1 = generate_thread_id("acc1", "<msg@test.com>");
        let id2 = generate_thread_id("acc2", "<msg@test.com>");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_generate_thread_id_no_collision_with_gmail() {
        let id = generate_thread_id("user@test.com", "<abc@mail.com>");
        assert!(id.starts_with("imap:"));
    }

    #[test]
    fn test_empty_messages() {
        let groups = group_into_threads("acc1", &[]);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_standalone_messages() {
        let messages = vec![
            msg(1, "<msg1@test.com>", None, None, "Subject 1", 1000),
            msg(2, "<msg2@test.com>", None, None, "Subject 2", 2000),
            msg(3, "<msg3@test.com>", None, None, "Subject 3", 3000),
        ];

        let groups = group_into_threads("acc1", &messages);
        assert_eq!(groups.len(), 3);
    }

    #[test]
    fn test_linear_reply_chain() {
        let messages = vec![
            msg(1, "<msg1@test.com>", None, None, "Hello", 1000),
            msg(2, "<msg2@test.com>", Some("<msg1@test.com>"), Some("<msg1@test.com>"), "Re: Hello", 2000),
            msg(3, "<msg3@test.com>", Some("<msg2@test.com>"), Some("<msg1@test.com> <msg2@test.com>"), "Re: Re: Hello", 3000),
        ];

        let groups = group_into_threads("acc1", &messages);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].message_uids, vec![1, 2, 3]);
        assert_eq!(groups[0].latest_date, 3000);
        assert_eq!(groups[0].subject, "Hello");
    }

    #[test]
    fn test_forking_conversation() {
        let messages = vec![
            msg(1, "<msg1@test.com>", None, None, "Original", 1000),
            msg(2, "<msg2@test.com>", Some("<msg1@test.com>"), Some("<msg1@test.com>"), "Re: Original (fork A)", 2000),
            msg(3, "<msg3@test.com>", Some("<msg1@test.com>"), Some("<msg1@test.com>"), "Re: Original (fork B)", 3000),
        ];

        let groups = group_into_threads("acc1", &messages);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].message_uids.len(), 3);
    }

    #[test]
    fn test_missing_parent_falls_back_to_references() {
        let messages = vec![
            msg(1, "<msg1@test.com>", None, None, "Root", 1000),
            msg(3, "<msg3@test.com>", Some("<msg2@missing.com>"), Some("<msg1@test.com> <msg2@missing.com>"), "Re: Root", 3000),
        ];

        let groups = group_into_threads("acc1", &messages);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].message_uids, vec![1, 3]);
    }

    #[test]
    fn test_no_message_id_standalone() {
        let messages = vec![
            msg(1, "", None, None, "No Message-ID", 1000),
            msg(2, "<msg2@test.com>", None, None, "Has ID", 2000),
        ];

        let groups = group_into_threads("acc1", &messages);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_out_of_order_arrival() {
        let messages = vec![
            msg(3, "<msg3@test.com>", Some("<msg2@test.com>"), Some("<msg1@test.com> <msg2@test.com>"), "Re: Re: Topic", 3000),
            msg(1, "<msg1@test.com>", None, None, "Topic", 1000),
            msg(2, "<msg2@test.com>", Some("<msg1@test.com>"), Some("<msg1@test.com>"), "Re: Topic", 2000),
        ];

        let groups = group_into_threads("acc1", &messages);
        assert_eq!(groups.len(), 1);
        let mut uids = groups[0].message_uids.clone();
        uids.sort();
        assert_eq!(uids, vec![1, 2, 3]);
    }

    #[test]
    fn test_mixed_threaded_and_standalone() {
        let messages = vec![
            msg(1, "<msg1@test.com>", None, None, "Thread A", 1000),
            msg(2, "<msg2@test.com>", Some("<msg1@test.com>"), None, "Re: Thread A", 2000),
            msg(3, "<msg3@test.com>", None, None, "Standalone", 3000),
            msg(4, "<msg4@test.com>", None, None, "Thread B", 500),
            msg(5, "<msg5@test.com>", Some("<msg4@test.com>"), None, "Re: Thread B", 4000),
        ];

        let groups = group_into_threads("acc1", &messages);
        assert_eq!(groups.len(), 3);

        let thread_a = groups.iter().find(|g| g.subject == "Thread A").unwrap();
        assert_eq!(thread_a.message_uids, vec![1, 2]);

        let thread_b = groups.iter().find(|g| g.subject == "Thread B").unwrap();
        assert_eq!(thread_b.message_uids, vec![4, 5]);

        let standalone = groups.iter().find(|g| g.subject == "Standalone").unwrap();
        assert_eq!(standalone.message_uids, vec![3]);
    }

    #[test]
    fn test_sorted_by_latest_date_descending() {
        let messages = vec![
            msg(1, "<old@test.com>", None, None, "Old", 1000),
            msg(2, "<new@test.com>", None, None, "New", 5000),
            msg(3, "<mid@test.com>", None, None, "Mid", 3000),
        ];

        let groups = group_into_threads("acc1", &messages);
        assert_eq!(groups[0].subject, "New");
        assert_eq!(groups[1].subject, "Mid");
        assert_eq!(groups[2].subject, "Old");
    }

    #[test]
    fn test_angle_bracket_normalization() {
        let messages = vec![
            msg(1, "<msg1@test.com>", None, None, "Root", 1000),
            msg(2, "<msg2@test.com>", Some("msg1@test.com"), None, "Reply", 2000),
        ];

        let groups = group_into_threads("acc1", &messages);
        assert_eq!(groups.len(), 1, "should group even without angle brackets in In-Reply-To");
    }

    #[test]
    fn test_self_referencing_message_ignored() {
        let messages = vec![
            msg(1, "<msg1@test.com>", Some("<msg1@test.com>"), None, "Self-ref", 1000),
        ];

        let groups = group_into_threads("acc1", &messages);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].message_uids, vec![1]);
    }
}
