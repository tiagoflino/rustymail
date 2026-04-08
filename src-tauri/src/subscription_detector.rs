use regex_lite::Regex;

#[derive(Debug, Clone)]
pub struct DetectionInput<'a> {
    pub headers: Vec<(&'a str, &'a str)>,
    pub body_plain: Option<&'a str>,
    pub body_html: Option<&'a str>,
    pub sender: &'a str,
}

#[derive(Debug, Clone)]
pub struct DetectionResult {
    pub is_subscription: bool,
    pub confidence: f32,
    pub methods: Vec<String>,
    pub details: String,
    pub unsubscribe_url: Option<String>,
    pub unsubscribe_mailto: Option<String>,
    pub supports_one_click: bool,
    pub sender_email: String,
    pub sender_name: Option<String>,
}

fn parse_sender(sender: &str) -> (String, Option<String>) {
    let sender = sender.trim();
    if let Some(start) = sender.find('<') {
        if let Some(end) = sender.find('>') {
            let name_part = sender[..start].trim();
            let email_part = sender[start + 1..end].trim().to_string();
            let name = if name_part.is_empty() {
                None
            } else {
                Some(name_part.trim_matches('"').to_string())
            };
            return (email_part, name);
        }
    }
    (sender.to_string(), None)
}

fn extract_list_unsubscribe_url(header_value: &str) -> (Option<String>, Option<String>) {
    let mut url = None;
    let mut mailto = None;

    for part in header_value.split(',') {
        let part = part.trim();
        if part.starts_with('<') && part.ends_with('>') {
            let inner = &part[1..part.len()-1];
            if inner.starts_with("http://") || inner.starts_with("https://") {
                url = Some(inner.to_string());
            } else if inner.contains('@') {
                let mail = if inner.starts_with("mailto:") {
                    inner.strip_prefix("mailto:").unwrap_or(inner).to_string()
                } else {
                    inner.to_string()
                };
                mailto = Some(mail);
            }
        } else if part.starts_with("http://") || part.starts_with("https://") {
            url = Some(part.to_string());
        } else if part.contains('@') {
            let mail = if part.starts_with("mailto:") {
                part.strip_prefix("mailto:").unwrap_or(part).to_string()
            } else {
                part.to_string()
            };
            mailto = Some(mail);
        }
    }

    (url, mailto)
}

