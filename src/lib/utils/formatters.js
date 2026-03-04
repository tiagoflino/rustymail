export function formatTime(ts) {
    if (!ts) return "";
    const d = new Date(ts);
    const now = new Date();
    if (d.toDateString() === now.toDateString()) return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    if (now.getTime() - d.getTime() < 7 * 86400000) return d.toLocaleDateString([], { weekday: 'short' });
    return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
}

export function decodeEntities(str) {
    if (!str) return '';
    return str.replace(/&#(\d+);/g, (_, dec) => String.fromCharCode(dec))
        .replace(/&#x([0-9a-f]+);/gi, (_, hex) => String.fromCharCode(parseInt(hex, 16)))
        .replace(/&amp;/g, '&')
        .replace(/&lt;/g, '<')
        .replace(/&gt;/g, '>')
        .replace(/&quot;/g, '"')
        .replace(/&nbsp;/g, ' ');
}

/**
 * Prepares HTML content for quoting in a reply or forward.
 * Extracts body content and potentially relevant styles, 
 * wrapping them in a scoped container.
 */
export function prepareQuotedHtml(html) {
    if (!html) return "";

    try {
        const parser = new DOMParser();
        const doc = parser.parseFromString(html, 'text/html');

        // Extract body content only — skip style tags to prevent
        // leaked selectors causing content to visually duplicate
        // in the WYSIWYG editor or draft body.
        const bodyContent = doc.body ? doc.body.innerHTML : html;

        return `<div class="gmail_quote" style="border-left:1px solid #ccc; margin-left:1ex; padding-left:1ex">
            ${bodyContent}
        </div>`;
    } catch (e) {
        console.error("Failed to prepare quoted HTML:", e);
        return html;
    }
}
