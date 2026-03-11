import { describe, it, expect } from 'vitest';
import { analyzeLinkSafety } from './linkSafety';

describe('analyzeLinkSafety', () => {
    it('marks normal matching links as safe', () => {
        const result = analyzeLinkSafety('https://github.com/repo', 'notifications@github.com');
        expect(result.risk).toBe('safe');
    });

    it('flags IP address URLs as danger', () => {
        const result = analyzeLinkSafety('http://192.168.1.1/login', 'admin@company.com');
        expect(result.risk).toBe('danger');
        expect(result.reasons.some(r => r.includes('IP address'))).toBe(true);
    });

    it('flags URL shorteners as caution', () => {
        const result = analyzeLinkSafety('https://bit.ly/abc123', 'user@example.com');
        expect(result.risk).toBe('caution');
        expect(result.reasons.some(r => r.includes('Shortened'))).toBe(true);
    });

    it('flags homoglyph domains as danger', () => {
        const result = analyzeLinkSafety('https://paypa1.com/login', 'security@paypal.com');
        expect(result.risk).toBe('danger');
        expect(result.reasons.some(r => r.includes('paypal'))).toBe(true);
    });

    it('flags excessive subdomains as caution', () => {
        const result = analyzeLinkSafety('https://login.secure.account.update.example.com/verify', 'user@example.com');
        expect(result.risk).toBe('caution');
        expect(result.reasons.some(r => r.includes('subdomains'))).toBe(true);
    });

    it('flags sender domain mismatch as caution', () => {
        const result = analyzeLinkSafety('https://tracking.mailchimp.com/click', 'newsletter@mycompany.com');
        expect(result.risk).toBe('caution');
        expect(result.reasons.some(r => r.includes('mailchimp.com'))).toBe(true);
    });

    it('returns danger for invalid URLs', () => {
        const result = analyzeLinkSafety('not-a-url', 'user@example.com');
        expect(result.risk).toBe('danger');
        expect(result.reasons).toContain('Invalid URL');
    });

    it('handles sender without email brackets', () => {
        const result = analyzeLinkSafety('https://github.com/repo', 'GitHub <noreply@github.com>');
        expect(result.risk).toBe('safe');
    });

    it('handles empty sender gracefully', () => {
        const result = analyzeLinkSafety('https://example.com', '');
        expect(result.risk).toBe('safe');
    });

    it('does not flag legitimate paypal.com as homoglyph', () => {
        const result = analyzeLinkSafety('https://www.paypal.com/invoice', 'service@paypal.com');
        expect(result.risk).toBe('safe');
    });

    it('combines multiple risks', () => {
        const result = analyzeLinkSafety('http://192.168.1.1/paypa1', 'user@bank.com');
        expect(result.risk).toBe('danger');
        expect(result.reasons.length).toBeGreaterThanOrEqual(1);
    });
});
