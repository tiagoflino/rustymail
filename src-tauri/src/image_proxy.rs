/// Remote image proxy/blocking — gives users control over remote image loading.
///
/// Three modes:
/// - "always": Load all remote images normally (no changes)
/// - "ask": Replace remote src with placeholder, show "Load Images" button
/// - "never": Strip all remote image src attributes
///
/// All processing is local. No external proxy server.

use regex_lite::Regex;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum ImageLoadMode {
    Always,
    Ask,
    Never,
}

impl ImageLoadMode {
    pub fn from_setting(s: &str) -> Self {
        match s {
            "ask" => ImageLoadMode::Ask,
            "never" => ImageLoadMode::Never,
            _ => ImageLoadMode::Always,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            ImageLoadMode::Always => "always",
            ImageLoadMode::Ask => "ask",
            ImageLoadMode::Never => "never",
        }
    }
}

/// Block remote images by rewriting their src attributes.
/// Returns (modified_html, count_blocked).
pub fn block_remote_images(html: &str) -> (String, usize) {
    let re_img = Regex::new(r#"(?i)<img([^>]*\bsrc\s*=\s*["']https?://[^"']+["'][^>]*)>"#);
    if re_img.is_err() {
        return (html.to_string(), 0);
    }
    let re_img = re_img.unwrap();

    let mut result = String::with_capacity(html.len());
    let mut last_end = 0;
    let mut count = 0usize;

    for m in re_img.find_iter(html) {
        let start = m.start();
        let end = m.end();
        result.push_str(&html[last_end..start]);
        let tag = m.as_str();
        // Replace src= with data-blocked-src= and add a placeholder
        let blocked = tag
            .replace("src=", "data-blocked-src=")
            .replace("src =", "data-blocked-src =");
        result.push_str("<!-- remote image blocked -->");
        count += 1;
        last_end = end;
    }
    result.push_str(&html[last_end..]);

    (result, count)
}

/// Extract all remote image URLs from HTML for per-sender allowlisting.
pub fn extract_remote_image_urls(html: &str) -> Vec<String> {
    let re = Regex::new(r#"(?i)<img[^>]*\bsrc\s*=\s*["'](https?://[^"']+)["'][^>]*>"#);
    if re.is_err() {
        return vec![];
    }
    let re = re.unwrap();
    re.captures_iter(html)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_remote_images_basic() {
        let html = r#"<html><body><img src="https://cdn.example.com/logo.png"><p>Hello</p></body></html>"#;
        let (result, count) = block_remote_images(html);
        assert_eq!(count, 1);
        assert!(!result.contains(r#"src="https://cdn.example.com/logo.png""#));
        assert!(result.contains("remote image blocked"));
        assert!(result.contains("<p>Hello</p>"));
    }

    #[test]
    fn test_block_remote_images_preserves_local() {
        let html = r#"<img src="cid:attachment123"><img src="https://remote.com/img.jpg">"#;
        let (result, count) = block_remote_images(html);
        assert_eq!(count, 1);
        assert!(result.contains("cid:attachment123"));
    }

    #[test]
    fn test_block_remote_images_no_remote() {
        let html = r#"<p>Plain text email</p>"#;
        let (result, count) = block_remote_images(html);
        assert_eq!(count, 0);
        assert_eq!(result, html);
    }

    #[test]
    fn test_block_remote_images_empty() {
        let (result, count) = block_remote_images("");
        assert_eq!(count, 0);
        assert_eq!(result, "");
    }

    #[test]
    fn test_extract_remote_image_urls() {
        let html = r#"<img src="https://cdn.com/a.jpg"><img src="https://track.com/pixel.gif"><img src="cid:inline">"#;
        let urls = extract_remote_image_urls(html);
        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"https://cdn.com/a.jpg".to_string()));
    }

    #[test]
    fn test_image_load_mode_from_setting() {
        assert_eq!(ImageLoadMode::from_setting("always"), ImageLoadMode::Always);
        assert_eq!(ImageLoadMode::from_setting("ask"), ImageLoadMode::Ask);
        assert_eq!(ImageLoadMode::from_setting("never"), ImageLoadMode::Never);
        assert_eq!(ImageLoadMode::from_setting("unknown"), ImageLoadMode::Always);
    }

    #[test]
    fn test_image_load_mode_as_str() {
        assert_eq!(ImageLoadMode::Always.as_str(), "always");
        assert_eq!(ImageLoadMode::Ask.as_str(), "ask");
        assert_eq!(ImageLoadMode::Never.as_str(), "never");
    }
}
