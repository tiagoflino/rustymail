import { describe, it, expect } from 'vitest';
import { formatTime, decodeEntities } from './formatters';

describe('formatters', () => {
    describe('formatTime', () => {
        it('formats today as time', () => {
            const today = new Date();
            today.setHours(10, 30);
            const ts = today.getTime();
            const result = formatTime(ts);
            expect(result).toMatch(/10:30|10:30 AM/); // Locale dependent, but should contain the time
        });

        it('formats this week as day name', () => {
            const threeDaysAgo = new Date();
            threeDaysAgo.setDate(threeDaysAgo.getDate() - 3);
            const ts = threeDaysAgo.getTime();
            const result = formatTime(ts);
            expect(result.length).toBeGreaterThan(0);
            expect(['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']).toContain(result);
        });

        it('formats older dates as month and day', () => {
            const longAgo = new Date(2023, 0, 15); // Jan 15, 2023
            const ts = longAgo.getTime();
            const result = formatTime(ts);
            expect(result).toMatch(/Jan 15/);
        });
    });

    describe('decodeEntities', () => {
        it('decodes common HTML entities', () => {
            expect(decodeEntities('Hello &amp; world')).toBe('Hello & world');
            expect(decodeEntities('2 &gt; 1')).toBe('2 > 1');
            expect(decodeEntities('1 &lt; 2')).toBe('1 < 2');
            expect(decodeEntities('He said &quot;Hi&quot;')).toBe('He said "Hi"');
            expect(decodeEntities('Space&nbsp;Space')).toBe('Space Space');
        });

        it('decodes numeric entities', () => {
            expect(decodeEntities('&#65;')).toBe('A');
            expect(decodeEntities('&#x41;')).toBe('A');
        });

        it('returns empty string for null/empty input', () => {
            expect(decodeEntities('')).toBe('');
            expect(decodeEntities(null)).toBe('');
        });
    });
});
