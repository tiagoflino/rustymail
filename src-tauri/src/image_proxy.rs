//! Remote image / remote-content control for rendered email HTML.
//!
//! Modes (see `ImageLoadMode`):
//! - `always`: leave HTML untouched, remote content loads normally.
//! - `ask` / `never`: rewrite every remote-content load vector into a recoverable
//!   placeholder. The original URL is preserved in a `data-blocked-*` attribute so
//!   the renderer can offer a "Load Images" affordance (Ask) or leave it blocked
//!   forever (Never). The two modes share the same backend rewrite; the difference
//!   is purely whether the frontend exposes a restore control.
//!
//! All processing is local. No URL is ever fetched server-side, so there is no
//! SSRF surface. A real HTML parser (`lol_html`) is used instead of regular
//! expressions because HTML is not a regular language: nesting, entity encoding,
//! attribute-quoting variants, malformed tags and CSS `url()` cannot be matched
//! reliably with a regex, which is why the previous `<img src="http...">`-only
//! regex was trivially bypassed by protocol-relative URLs, `srcset`, CSS
//! backgrounds, `<source>`, `<video poster>` and `<input type=image>`.

use lol_html::{element, rewrite_str, RewriteStrSettings};
use std::cell::Cell;

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

/// Returns true if a URL would cause a network fetch when rendered.
/// Remote = `http://`, `https://`, or protocol-relative `//host/...`.
/// Local/inline references (`cid:`, `data:`, fragment, relative path) are kept.
fn is_remote_url(raw: &str) -> bool {
    let url = raw.trim();
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return true;
    }
    // Protocol-relative: //host/path — but not a bare fragment or path.
    if url.starts_with("//") {
        return true;
    }
    false
}

/// Returns true if any candidate inside a `srcset` value is remote.
fn srcset_has_remote(srcset: &str) -> bool {
    srcset
        .split(',')
        .filter_map(|candidate| candidate.split_whitespace().next())
        .any(is_remote_url)
}

/// Returns true if an inline `style` value loads remote content via `url(...)`.
fn style_has_remote_url(style: &str) -> bool {
    let lower = style.to_ascii_lowercase();
    let mut search_from = 0;
    while let Some(idx) = lower[search_from..].find("url(") {
        let start = search_from + idx + 4;
        if let Some(end_rel) = lower[start..].find(')') {
            let inner = lower[start..start + end_rel].trim().trim_matches(['"', '\'']);
            if is_remote_url(inner) {
                return true;
            }
            search_from = start + end_rel + 1;
        } else {
            break;
        }
    }
    false
}

/// Block remote content by rewriting every remote-load vector into a recoverable
/// placeholder. The original value is stashed in a `data-blocked-*` attribute so a
/// later "Load Images" action can restore it.
///
/// Returns `(modified_html, count_blocked)`.
pub fn block_remote_images(html: &str) -> (String, usize) {
    if html.is_empty() {
        return (String::new(), 0);
    }

    let count = Cell::new(0usize);

    let bump = || count.set(count.get() + 1);

    let settings = RewriteStrSettings::new()
        // <img>, <input type=image>: src + srcset + inline style background.
        .append_element_content_handler(element!("img, input[type=image]", |el| {
            block_url_attr(el, "src", "data-blocked-src", &bump);
            block_srcset(el, &bump);
            block_style(el, &bump);
            Ok(())
        }))
        // <source> inside <picture>/<video>/<audio>: src + srcset.
        .append_element_content_handler(element!("source", |el| {
            block_url_attr(el, "src", "data-blocked-src", &bump);
            block_srcset(el, &bump);
            Ok(())
        }))
        // <video poster=...> + inline style background.
        .append_element_content_handler(element!("video", |el| {
            block_url_attr(el, "poster", "data-blocked-poster", &bump);
            block_style(el, &bump);
            Ok(())
        }))
        // <image> (legacy / SVG alias for <img>): src + href + xlink:href.
        .append_element_content_handler(element!("image", |el| {
            block_url_attr(el, "src", "data-blocked-src", &bump);
            block_url_attr(el, "href", "data-blocked-href", &bump);
            block_url_attr(el, "xlink:href", "data-blocked-xlink:href", &bump);
            Ok(())
        }))
        // Any other element carrying an inline style with a remote url() background.
        .append_element_content_handler(element!("*[style]", |el| {
            block_style(el, &bump);
            Ok(())
        }));

    let rewritten = rewrite_str(html, settings);

    match rewritten {
        Ok(out) => (out, count.get()),
        // On a parser error, fail safe: return original unchanged with 0 count
        // rather than panicking on attacker-controlled input.
        Err(_) => (html.to_string(), 0),
    }
}

/// If `attr` holds a remote URL, move it into `blocked_attr` (recoverable) and
/// remove the live attribute so it does not load.
fn block_url_attr<F: Fn()>(
    el: &mut lol_html::html_content::Element,
    attr: &str,
    blocked_attr: &str,
    bump: &F,
) {
    if let Some(val) = el.get_attribute(attr) {
        if is_remote_url(&val) {
            el.remove_attribute(attr);
            el.set_attribute(blocked_attr, &val).ok();
            bump();
        }
    }
}

/// If a `srcset` contains any remote candidate, move the whole value into
/// `data-blocked-srcset` and remove the live attribute.
fn block_srcset<F: Fn()>(el: &mut lol_html::html_content::Element, bump: &F) {
    if let Some(srcset) = el.get_attribute("srcset") {
        if srcset_has_remote(&srcset) {
            el.remove_attribute("srcset");
            el.set_attribute("data-blocked-srcset", &srcset).ok();
            bump();
        }
    }
}

