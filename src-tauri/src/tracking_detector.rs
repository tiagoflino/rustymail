use regex_lite::Regex;

/// Represents a detected tracking element in an email
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DetectedTracker {
    pub tracker_type: TrackerType,
    pub details: String,
    pub url_snippet: String,
    pub blocked: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum TrackerType {
    TrackingPixel,
    RemoteImage,
    ReadReceipt,
    TrackingLink,
}

impl TrackerType {
    pub fn as_str(&self) -> &str {
        match self {
            TrackerType::TrackingPixel => "tracking_pixel",
            TrackerType::RemoteImage => "remote_image",
            TrackerType::ReadReceipt => "read_receipt",
            TrackerType::TrackingLink => "tracking_link",
        }
    }
}

/// Scan HTML content for tracking elements
pub fn detect_trackers(html: &str) -> Vec<DetectedTracker> {
    let mut trackers = Vec::new();

    // 1. Detect 1x1 tracking pixels
    detect_tracking_pixels(html, &mut trackers);

    // 2. Detect remote images with tracking parameters
    detect_tracking_params(html, &mut trackers);

    // 3. Detect read receipt patterns
    detect_read_receipts(html, &mut trackers);

    trackers
}

fn detect_tracking_pixels(html: &str, trackers: &mut Vec<DetectedTracker>) {
    // Pattern: <img ... width="1" height="1" ...>
    let re_1x1_width = Regex::new(r#"(?i)<img[^>]*\bwidth\s*=\s*["']?1["']?[^>]*>"#);
    let re_1x1_height = Regex::new(r#"(?i)<img[^>]*\bheight\s*=\s*["']?1["']?[^>]*>"#);

    if let Ok(re) = &re_1x1_width {
        for cap in re.find_iter(html) {
            let tag = cap.as_str();
            if let Ok(re_h) = &re_1x1_height {
                if re_h.is_match(tag) {
                    let url_snippet = extract_src(tag);
                    trackers.push(DetectedTracker {
                        tracker_type: TrackerType::TrackingPixel,
                        details: "1x1 tracking pixel".to_string(),
                        url_snippet,
                        blocked: false,
                    });
                }
            }
        }
    }

    // Detect transparent/beacon images by common patterns
    let re_beacon = Regex::new(r#"(?i)<img[^>]*\b(?:src)=["']([^"']*(?:pixel|beacon|track|open|read)[^"']*)["'][^>]*>"#);
    if let Ok(re) = &re_beacon {
        for cap in re.captures_iter(html) {
            let full_tag = cap.get(0).map(|m| m.as_str()).unwrap_or("");
            let url = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            // Avoid duplicates with 1x1 detection
            if !trackers.iter().any(|t| t.url_snippet == url) {
                trackers.push(DetectedTracker {
                    tracker_type: TrackerType::TrackingPixel,
                    details: "Beacon image (pixel/track/open)".to_string(),
                    url_snippet: url.to_string(),
                    blocked: false,
                });
            }
        }
    }
}

fn detect_tracking_params(html: &str, trackers: &mut Vec<DetectedTracker>) {
    // Images with UTM or tracking query parameters
    let re_tracking_url = Regex::new(
        r#"(?i)<img[^>]*\bsrc=["']([^"']*(?:utm_|utm-|tracking|_ga=|mc_cid|mc_eid|ml_subscriber|oly_enc_id)[^"']*)["'][^>]*>"#
    );

    if let Ok(re) = &re_tracking_url {
        for cap in re.captures_iter(html) {
            let url = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            trackers.push(DetectedTracker {
                tracker_type: TrackerType::RemoteImage,
                details: "Image with tracking parameters".to_string(),
                url_snippet: truncate_url(url, 100),
                blocked: false,
            });
        }
    }
}

fn detect_read_receipts(html: &str, trackers: &mut Vec<DetectedTracker>) {
    // Detect Disposition-Notification-To patterns (would be in headers, but check body meta)
    let re_receipt = Regex::new(
        r#"(?i)<meta[^>]*\b(?:name|http-equiv)=["']?(?:disposition-notification-to|x-dln|return-receipt-to)["']?[^>]*>"#
    );

    if let Ok(re) = &re_receipt {
        for cap in re.find_iter(html) {
            trackers.push(DetectedTracker {
                tracker_type: TrackerType::ReadReceipt,
                details: "Read receipt meta tag".to_string(),
                url_snippet: truncate_url(cap.as_str(), 80),
                blocked: false,
            });
        }
    }

    // Detect CSS background-image with tracking URLs (hidden elements)
    let re_css_bg = Regex::new(
        r#"(?i)(?:background|background-image)\s*:\s*url\(["']?([^)"']*(?:pixel|beacon|track|open)[^)"']*)["']?\)"#
    );

    if let Ok(re) = &re_css_bg {
        for cap in re.captures_iter(html) {
            let url = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            trackers.push(DetectedTracker {
                tracker_type: TrackerType::TrackingPixel,
                details: "CSS background tracking image".to_string(),
                url_snippet: truncate_url(url, 100),
                blocked: false,
            });
        }
    }
}

/// Rewrite HTML to block detected trackers — replaces tracking image src with a placeholder
pub fn block_trackers(html: &str) -> (String, usize) {
    let mut blocked_count = 0usize;
    let mut result = html.to_string();

    // Block 1x1 images by rewriting src to data-blocked-src
    let re_1x1 = Regex::new(r#"(?i)<img[^>]*width\s*=\s*["']?1["']?[^>]*height\s*=\s*["']?1["']?[^>]*>"#);
    if let Ok(re) = &re_1x1 {
        let mut new_html = String::with_capacity(result.len());
        let mut last_end = 0;
        for m in re.find_iter(&result) {
            let start = m.start();
            let end = m.end();
            new_html.push_str(&result[last_end..start]);
            let tag = m.as_str();
            let blocked_tag = str::replace(tag, "src=", "data-blocked-src=");
            new_html.push_str(&blocked_tag);
            blocked_count += 1;
            last_end = end;
        }
        new_html.push_str(&result[last_end..]);
        result = new_html;
    }

    // Block beacon/pixel images (contains "pixel", "beacon", "track", "open" in URL)
    let re_beacon = Regex::new(r#"(?i)<img[^>]*src\s*=\s*["'][^"']*(?:pixel|beacon|track|open)[^"']*["'][^>]*>"#);
    if let Ok(re) = &re_beacon {
        let mut new_html = String::with_capacity(result.len());
        let mut last_end = 0;
        for m in re.find_iter(&result) {
            let start = m.start();
            let end = m.end();
            new_html.push_str(&result[last_end..start]);
            let tag = m.as_str();
            // Only block if not already handled
            if tag.contains("data-blocked-src") {
                new_html.push_str(tag);
            } else {
                let blocked_tag = str::replace(tag, "src=", "data-blocked-src=");
                new_html.push_str(&blocked_tag);
                blocked_count += 1;
            }
            last_end = end;
        }
        new_html.push_str(&result[last_end..]);
        result = new_html;
    }

    (result, blocked_count)
}

fn extract_src(tag: &str) -> String {
    let re = Regex::new(r#"(?i)src\s*=\s*["']([^"']*)["']"#);
    if let Ok(re) = &re {
        if let Some(cap) = re.captures(tag) {
            if let Some(src) = cap.get(1) {
                return truncate_url(src.as_str(), 100);
            }
        }
    }
    "(no src)".to_string()
}

fn truncate_url(url: &str, max_len: usize) -> String {
    if url.len() <= max_len {
        url.to_string()
    } else {
        format!("{}...", &url[..max_len - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_1x1_tracking_pixel() {
        let html = r#"<html><body><img src="https://track.example.com/pixel.gif" width="1" height="1"></body></html>"#;
        let trackers = detect_trackers(html);
        assert!(trackers.iter().any(|t| matches!(t.tracker_type, TrackerType::TrackingPixel)));
        assert!(trackers.iter().any(|t| t.url_snippet.contains("track.example.com")));
    }

    #[test]
    fn test_detect_tracking_pixel_no_quotes() {
        let html = r#"<img src="https://a.com/p.gif" width=1 height=1>"#;
        let trackers = detect_trackers(html);
        assert!(trackers.iter().any(|t| matches!(t.tracker_type, TrackerType::TrackingPixel)));
    }

    #[test]
    fn test_detect_beacon_image() {
        let html = r#"<img src="https://mail.example.com/track/open/abc123" width="0" height="0">"#;
        let trackers = detect_trackers(html);
        assert!(trackers.iter().any(|t| t.url_snippet.contains("track/open")));
    }

    #[test]
    fn test_detect_utm_tracking_params() {
        let html = r#"<img src="https://example.com/logo.png?utm_source=newsletter&utm_campaign=test" width="200">"#;
        let trackers = detect_trackers(html);
        assert!(trackers.iter().any(|t| matches!(t.tracker_type, TrackerType::RemoteImage)));
    }

    #[test]
    fn test_detect_mailchimp_tracking() {
        let html = r#"<img src="https://list-manage.com/track/open.php?u=abc123&id=xyz789">"#;
        let trackers = detect_trackers(html);
        // Should be caught by beacon detection (contains "track" and "open")
        assert!(!trackers.is_empty());
    }

    #[test]
    fn test_detect_read_receipt_meta() {
        let html = r#"<meta http-equiv="Disposition-Notification-To" content="sender@example.com">"#;
        let trackers = detect_trackers(html);
        assert!(trackers.iter().any(|t| matches!(t.tracker_type, TrackerType::ReadReceipt)));
    }

    #[test]
    fn test_detect_css_background_tracking() {
        let html = r#"<div style="background: url('https://track.com/pixel.png')"></div>"#;
        let trackers = detect_trackers(html);
        assert!(trackers.iter().any(|t| t.url_snippet.contains("track.com/pixel.png")));
    }

    #[test]
    fn test_no_trackers_clean_email() {
        let html = r#"<html><body><p>Hello, how are you?</p></body></html>"#;
        let trackers = detect_trackers(html);
        assert!(trackers.is_empty());
    }

    #[test]
    fn test_no_trackers_legitimate_images() {
        let html = r#"<img src="https://example.com/logo.png" width="200" height="50" alt="Logo">"#;
        let trackers = detect_trackers(html);
        // Legitimate images should not be flagged unless they have tracking params
        let has_tracking_pixel = trackers.iter().any(|t| matches!(t.tracker_type, TrackerType::TrackingPixel));
        assert!(!has_tracking_pixel, "Legitimate images should not be flagged as tracking pixels");
    }

    #[test]
    fn test_block_1x1_pixel() {
        let html = "<html><body><img src=\"https://track.com/p.gif\" width=\"1\" height=\"1\"><p>Hello</p></body></html>";
        let (blocked, count) = block_trackers(html);
        assert!(count > 0, "Expected >0 blocked pixels, got {count}");
        // The img tag should now have data-blocked-src instead of src
        assert!(blocked.contains("data-blocked-src=\"https://track.com/p.gif\""), "Should contain data-blocked-src");
        // The original img tag with src as an HTML attribute should not exist
        // (check with leading space to avoid matching data-blocked-src substring)
        assert!(!blocked.contains(" src=\"https://track.com/p.gif\""), "Should not have original src attribute");
        assert!(blocked.contains("<p>Hello</p>"));
    }

    #[test]
    fn test_block_preserves_non_tracking_content() {
        let html = r#"<html><body><img src="https://cdn.example.com/photo.jpg" width="600" height="400"><p>Family photo</p></body></html>"#;
        let (blocked, _count) = block_trackers(html);
        assert!(blocked.contains("photo.jpg"));
        assert!(blocked.contains("Family photo"));
    }

    #[test]
    fn test_detect_trackers_empty_html() {
        let trackers = detect_trackers("");
        assert!(trackers.is_empty());
    }

    #[test]
    fn test_block_trackers_empty_html() {
        let (result, count) = block_trackers("");
        assert_eq!(result, "");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_tracker_type_as_str() {
        assert_eq!(TrackerType::TrackingPixel.as_str(), "tracking_pixel");
        assert_eq!(TrackerType::RemoteImage.as_str(), "remote_image");
        assert_eq!(TrackerType::ReadReceipt.as_str(), "read_receipt");
        assert_eq!(TrackerType::TrackingLink.as_str(), "tracking_link");
    }
}
