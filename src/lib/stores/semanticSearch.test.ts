import { describe, it, expect } from 'vitest';
import {
    semanticResultToThread,
    type SemanticSearchResult,
    type LocalThread,
} from './threads';

const sample: SemanticSearchResult = {
    message_id: 'm1',
    thread_id: 't1',
    subject: 'Quarterly budget review',
    sender: 'alice@example.com',
    snippet: 'Here is the budget',
    score: 0.873,
    internal_date: 1_700_000_000,
};

describe('semanticResultToThread', () => {
    it('produces a fully-typed LocalThread keyed by thread_id', () => {
        const t: LocalThread = semanticResultToThread(sample, 'acc1');
        expect(t.id).toBe('t1');
        expect(t.subject).toBe('Quarterly budget review');
        expect(t.sender).toBe('alice@example.com');
        expect(t.account_id).toBe('acc1');
    });

    it('carries the real internal_date so date sorting is not broken', () => {
        const t = semanticResultToThread(sample, 'acc1');
        expect(t.internal_date).toBe(1_700_000_000);
        expect(t.internal_date).not.toBe(0);
    });

    it('appends a rounded match-score badge to the snippet', () => {
        const t = semanticResultToThread(sample, 'acc1');
        expect(t.snippet).toContain('Here is the budget');
        expect(t.snippet).toContain('[match: 87%]');
    });

    it('sorts newest-first correctly when mixed into a thread list', () => {
        const older = semanticResultToThread(
            { ...sample, thread_id: 'old', internal_date: 1_000 },
            'acc1',
        );
        const newer = semanticResultToThread(
            { ...sample, thread_id: 'new', internal_date: 2_000 },
            'acc1',
        );
        const sorted = [older, newer].sort((a, b) => b.internal_date - a.internal_date);
        expect(sorted[0].id).toBe('new');
        expect(sorted[1].id).toBe('old');
    });

    it('does not fabricate fields outside the LocalThread interface', () => {
        const t = semanticResultToThread(sample, 'acc1') as unknown as Record<string, unknown>;
        // Fields the old buggy mapping invented must be absent.
        expect('date' in t).toBe(false);
        expect('message_count' in t).toBe(false);
        expect('labels' in t).toBe(false);
    });
});