/// If an element's inline `style` loads remote content, stash the original in
/// `data-blocked-style` and neutralize the live `style` by removing it. The
/// renderer restores the original on "Load Images".
fn block_style<F: Fn()>(el: &mut lol_html::html_content::Element, bump: &F) {
    if let Some(style) = el.get_attribute("style") {
        if style_has_remote_url(&style) {
            el.set_attribute("data-blocked-style", &style).ok();
            el.remove_attribute("style");
            bump();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// True if `attr` appears as a live (non-data-blocked) attribute in the HTML.
    /// A live attribute is preceded by whitespace or `<`, so `data-blocked-src`
    /// (preceded by `-`) does not count as a live `src`.
    fn has_live_attr(html: &str, attr: &str) -> bool {
        let needle = format!("{attr}=");
        let mut from = 0;
        while let Some(idx) = html[from..].find(&needle) {
            let abs = from + idx;
            let prev = html[..abs].chars().next_back();
            match prev {
                Some(c) if c.is_whitespace() || c == '<' => return true,
                _ => {}
            }
            from = abs + needle.len();
        }
        false
    }

    #[test]
    fn test_block_emits_recoverable_placeholder() {
        let html = r#"<img src="https://cdn.example.com/logo.png" alt="Logo">"#;
        let (result, count) = block_remote_images(html);
        assert_eq!(count, 1);
        // Live src is gone (only the data-blocked- recovery attribute remains),
        // but recoverable, and the tag itself survives with its other attributes.
        assert!(!has_live_attr(&result, "src"));
        assert!(result.contains(r#"data-blocked-src="https://cdn.example.com/logo.png""#));
        assert!(result.contains(r#"alt="Logo""#));
        assert!(result.contains("<img"));
    }

    #[test]
    fn test_preserves_cid_and_relative() {
        let html = r#"<img src="cid:attachment123"><img src="/local.png"><img src="https://remote.com/img.jpg">"#;
        let (result, count) = block_remote_images(html);
        assert_eq!(count, 1);
        assert!(result.contains(r#"src="cid:attachment123""#));
        assert!(result.contains(r#"src="/local.png""#));
        assert!(result.contains(r#"data-blocked-src="https://remote.com/img.jpg""#));
    }

    #[test]
    fn test_no_remote() {
        let html = r#"<p>Plain text email</p>"#;
        let (result, count) = block_remote_images(html);
        assert_eq!(count, 0);
        assert!(result.contains("<p>Plain text email</p>"));
    }

    #[test]
    fn test_empty() {
        let (result, count) = block_remote_images("");
        assert_eq!(count, 0);
        assert_eq!(result, "");
    }

    #[test]
    fn test_protocol_relative_blocked() {
        let html = r#"<img src="//track.com/pixel.gif">"#;
        let (result, count) = block_remote_images(html);
        assert_eq!(count, 1);
        assert!(result.contains(r#"data-blocked-src="//track.com/pixel.gif""#));
        assert!(!has_live_attr(&result, "src"));
    }

    #[test]
    fn test_srcset_blocked() {
        let html = r#"<img srcset="https://cdn.com/a.jpg 1x, https://cdn.com/b.jpg 2x">"#;
        let (result, count) = block_remote_images(html);
        assert_eq!(count, 1);
        assert!(result.contains("data-blocked-srcset"));
        assert!(!has_live_attr(&result, "srcset"));
    }

    #[test]
    fn test_css_background_url_blocked() {
        let html = r#"<div style="background-image:url('https://track.com/bg.png');color:red">x</div>"#;
        let (result, count) = block_remote_images(html);
        assert_eq!(count, 1);
        assert!(result.contains("data-blocked-style"));
        assert!(!has_live_attr(&result, "style"));
    }

    #[test]
    fn test_css_local_url_kept() {
        let html = r#"<div style="background-image:url('cid:bg');color:red">x</div>"#;
        let (result, count) = block_remote_images(html);
        assert_eq!(count, 0);
        assert!(result.contains("background-image:url('cid:bg')"));
    }

    #[test]
    fn test_input_type_image_blocked() {
        let html = r#"<input type="image" src="https://track.com/p.gif">"#;
        let (result, count) = block_remote_images(html);
        assert_eq!(count, 1);
        assert!(result.contains("data-blocked-src"));
    }

    #[test]
    fn test_video_poster_and_source_blocked() {
        let html = r#"<video poster="https://track.com/poster.jpg"><source src="https://track.com/v.mp4"></video>"#;
        let (result, count) = block_remote_images(html);
        assert_eq!(count, 2);
        assert!(result.contains("data-blocked-poster"));
        assert!(result.contains("data-blocked-src"));
    }

    #[test]
    fn test_picture_source_srcset_blocked() {
        let html = r#"<picture><source srcset="//cdn.com/a.webp"><img src="https://cdn.com/a.jpg"></picture>"#;
        let (result, count) = block_remote_images(html);
        assert_eq!(count, 2);
        assert!(result.contains("data-blocked-srcset"));
        assert!(result.contains("data-blocked-src"));
    }

    #[test]
    fn test_uppercase_and_spacing_variants() {
        let html = r#"<IMG  SRC = "HTTPS://Track.com/P.GIF" >"#;
        let (result, count) = block_remote_images(html);
        assert_eq!(count, 1);
        assert!(result.to_ascii_lowercase().contains("data-blocked-src"));
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