pub fn detect(input: &DetectionInput) -> DetectionResult {
    let mut confidence = 0.0f32;
    let mut methods = Vec::new();
    let mut details = String::new();
    let mut unsubscribe_url = None;
    let mut unsubscribe_mailto = None;
    let mut supports_one_click = false;

    let (sender_email, sender_name) = parse_sender(input.sender);

    let header_map: std::collections::HashMap<&str, &str> =
        input.headers.iter().map(|(k, v)| (*k, *v)).collect();

    if let Some(list_unsubscribe) = header_map.get("List-Unsubscribe") {
        confidence += 0.7;
        methods.push("List-Unsubscribe header".to_string());
        if details.is_empty() {
            details = format!("List-Unsubscribe: {}", list_unsubscribe);
        }
        let (url, mail) = extract_list_unsubscribe_url(list_unsubscribe);
        if url.is_some() {
            unsubscribe_url = url;
        }
        if mail.is_some() {
            unsubscribe_mailto = mail;
        }
    }

    if let Some(list_unsubscribe_post) = header_map.get("List-Unsubscribe-Post") {
        if list_unsubscribe_post.contains("List-Unsubscribe=One-Click") {
            supports_one_click = true;
            methods.push("One-Click Unsubscribe".to_string());
            confidence += 0.1;
        }
    }

    if header_map.contains_key("List-Id") {
        confidence += 0.3;
        methods.push("List-Id header".to_string());
    }

    if let Some(precedence) = header_map.get("Precedence") {
        if *precedence == "bulk" || *precedence == "list" {
            confidence += 0.3;
            methods.push("Precedence header".to_string());
        }
    }

    let esp_headers = [
        ("X-MC-User", "Mailchimp"),
        ("X-SG-", "SendGrid"),
        ("X-Mailgun-", "Mailgun"),
        ("X-campaignid", "Campaign"),
        ("X-PM-Message-Id", "Postmark"),
    ];

    for (header, esp) in esp_headers.iter() {
        for (key, _) in input.headers.iter() {
            if key.starts_with(header) || *key == *header {
                confidence += 0.5;
                methods.push(format!("{} header", esp));
                break;
            }
        }
    }

    if let Some(mailer) = header_map.get("X-Mailer") {
        let mailer_lower = mailer.to_lowercase();
        if mailer_lower.contains("substack") {
            confidence += 0.5;
            methods.push("Substack mailer".to_string());
        }
    }

    if let Some(feedback_id) = header_map.get("feedback-id") {
        let feedback_lower = feedback_id.to_lowercase();
        if feedback_lower.contains("sendgrid")
            || feedback_lower.contains("mailchimp")
            || feedback_lower.contains("mailgun")
        {
            confidence += 0.4;
            methods.push("ESP feedback-id".to_string());
        }
    }

    let sender_lower = sender_email.to_lowercase();
    let known_esps = [
        "substack.com",
        "email.mg.",
        "sendgrid.net",
        "mandrillapp.com",
    ];
    for esp in known_esps.iter() {
        if sender_lower.contains(esp) {
            confidence += 0.5;
            methods.push(format!("Known ESP domain ({})", esp));
            break;
        }
    }

    let unsubscribe_link_re =
        Regex::new(r"(?i)<a[^>]*href\s*=\s*([^\s>]+)[^>]*>.*?unsubscribe").ok();
    let has_unsubscribe_link = if let Some(body) = input.body_html {
        let body_lower = body.to_lowercase();
        if body_lower.contains("unsubscribe") {
            if let Some(re) = &unsubscribe_link_re {
                re.is_match(body)
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    if has_unsubscribe_link {
        confidence += 0.3;
        methods.push("Unsubscribe link in HTML body".to_string());
    }

    if let Some(body) = input.body_html {
        let pixel_re = Regex::new(r#"(?i)<img[^>]+width\s*=\s*['"]?1['"]?[^>]*>"#).ok();
        let pixel_re2 = Regex::new(r#"(?i)<img[^>]+height\s*=\s*['"]?1['"]?[^>]*>"#).ok();

        let mut has_tracking_pixel = false;
        if let Some(re) = &pixel_re {
            if re.is_match(body) {
                has_tracking_pixel = true;
            }
        }
        if !has_tracking_pixel {
            if let Some(re) = &pixel_re2 {
                if re.is_match(body) {
                    has_tracking_pixel = true;
                }
            }
        }
        if has_tracking_pixel {
            confidence += 0.15;
            methods.push("Tracking pixel (1x1)".to_string());
        }
    }

    if let Some(body) = input.body_html.or(input.body_plain) {
        let can_spam_re =
            Regex::new(r"(?is)unsubscribe.*?(?:address|postal|street|city|state|zip|phone)").ok();
        if let Some(re) = &can_spam_re {
            if re.is_match(body) {
                confidence += 0.15;
                methods.push("CAN-SPAM footer pattern".to_string());
            }
        }
    }

    if confidence > 1.0 {
        confidence = 1.0;
    }

    let is_subscription = confidence >= 0.3;

    DetectionResult {
        is_subscription,
        confidence,
        methods,
        details,
        unsubscribe_url,
        unsubscribe_mailto,
        supports_one_click,
        sender_email,
        sender_name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_list_unsubscribe_header() {
        let input = DetectionInput {
            headers: vec![("List-Unsubscribe", "<https://example.com/unsubscribe>")],
            body_plain: None,
            body_html: None,
            sender: "sender@example.com",
        };
        let result = detect(&input);
        assert!(result.is_subscription);
        assert!(result
            .methods
            .iter()
            .any(|m| m.contains("List-Unsubscribe")));
    }

    #[test]
    fn test_detect_list_unsubscribe_extracts_url() {
        let input = DetectionInput {
            headers: vec![(
                "List-Unsubscribe",
                "<https://example.com/unsubscribe>, <mailto:unsub@example.com>",
            )],
            body_plain: None,
            body_html: None,
            sender: "sender@example.com",
        };
        let result = detect(&input);
        assert_eq!(
            result.unsubscribe_url,
            Some("https://example.com/unsubscribe".to_string())
        );
        assert_eq!(
            result.unsubscribe_mailto,
            Some("unsub@example.com".to_string())
        );
    }

    #[test]
    fn test_detect_list_unsubscribe_extracts_mailto() {
        let input = DetectionInput {
            headers: vec![("List-Unsubscribe", "unsubscribe@example.com")],
            body_plain: None,
            body_html: None,
            sender: "sender@example.com",
        };
        let result = detect(&input);
        assert_eq!(
            result.unsubscribe_mailto,
            Some("unsubscribe@example.com".to_string())
        );
    }

    #[test]
    fn test_detect_one_click_unsubscribe() {
        let input = DetectionInput {
            headers: vec![
                ("List-Unsubscribe", "<https://example.com/unsubscribe>"),
                ("List-Unsubscribe-Post", "List-Unsubscribe=One-Click"),
            ],
            body_plain: None,
            body_html: None,
            sender: "sender@example.com",
        };
        let result = detect(&input);
        assert!(result.supports_one_click);
        assert!(result.methods.iter().any(|m| m.contains("One-Click")));
    }

    #[test]
    fn test_detect_precedence_bulk() {
        let input = DetectionInput {
            headers: vec![("Precedence", "bulk")],
            body_plain: None,
            body_html: None,
            sender: "sender@example.com",
        };
        let result = detect(&input);
        assert!(result.is_subscription);
        assert!(result.methods.iter().any(|m| m.contains("Precedence")));
    }

    #[test]
    fn test_detect_esp_mailchimp() {
        let input = DetectionInput {
            headers: vec![("X-MC-User", "abc123")],
            body_plain: None,
            body_html: None,
            sender: "sender@example.com",
        };
        let result = detect(&input);
        assert!(result.is_subscription);
        assert!(result.methods.iter().any(|m| m.contains("Mailchimp")));
    }

    #[test]
    fn test_detect_esp_sendgrid() {
        let input = DetectionInput {
            headers: vec![("X-SG-Message-Id", "abc123")],
            body_plain: None,
            body_html: None,
            sender: "sender@example.com",
        };
        let result = detect(&input);
        assert!(result.is_subscription);
        assert!(result.methods.iter().any(|m| m.contains("SendGrid")));
    }

    #[test]
    fn test_detect_esp_substack() {
        let input = DetectionInput {
            headers: vec![],
            body_plain: None,
            body_html: None,
            sender: "newsletter@substack.com",
        };
        let result = detect(&input);
        assert!(result.is_subscription);
        assert!(result.methods.iter().any(|m| m.contains("substack")));
    }

    #[test]
    fn test_detect_body_unsubscribe_link() {
        let input = DetectionInput {
            headers: vec![],
            body_plain: Some("Plain text email"),
            body_html: Some(
                "<html><body><a href='https://unsub.com'>unsubscribe</a></body></html>",
            ),
            sender: "sender@example.com",
        };
        let result = detect(&input);
        assert!(result.is_subscription);
        assert!(result
            .methods
            .iter()
            .any(|m| m.contains("Unsubscribe link")));
    }

    #[test]
    fn test_detect_tracking_pixel() {
        let input = DetectionInput {
            headers: vec![],
            body_plain: None,
            body_html: Some("<html><body><img src='pixel' width='1' height='1'></body></html>"),
            sender: "sender@example.com",
        };
        let result = detect(&input);
        assert!(result.methods.iter().any(|m| m.contains("Tracking pixel")));
    }

    #[test]
    fn test_no_detection_normal_email() {
        let input = DetectionInput {
            headers: vec![
                ("From", "John Doe <john@example.com>"),
                ("Subject", "Hello"),
            ],
            body_plain: Some("Hey, let's meet for lunch!"),
            body_html: Some("<html><body>Hey, let's meet for lunch!</body></html>"),
            sender: "John Doe <john@example.com>",
        };
        let result = detect(&input);
        assert!(!result.is_subscription);
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_combined_confidence() {
        let input = DetectionInput {
            headers: vec![
                ("List-Unsubscribe", "<https://example.com/unsub>"),
                ("List-Id", "<list.example.com>"),
                ("Precedence", "bulk"),
            ],
            body_plain: None,
            body_html: None,
            sender: "sender@example.com",
        };
        let result = detect(&input);
        assert_eq!(result.confidence, 1.0);
        assert!(result.is_subscription);
    }

    #[test]
    fn test_sender_parsing() {
        let input = DetectionInput {
            headers: vec![],
            body_plain: None,
            body_html: None,
            sender: "John Doe <john@example.com>",
        };
        let result = detect(&input);
        assert_eq!(result.sender_email, "john@example.com");
        assert_eq!(result.sender_name, Some("John Doe".to_string()));
    }

    #[test]
    fn test_sender_email_only() {
        let input = DetectionInput {
            headers: vec![],
            body_plain: None,
            body_html: None,
            sender: "john@example.com",
        };
        let result = detect(&input);
        assert_eq!(result.sender_email, "john@example.com");
        assert_eq!(result.sender_name, None);
    }
}
