export type LinkRisk = "safe" | "caution" | "danger";

export interface LinkAnalysis {
    risk: LinkRisk;
    reasons: string[];
}

const URL_SHORTENERS = new Set([
    "bit.ly", "t.co", "goo.gl", "tinyurl.com", "ow.ly", "is.gd",
    "buff.ly", "adf.ly", "bl.ink", "lnkd.in", "soo.gd", "s2r.co",
    "clicky.me", "budurl.com", "bc.vc", "j.mp", "rb.gy", "shorturl.at",
    "cutt.ly", "surl.li",
]);

// Each entry: [pattern that matches lookalikes, the canonical domain to exclude]
const HOMOGLYPH_BRANDS: [RegExp, string][] = [
    [/paypa[l1]/i, "paypal.com"],
    [/g[o0][o0]g[l1]e/i, "google.com"],
    [/m[i1]cr[o0]s[o0]ft/i, "microsoft.com"],
    [/amaz[o0]n/i, "amazon.com"],
    [/app[l1]e/i, "apple.com"],
    [/faceb[o0][o0]k/i, "facebook.com"],
    [/netf[l1][i1]x/i, "netflix.com"],
];

const IP_URL = /^https?:\/\/\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}/;
const EXCESSIVE_SUBDOMAINS = /^https?:\/\/([^/]+\.){4,}[^/]+/;

function extractDomain(url: string): string {
    try {
        return new URL(url).hostname.toLowerCase();
    } catch {
        return "";
    }
}

function extractSenderDomain(sender: string): string {
    const match = sender.match(/@([a-zA-Z0-9.-]+)/);
    return match ? match[1].toLowerCase() : "";
}

export function analyzeLinkSafety(url: string, senderEmail: string): LinkAnalysis {
    const reasons: string[] = [];
    let risk: LinkRisk = "safe";

    const domain = extractDomain(url);
    if (!domain) {
        return { risk: "danger", reasons: ["Invalid URL"] };
    }

    // IP address instead of domain
    if (IP_URL.test(url)) {
        reasons.push("Uses IP address instead of domain name");
        risk = "danger";
    }

    // URL shortener
    if (URL_SHORTENERS.has(domain)) {
        reasons.push("Shortened URL — real destination hidden");
        risk = risk === "danger" ? "danger" : "caution";
    }

    // Homoglyph/lookalike domains
    for (const [pattern, canonical] of HOMOGLYPH_BRANDS) {
        if (pattern.test(domain) && !domain.endsWith(canonical)) {
            const brand = canonical.replace(".com", "");
            reasons.push(`Looks like ${brand} but isn't`);
            risk = "danger";
        }
    }

    // Excessive subdomains (e.g. login.secure.account.paypal.phishing.com)
    if (EXCESSIVE_SUBDOMAINS.test(url)) {
        reasons.push("Unusually many subdomains");
        risk = risk === "danger" ? "danger" : "caution";
    }

    // Sender domain mismatch
    const senderDomain = extractSenderDomain(senderEmail);
    if (senderDomain && domain) {
        // Extract root domain (last two parts) for comparison
        const linkRoot = domain.split(".").slice(-2).join(".");
        const senderRoot = senderDomain.split(".").slice(-2).join(".");

        if (linkRoot !== senderRoot) {
            // Not inherently dangerous — emails often link to external services
            // Only flag if nothing else is wrong, as a soft caution
            if (risk === "safe") {
                reasons.push(`Links to ${linkRoot} (sender is ${senderRoot})`);
                risk = "caution";
            }
        }
    }

    if (reasons.length === 0) {
        reasons.push("No issues detected");
    }

    return { risk, reasons };
}
