use lettre::message::{
    header::ContentType, Attachment as LettreAttachment, Mailbox, Message, MultiPart, SinglePart,
};
use lettre::Address;
use std::str::FromStr;

#[derive(Debug)]
pub struct AttachmentFile {
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

const MAX_TOTAL_ATTACHMENT_SIZE: u64 = 25 * 1024 * 1024;

pub fn read_attachment_files(paths: &[String]) -> Result<Vec<AttachmentFile>, String> {
    let mut files = Vec::new();
    let mut total_size: u64 = 0;

    for path_str in paths {
        let path = std::path::Path::new(path_str);
        let metadata = std::fs::metadata(path)
            .map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;
        total_size += metadata.len();
        if total_size > MAX_TOTAL_ATTACHMENT_SIZE {
            return Err("Total attachment size exceeds Gmail's 25MB limit.".to_string());
        }
        let data = std::fs::read(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("attachment")
            .to_string();
        let mime_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        files.push(AttachmentFile {
            filename,
            mime_type,
            data,
        });
    }
    Ok(files)
}

pub fn sanitize_email_html(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }

    let mut result = raw.to_string();

    let strip_patterns: &[&str] = &[
        r"(?i)<!DOCTYPE[^>]*>",
        r"(?is)<title[^>]*>.*?</title>",
        r"(?i)<(html|head|body)[^>]*>",
        r"(?i)</(html|head|body)>",
        r"(?i)<meta[^>]*/?>",
        r"(?i)<base[^>]*>",
    ];
    for pat in strip_patterns {
        let re = regex_lite::Regex::new(pat).unwrap();
        result = re.replace_all(&result, "").to_string();
    }

    let script_re = regex_lite::Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap();
    loop {
        let cleaned = script_re.replace_all(&result, "").to_string();
        if cleaned == result {
            break;
        }
        result = cleaned;
    }
    let orphan_script_re = regex_lite::Regex::new(r"(?i)</?script[^>]*>").unwrap();
    result = orphan_script_re.replace_all(&result, "").to_string();

    let event_re = regex_lite::Regex::new(r#"(?i)\s+on\w+\s*=\s*"[^"]*""#).unwrap();
    result = event_re.replace_all(&result, "").to_string();
    let event_re2 = regex_lite::Regex::new(r"(?i)\s+on\w+\s*=\s*'[^']*'").unwrap();
    result = event_re2.replace_all(&result, "").to_string();

    let js_url_re = regex_lite::Regex::new(r#"(?i)href\s*=\s*"javascript:[^"]*""#).unwrap();
    result = js_url_re.replace_all(&result, r#"href="""#).to_string();
    let js_url_re2 = regex_lite::Regex::new(r"(?i)href\s*=\s*'javascript:[^']*'").unwrap();
    result = js_url_re2.replace_all(&result, "href=''").to_string();

    let dangerous_tags: &[&str] = &[
        r"(?is)<iframe[^>]*>.*?</iframe>",
        r"(?i)<iframe[^>]*/?>",
        r"(?is)<object[^>]*>.*?</object>",
        r"(?i)<object[^>]*/?>",
        r"(?is)<embed[^>]*>.*?</embed>",
        r"(?i)<embed[^>]*/?>",
        r"(?is)<applet[^>]*>.*?</applet>",
        r"(?i)<applet[^>]*/?>",
        r"(?is)<form[^>]*>.*?</form>",
        r"(?i)<form[^>]*/?>",
        r"(?i)<input[^>]*/?>",
        r"(?i)<button[^>]*>.*?</button>",
    ];
    for pat in dangerous_tags {
        let re = regex_lite::Regex::new(pat).unwrap();
        result = re.replace_all(&result, "").to_string();
    }

    result
}

fn parse_to_mailbox(raw: &str) -> Result<Mailbox, String> {
    let raw = raw.trim();
    if let (Some(start), Some(end)) = (raw.find('<'), raw.rfind('>')) {
        let email_part = raw[start + 1..end].trim();
        let display_part = raw[..start].trim().trim_matches('"').trim();
        let address = Address::from_str(email_part)
            .map_err(|e| format!("Invalid email address '{}': {}", email_part, e))?;
        Ok(if display_part.is_empty() {
            Mailbox::new(None, address)
        } else {
            Mailbox::new(Some(display_part.to_string()), address)
        })
    } else {
        let address =
            Address::from_str(raw).map_err(|e| format!("Invalid To address '{}': {}", raw, e))?;
        Ok(Mailbox::new(None, address))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_mime_message(
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
    in_reply_to: Option<&str>,
    references: Option<&str>,
    allow_empty_to: bool,
    attachments: &[AttachmentFile],
) -> Result<Message, String> {
    let from_mailbox = Mailbox::from_str(from).map_err(|_| "Invalid From address")?;

    let recipients: Vec<Mailbox> = to
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse_to_mailbox)
        .collect::<Result<_, _>>()?;

    let mut builder = Message::builder().from(from_mailbox.clone()).subject(subject);

    if recipients.is_empty() {
        if allow_empty_to {
            let from_addr = from_mailbox.email.clone();
            let envelope = lettre::address::Envelope::new(Some(from_addr.clone()), vec![from_addr])
                .map_err(|e| e.to_string())?;
            builder = builder.envelope(envelope);
        } else {
            return Err(
                "No valid recipients. Please specify at least one recipient to send.".to_string(),
            );
        }
    } else {
        for mailbox in recipients {
            builder = builder.to(mailbox);
        }
    }

    if let Some(irt) = in_reply_to {
        if !irt.is_empty() {
            builder = builder.header(lettre::message::header::InReplyTo::from(irt.to_string()));
        }
    }
    if let Some(refs) = references {
        if !refs.is_empty() {
            builder = builder.header(lettre::message::header::References::from(refs.to_string()));
        }
    }

    let email = if attachments.is_empty() {
        builder
            .header(ContentType::TEXT_HTML)
            .body(body.to_string())
            .map_err(|e| e.to_string())?
    } else {
        let html_part = SinglePart::builder()
            .header(ContentType::TEXT_HTML)
            .body(body.to_string());

        let mut multipart = MultiPart::mixed().singlepart(html_part);

        for att in attachments {
            let content_type = ContentType::parse(&att.mime_type)
                .unwrap_or(ContentType::parse("application/octet-stream").unwrap());
            let attachment_part =
                LettreAttachment::new(att.filename.clone()).body(att.data.clone(), content_type);
            multipart = multipart.singlepart(attachment_part);
        }

        builder.multipart(multipart).map_err(|e| e.to_string())?
    };

    Ok(email)
}

pub fn mime_to_gmail_raw(message: &Message) -> String {
    base64::encode_config(message.formatted(), base64::URL_SAFE_NO_PAD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_parse_to_mailbox() {
        let mbox = parse_to_mailbox("John Doe <john@example.com>").unwrap();
        assert_eq!(mbox.name.as_deref(), Some("John Doe"));
        assert_eq!(mbox.email.to_string(), "john@example.com");

        let mbox = parse_to_mailbox("john@example.com").unwrap();
        assert!(mbox.name.is_none());
        assert_eq!(mbox.email.to_string(), "john@example.com");

        let mbox = parse_to_mailbox("\"Jane Doe\" <jane@test.com>").unwrap();
        assert_eq!(mbox.name.as_deref(), Some("Jane Doe"));

        assert!(parse_to_mailbox("not-an-email").is_err());
    }

    #[test]
    fn test_build_mime_message_basic() {
        let msg = build_mime_message(
            "sender@example.com",
            "recipient@example.com",
            "Test Subject",
            "<p>Hello</p>",
            None,
            None,
            false,
            &[],
        )
        .unwrap();

        let formatted = String::from_utf8_lossy(&msg.formatted()).to_string();
        assert!(formatted.contains("Test Subject"));
        assert!(formatted.contains("sender@example.com"));
        assert!(formatted.contains("recipient@example.com"));
        assert!(formatted.contains("<p>Hello</p>"));
    }

    #[test]
    fn test_build_mime_message_with_reply_headers() {
        let msg = build_mime_message(
            "sender@example.com",
            "recipient@example.com",
            "Re: Test",
            "<p>Reply</p>",
            Some("<original@example.com>"),
            Some("<original@example.com> <thread@example.com>"),
            false,
            &[],
        )
        .unwrap();

        let formatted = String::from_utf8_lossy(&msg.formatted()).to_string();
        assert!(formatted.contains("In-Reply-To"));
        assert!(formatted.contains("References"));
    }

    #[test]
    fn test_build_mime_message_allow_empty_to() {
        let result = build_mime_message(
            "sender@example.com",
            "",
            "Draft",
            "<p>Body</p>",
            None,
            None,
            true,
            &[],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_mime_message_reject_empty_to() {
        let result = build_mime_message(
            "sender@example.com",
            "",
            "Test",
            "<p>Body</p>",
            None,
            None,
            false,
            &[],
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No valid recipients"));
    }

    #[test]
    fn test_build_mime_message_unicode_subject() {
        let msg = build_mime_message(
            "sender@example.com",
            "recipient@example.com",
            "Héllo Wörld 🌍",
            "<p>body</p>",
            None,
            None,
            false,
            &[],
        )
        .unwrap();

        let formatted = String::from_utf8_lossy(&msg.formatted()).to_string();
        assert!(formatted.contains("Subject:"));
    }

    #[test]
    fn test_build_mime_message_with_attachment() {
        let attachment = AttachmentFile {
            filename: "test.txt".to_string(),
            mime_type: "text/plain".to_string(),
            data: b"file content".to_vec(),
        };

        let msg = build_mime_message(
            "sender@example.com",
            "recipient@example.com",
            "With Attachment",
            "<p>See attached</p>",
            None,
            None,
            false,
            &[attachment],
        )
        .unwrap();

        let formatted = String::from_utf8_lossy(&msg.formatted()).to_string();
        assert!(formatted.contains("multipart/mixed"));
        assert!(formatted.contains("test.txt"));
    }

    #[test]
    fn test_build_mime_message_with_multiple_attachments() {
        let attachments = vec![
            AttachmentFile {
                filename: "file1.txt".to_string(),
                mime_type: "text/plain".to_string(),
                data: b"content1".to_vec(),
            },
            AttachmentFile {
                filename: "file2.pdf".to_string(),
                mime_type: "application/pdf".to_string(),
                data: b"content2".to_vec(),
            },
        ];

        let msg = build_mime_message(
            "sender@example.com",
            "recipient@example.com",
            "Multiple",
            "<p>Files</p>",
            None,
            None,
            false,
            &attachments,
        )
        .unwrap();

        let formatted = String::from_utf8_lossy(&msg.formatted()).to_string();
        assert!(formatted.contains("file1.txt"));
        assert!(formatted.contains("file2.pdf"));
    }

    #[test]
    fn test_build_mime_message_no_attachments_unchanged() {
        let msg = build_mime_message(
            "sender@example.com",
            "recipient@example.com",
            "Plain",
            "<p>No attachments</p>",
            None,
            None,
            false,
            &[],
        )
        .unwrap();

        let formatted = String::from_utf8_lossy(&msg.formatted()).to_string();
        assert!(!formatted.contains("multipart"));
    }

    #[test]
    fn test_mime_to_gmail_raw_produces_base64() {
        let msg = build_mime_message(
            "sender@example.com",
            "recipient@example.com",
            "Test",
            "<p>Hello</p>",
            None,
            None,
            false,
            &[],
        )
        .unwrap();

        let raw = mime_to_gmail_raw(&msg);
        assert!(!raw.is_empty());
        assert!(!raw.contains('+'));
        assert!(!raw.contains('/'));
        let decoded = base64::decode_config(&raw, base64::URL_SAFE_NO_PAD).unwrap();
        let decoded_str = String::from_utf8_lossy(&decoded);
        assert!(decoded_str.contains("Test"));
    }

    #[test]
    fn test_read_attachment_files_valid() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "hello world").unwrap();

        let result = read_attachment_files(&[file_path.to_string_lossy().to_string()]);
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].filename, "test.txt");
        assert_eq!(files[0].data, b"hello world");
        assert_eq!(files[0].mime_type, "text/plain");
    }

    #[test]
    fn test_read_attachment_files_size_limit() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("big.bin");
        let data = vec![0u8; 26 * 1024 * 1024];
        std::fs::write(&file_path, &data).unwrap();

        let result = read_attachment_files(&[file_path.to_string_lossy().to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("25MB"));
    }

    #[test]
    fn test_read_attachment_files_nonexistent() {
        let result = read_attachment_files(&["/nonexistent/file.txt".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_attachment_files_empty() {
        let result = read_attachment_files(&[]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_sanitize_strips_script_tags() {
        let input = "<p>Hello</p><script>alert('xss')</script><p>World</p>";
        let result = sanitize_email_html(input);
        assert!(!result.contains("script"));
        assert!(result.contains("<p>Hello</p>"));
        assert!(result.contains("<p>World</p>"));
    }

    #[test]
    fn test_sanitize_strips_event_handlers() {
        let input = r#"<div onclick="alert('xss')">test</div>"#;
        let result = sanitize_email_html(input);
        assert!(!result.contains("onclick"));
        assert!(result.contains("<div"));
    }

    #[test]
    fn test_sanitize_strips_iframe_and_object() {
        let input = "<iframe src='evil.com'></iframe><object data='bad.swf'></object>";
        let result = sanitize_email_html(input);
        assert!(!result.contains("iframe"));
        assert!(!result.contains("object"));
    }

    #[test]
    fn test_sanitize_preserves_email_tables() {
        let input = r#"<table cellpadding="0"><tr><td style="padding:10px">Content</td></tr></table>"#;
        let result = sanitize_email_html(input);
        assert!(result.contains("<table"));
        assert!(result.contains("<td"));
        assert!(result.contains("Content"));
    }

    #[test]
    fn test_sanitize_preserves_links() {
        let input = r#"<a href="https://example.com">Click</a>"#;
        let result = sanitize_email_html(input);
        assert!(result.contains("https://example.com"));
        assert!(result.contains("Click"));
    }

    #[test]
    fn test_sanitize_strips_form_elements() {
        let input = "<form action='/steal'><input type='text'/><button>Submit</button></form>";
        let result = sanitize_email_html(input);
        assert!(!result.contains("form"));
        assert!(!result.contains("input"));
        assert!(!result.contains("button"));
    }

    #[test]
    fn test_sanitize_empty_input() {
        assert_eq!(sanitize_email_html(""), "");
    }

    #[test]
    fn test_sanitize_strips_javascript_urls() {
        let input = r#"<a href="javascript:alert(1)">Click</a>"#;
        let result = sanitize_email_html(input);
        assert!(!result.contains("javascript"));
    }

    #[test]
    fn test_sanitize_preserves_mailto_links() {
        let input = r#"<a href="mailto:test@example.com">Email</a>"#;
        let result = sanitize_email_html(input);
        assert!(result.contains("mailto:test@example.com"));
    }

    #[test]
    fn test_sanitize_preserves_http_and_https_images() {
        let input = r#"<img src="https://cdn.example.com/logo.png"/>"#;
        let result = sanitize_email_html(input);
        assert!(result.contains("https://cdn.example.com/logo.png"));
    }

    #[test]
    fn test_sanitize_preserves_data_uri_img() {
        let input = r#"<img src="data:image/png;base64,iVBOR"/>"#;
        let result = sanitize_email_html(input);
        assert!(result.contains("data:image/png;base64,iVBOR"));
    }

    #[test]
    fn test_sanitize_preserves_inline_style_attr() {
        let input = r#"<td style="color: red; font-size: 14px;">Text</td>"#;
        let result = sanitize_email_html(input);
        assert!(result.contains("style="));
        assert!(result.contains("color: red"));
    }

    #[test]
    fn test_sanitize_strips_nested_obfuscated_scripts() {
        let input = "<scr<script>ipt>alert(1)</scr</script>ipt>";
        let result = sanitize_email_html(input);
        assert!(!result.contains("alert"));
    }

    #[test]
    fn test_sanitize_strips_svg_script() {
        let input = r#"<svg><script>alert(1)</script></svg>"#;
        let result = sanitize_email_html(input);
        assert!(!result.contains("alert"));
    }

    #[test]
    fn test_sanitize_strips_base_tag() {
        let input = r#"<base href="https://evil.com"><a href="/page">Link</a>"#;
        let result = sanitize_email_html(input);
        assert!(!result.contains("<base"));
    }

    #[test]
    fn test_sanitize_preserves_css_url_properties() {
        let input = r#"<style>
            .header { background: url('https://cdn.example.com/bg.png'); }
        </style>"#;
        let result = sanitize_email_html(input);
        assert!(result.contains("url('https://cdn.example.com/bg.png')"));
    }

    #[test]
    fn test_sanitize_preserves_td_background_attr() {
        let input = r##"<td background="https://cdn.example.com/bg.png" bgcolor="#ffffff">Content</td>"##;
        let result = sanitize_email_html(input);
        assert!(result.contains("background="));
        assert!(result.contains("bgcolor="));
    }

    #[test]
    fn test_sanitize_inlines_style_rules_and_keeps_style_tag() {
        let input = r#"<style>.red { color: red; }</style><p class="red">Hello</p>"#;
        let result = sanitize_email_html(input);
        assert!(result.contains("<style>"));
        assert!(result.contains("<p"));
    }
}
