use lol_html::{element, rewrite_str, RewriteStrSettings};
use regex_lite::Regex;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DetectedTracker {
    pub tracker_type: TrackerType,
    pub details: String,
    pub url_snippet: String,
    pub blocked: bool,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum TrackerType {
    TrackingPixel,
    RemoteImage,
    ReadReceipt,
}

impl TrackerType {
    pub fn as_str(&self) -> &str {
        match self {
            TrackerType::TrackingPixel => "tracking_pixel",
            TrackerType::RemoteImage => "remote_image",
            TrackerType::ReadReceipt => "read_receipt",
        }
    }
}

const TRACKER_PARAM_PATTERNS: &[&str] = &[
    "utm_", "utm-", "tracking", "_ga=", "mc_cid", "mc_eid", "ml_subscriber", "oly_enc_id",
];

const BEACON_PATH_SEGMENTS: &[&str] = &["pixel", "beacon", "track", "open", "read"];

fn url_path_and_query(url: &str) -> &str {
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    match after_scheme.find('/') {
        Some(idx) => &after_scheme[idx..],
        None => "",
    }
}

fn segment_matches_keyword(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment);
    BEACON_PATH_SEGMENTS.iter().any(|kw| {
        segment.eq_ignore_ascii_case(kw) || stem.eq_ignore_ascii_case(kw)
    })
}

fn is_beacon_url(url: &str) -> bool {
    let path = url_path_and_query(url);
    if path.is_empty() {
        return false;
    }
    let path_only = path.split(['?', '#']).next().unwrap_or(path);
    path_only
        .split('/')
        .filter(|s| !s.is_empty())
        .any(segment_matches_keyword)
}

fn has_tracking_params(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    TRACKER_PARAM_PATTERNS.iter().any(|p| lower.contains(p))
}

fn attr_value<'a>(tag: &'a str, attr: &str) -> Option<&'a str> {
    let re = Regex::new(&format!(
        r#"(?i)\b{}\s*=\s*["']?([^"'\s>]*)["']?"#,
        regex_lite::escape(attr)
    ))
    .ok()?;
    re.captures(tag)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
}

fn is_zero_or_one(value: &str) -> bool {
    let v = value.trim().trim_matches(['"', '\'']);
    v == "0" || v == "1"
}

fn is_tracking_pixel_tag(tag: &str) -> bool {
    let w = attr_value(tag, "width");
    let h = attr_value(tag, "height");
    matches!((w, h), (Some(w), Some(h)) if is_zero_or_one(w) && is_zero_or_one(h))
}

fn img_src(tag: &str) -> Option<String> {
    attr_value(tag, "src").map(|s| s.to_string())
}

pub fn detect_trackers(html: &str) -> Vec<DetectedTracker> {
    let mut trackers = Vec::new();

    detect_tracking_pixels(html, &mut trackers);
    detect_tracking_params(html, &mut trackers);
    detect_read_receipts(html, &mut trackers);

    trackers
}

fn detect_tracking_pixels(html: &str, trackers: &mut Vec<DetectedTracker>) {
    let re_img = match Regex::new(r#"(?i)<img\b[^>]*>"#) {
        Ok(re) => re,
        Err(_) => return,
    };

    for m in re_img.find_iter(html) {
        let tag = m.as_str();
        let src = img_src(tag).unwrap_or_default();

        if is_tracking_pixel_tag(tag) {
            trackers.push(DetectedTracker {
                tracker_type: TrackerType::TrackingPixel,
                details: "1x1 tracking pixel".to_string(),
                url_snippet: truncate_url(&src, 100),
                blocked: false,
            });
            continue;
        }

        if !src.is_empty() && is_beacon_url(&src) && !trackers.iter().any(|t| t.url_snippet == src)
        {
            trackers.push(DetectedTracker {
                tracker_type: TrackerType::TrackingPixel,
                details: "Beacon image (pixel/track/open)".to_string(),
                url_snippet: truncate_url(&src, 100),
                blocked: false,
            });
        }
    }
}

fn detect_tracking_params(html: &str, trackers: &mut Vec<DetectedTracker>) {
    let re_img = match Regex::new(r#"(?i)<img\b[^>]*>"#) {
        Ok(re) => re,
        Err(_) => return,
    };

    for m in re_img.find_iter(html) {
        let tag = m.as_str();
        if is_tracking_pixel_tag(tag) {
            continue;
        }
        let src = match img_src(tag) {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };
        if has_tracking_params(&src) {
            trackers.push(DetectedTracker {
                tracker_type: TrackerType::RemoteImage,
                details: "Image with tracking parameters".to_string(),
                url_snippet: truncate_url(&src, 100),
                blocked: false,
            });
        }
    }
}

fn detect_read_receipts(html: &str, trackers: &mut Vec<DetectedTracker>) {
    let re_receipt = Regex::new(
        r#"(?i)<meta[^>]*\b(?:name|http-equiv)=["']?(?:disposition-notification-to|x-dln|return-receipt-to)["']?[^>]*>"#,
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

    let re_css_bg = Regex::new(
        r#"(?i)(?:background|background-image)\s*:\s*url\(["']?([^)"']*)["']?\)"#,
    );

    if let Ok(re) = &re_css_bg {
        for cap in re.captures_iter(html) {
            let url = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            if is_beacon_url(url) || has_tracking_params(url) {
                trackers.push(DetectedTracker {
                    tracker_type: TrackerType::TrackingPixel,
                    details: "CSS background tracking image".to_string(),
                    url_snippet: truncate_url(url, 100),
                    blocked: false,
                });
            }
        }
    }
}

/// Rewrite HTML to neutralize detected trackers using a real HTML parser (lol_html).
/// Returns the rewritten HTML and the number of trackers actually neutralized.
/// Tracking `<img>` tags (1x1 pixels, beacon URLs, tracking params) have their `src`
/// rewritten to `data-blocked-src`. Read-receipt meta tags are dropped.
pub fn block_trackers(html: &str) -> (String, usize) {
    let blocked_count = std::cell::Cell::new(0usize);

    let element_handlers = vec![
        element!("img[src]", |el| {
            let src = el.get_attribute("src").unwrap_or_default();
            let is_pixel = {
                let w = el.get_attribute("width");
                let h = el.get_attribute("height");
                matches!((w.as_deref(), h.as_deref()), (Some(w), Some(h)) if is_zero_or_one(w) && is_zero_or_one(h))
            };
            if is_pixel || is_beacon_url(&src) || has_tracking_params(&src) {
                el.remove_attribute("src");
                el.set_attribute("data-blocked-src", &src)
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
                blocked_count.set(blocked_count.get() + 1);
            }
            Ok(())
        }),
        element!(
            r#"meta[name="disposition-notification-to" i], meta[name="x-dln" i], meta[name="return-receipt-to" i], meta[http-equiv="disposition-notification-to" i], meta[http-equiv="return-receipt-to" i]"#,
            |el| {
                el.remove();
                blocked_count.set(blocked_count.get() + 1);
                Ok(())
            }
        ),
    ];

    let result = rewrite_str(
        html,
        RewriteStrSettings {
            element_content_handlers: element_handlers,
            ..RewriteStrSettings::new()
        },
    );
    match result {
        Ok(rewritten) => (rewritten, blocked_count.get()),
        Err(_) => (html.to_string(), 0),
    }
}

/// Detect trackers and neutralize blockable ones in a single pass.
/// Returns the cleaned HTML plus per-tracker results where `blocked` reflects
/// whether that specific tracker was actually neutralized.
pub fn scan_and_block(html: &str) -> (Vec<DetectedTracker>, String, usize) {
    let mut trackers = detect_trackers(html);
    let (cleaned_html, blocked_count) = block_trackers(html);

    for t in &mut trackers {
        t.blocked = match t.tracker_type {
            TrackerType::TrackingPixel | TrackerType::RemoteImage => {
                t.details != "CSS background tracking image"
            }
            TrackerType::ReadReceipt => true,
        };
    }

    (trackers, cleaned_html, blocked_count)
}

fn truncate_url(url: &str, max_len: usize) -> String {
    if url.len() <= max_len {
        return url.to_string();
    }
    let budget = max_len.saturating_sub(3);
    let mut end = budget;
    while end > 0 && !url.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &url[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(trackers: &[DetectedTracker]) -> Vec<&str> {
        trackers.iter().map(|t| t.tracker_type.as_str()).collect()
    }

    #[test]
    fn test_detect_1x1_tracking_pixel() {
        let html = r#"<html><body><img src="https://track.example.com/pixel.gif" width="1" height="1"></body></html>"#;
        let trackers = detect_trackers(html);
        assert!(trackers.iter().any(|t| t.tracker_type == TrackerType::TrackingPixel));
        assert!(trackers.iter().any(|t| t.url_snippet.contains("track.example.com")));
    }

    #[test]
    fn test_detect_tracking_pixel_no_quotes() {
        let html = r#"<img src="https://a.com/p.gif" width=1 height=1>"#;
        let trackers = detect_trackers(html);
        assert!(trackers.iter().any(|t| t.tracker_type == TrackerType::TrackingPixel));
    }

    #[test]
    fn test_width_1_exact_is_pixel() {
        let html = r#"<img src="https://a.com/p.gif" width="1" height="1">"#;
        assert!(is_tracking_pixel_tag(html));
    }

    #[test]
    fn test_width_10_is_not_pixel() {
        let html = r#"<img src="https://a.com/logo.png" width="10" height="10">"#;
        assert!(!is_tracking_pixel_tag(html));
        let trackers = detect_trackers(html);
        assert!(!trackers.iter().any(|t| t.tracker_type == TrackerType::TrackingPixel),
            "width=10 must not be flagged as a tracking pixel");
    }

    #[test]
    fn test_width_100_is_not_pixel() {
        let html = r#"<img src="https://a.com/logo.png" width="100" height="100">"#;
        assert!(!is_tracking_pixel_tag(html));
        let trackers = detect_trackers(html);
        assert!(!trackers.iter().any(|t| t.tracker_type == TrackerType::TrackingPixel),
            "width=100 must not be flagged as a tracking pixel");
    }

    #[test]
    fn test_width_128_is_not_pixel() {
        let html = r#"<img src="https://a.com/logo.png" width="128" height="128">"#;
        assert!(!is_tracking_pixel_tag(html));
    }

    #[test]
    fn test_width_1_with_trailing_junk_is_pixel_only_when_exact() {
        let exact = r#"<img src="x" width="1" height="1">"#;
        assert!(is_tracking_pixel_tag(exact));
        let junk = r#"<img src="x" width="1px" height="1px">"#;
        assert!(!is_tracking_pixel_tag(junk), "width=1px is not exactly 1");
    }

    #[test]
    fn test_detect_beacon_image() {
        let html = r#"<img src="https://mail.example.com/track/open/abc123" width="20" height="20">"#;
        let trackers = detect_trackers(html);
        assert!(trackers.iter().any(|t| t.url_snippet.contains("track/open")));
    }

    #[test]
    fn test_detect_utm_tracking_params() {
        let html = r#"<img src="https://example.com/logo.png?utm_source=newsletter&utm_campaign=test" width="200" height="50">"#;
        let trackers = detect_trackers(html);
        assert!(trackers.iter().any(|t| t.tracker_type == TrackerType::RemoteImage));
    }

    #[test]
    fn test_detect_mailchimp_tracking() {
        let html = r#"<img src="https://list-manage.com/track/open.php?u=abc123&id=xyz789">"#;
        let trackers = detect_trackers(html);
        assert!(!trackers.is_empty());
    }

    #[test]
    fn test_detect_read_receipt_meta() {
        let html = r#"<meta http-equiv="Disposition-Notification-To" content="sender@example.com">"#;
        let trackers = detect_trackers(html);
        assert!(trackers.iter().any(|t| t.tracker_type == TrackerType::ReadReceipt));
    }

    #[test]
    fn test_detect_css_background_tracking() {
        let html = r#"<div style="background: url('https://track.com/track/pixel.png')"></div>"#;
        let trackers = detect_trackers(html);
        assert!(trackers.iter().any(|t| t.url_snippet.contains("track.com")));
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
        assert!(trackers.is_empty(), "Legitimate images should not be flagged");
    }

    #[test]
    fn test_substring_false_positives_avoided() {
        let cases = [
            r#"<img src="https://cdn.example.com/open-graph-logo.png" width="200" height="200">"#,
            r#"<img src="https://racetrack.com/photo.jpg" width="300" height="200">"#,
            r#"<img src="https://example.com/readme-banner.png" width="600" height="100">"#,
        ];
        for html in cases {
            let trackers = detect_trackers(html);
            assert!(trackers.is_empty(), "False positive for: {html}");
        }
    }

    #[test]
    fn test_block_1x1_pixel() {
        let html = "<html><body><img src=\"https://track.com/p.gif\" width=\"1\" height=\"1\"><p>Hello</p></body></html>";
        let (blocked, count) = block_trackers(html);
        assert!(count > 0, "Expected >0 blocked pixels, got {count}");
        assert!(blocked.contains("data-blocked-src=\"https://track.com/p.gif\""));
        assert!(!blocked.contains(" src=\"https://track.com/p.gif\""));
        assert!(blocked.contains("<p>Hello</p>"));
    }

    #[test]
    fn test_block_reversed_attribute_order() {
        let html = r#"<img src="https://track.com/p.gif" height="1" width="1">"#;
        let (blocked, count) = block_trackers(html);
        assert_eq!(count, 1, "reversed height/width order must still be blocked");
        assert!(blocked.contains("data-blocked-src"));
    }

    #[test]
    fn test_block_src_with_spaces() {
        let html = r#"<img src = "https://track.com/track/pixel.gif">"#;
        let (blocked, count) = block_trackers(html);
        assert_eq!(count, 1, "src with surrounding spaces must be blocked");
        assert!(blocked.contains("data-blocked-src"));
        assert!(!blocked.contains(" src=\"https://track.com"));
    }

    #[test]
    fn test_block_preserves_non_tracking_content() {
        let html = r#"<html><body><img src="https://cdn.example.com/photo.jpg" width="600" height="400"><p>Family photo</p></body></html>"#;
        let (blocked, count) = block_trackers(html);
        assert!(blocked.contains("photo.jpg"));
        assert!(blocked.contains("Family photo"));
        assert!(!blocked.contains("data-blocked-src"));
        assert_eq!(count, 0);
    }

    #[test]
    fn test_block_read_receipt_meta_removed() {
        let html = r#"<head><meta http-equiv="Disposition-Notification-To" content="x@y.com"></head>"#;
        let (blocked, count) = block_trackers(html);
        assert_eq!(count, 1);
        assert!(!blocked.to_lowercase().contains("disposition-notification-to"));
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
    fn test_truncate_url_multibyte_no_panic() {
        let url = "é".repeat(60);
        let out = truncate_url(&url, 100);
        assert!(out.ends_with("..."));
        assert!(out.is_char_boundary(out.len() - 3));
    }

    #[test]
    fn test_truncate_url_ascii() {
        let url = "a".repeat(200);
        let out = truncate_url(&url, 100);
        assert_eq!(out.len(), 100);
        assert!(out.ends_with("..."));
    }

    #[test]
    fn test_scan_and_block_flags_match_neutralized() {
        let html = r#"<img src="https://track.com/pixel.gif" width="1" height="1"><img src="https://x.com/i.png?utm_source=n" width="10" height="10">"#;
        let (trackers, _cleaned, blocked) = scan_and_block(html);
        let neutralized = trackers.iter().filter(|t| t.blocked).count();
        assert_eq!(neutralized, blocked, "per-tracker blocked flags must match neutralized count");
    }

    #[test]
    fn test_scan_and_block_css_bg_not_marked_blocked() {
        let html = r#"<div style="background: url('https://track.com/track/pixel.png')"></div>"#;
        let (trackers, _cleaned, _blocked) = scan_and_block(html);
        assert!(trackers.iter().any(|t| t.details == "CSS background tracking image"));
        assert!(trackers.iter().all(|t| !t.blocked),
            "CSS background trackers are detected but not neutralized, so blocked must be false");
    }

    #[test]
    fn test_tracker_type_as_str() {
        assert_eq!(TrackerType::TrackingPixel.as_str(), "tracking_pixel");
        assert_eq!(TrackerType::RemoteImage.as_str(), "remote_image");
        assert_eq!(TrackerType::ReadReceipt.as_str(), "read_receipt");
    }

    #[test]
    fn test_is_beacon_url_path_segments() {
        assert!(is_beacon_url("https://m.example.com/track/open/abc"));
        assert!(is_beacon_url("https://m.example.com/o/read.gif"));
        assert!(!is_beacon_url("https://racetrack.com/photo.jpg"));
        assert!(!is_beacon_url("https://cdn.example.com/open-graph-logo.png"));
        assert!(!is_beacon_url("https://example.com/readme-banner.png"));
    }

    #[test]
    fn test_types_helper() {
        let trackers = detect_trackers(r#"<img src="x" width="1" height="1">"#);
        assert_eq!(types(&trackers), vec!["tracking_pixel"]);
    }
}
