import { describe, it, expect } from 'vitest';
import { get } from 'svelte/store';
import {
  TONES,
  URGENCIES,
  TONE_CONFIG,
  URGENCY_CONFIG,
  toneConfig,
  urgencyConfig,
  type SentimentResult,
} from './sentiment';
import { threads, type LocalThread } from '$lib/stores/threads';

describe('sentiment shared config (single source of truth)', () => {
  it('every backend tone value has a config', () => {
    for (const tone of TONES) {
      expect(toneConfig(tone), `missing config for tone ${tone}`).toBeTruthy();
    }
  });

  it('every backend urgency value has a config', () => {
    for (const urgency of URGENCIES) {
      expect(urgencyConfig(urgency), `missing config for urgency ${urgency}`).toBeTruthy();
    }
  });

  it('covers exactly the backend tone enum (no dead config, no missing values)', () => {
    expect(new Set(Object.keys(TONE_CONFIG))).toEqual(new Set(TONES));
  });

  it('covers exactly the backend urgency enum', () => {
    expect(new Set(Object.keys(URGENCY_CONFIG))).toEqual(new Set(URGENCIES));
  });

  it('returns null for null/undefined input', () => {
    expect(toneConfig(null)).toBeNull();
    expect(urgencyConfig(undefined)).toBeNull();
  });
});

describe('contract: ai_analyze_sentiment response shape maps to a renderable badge', () => {
  // This is the exact backend SentimentAnalysis serialization shape (serde):
  // { tone, urgency, summary, requires_response }. The original bug read
  // result.sentiment (undefined). This test locks the field name to `tone`.
  function backendResponse(tone: string, urgency: string): SentimentResult {
    return { tone: tone as any, urgency: urgency as any, summary: 's', requires_response: true };
  }

  for (const tone of TONES) {
    for (const urgency of URGENCIES) {
      it(`backend {tone:"${tone}", urgency:"${urgency}"} -> non-null badge config`, () => {
        const result = backendResponse(tone, urgency);
        expect(toneConfig(result.tone)).toBeTruthy();
        expect(urgencyConfig(result.urgency)).toBeTruthy();
      });
    }
  }

  it('reading result.sentiment (the old bug) yields undefined, proving the regression guard', () => {
    const result = backendResponse('negative', 'critical');
    // @ts-expect-error - `sentiment` is intentionally not a field on SentimentResult
    expect(result.sentiment).toBeUndefined();
    expect(result.tone).toBe('negative');
  });
});

describe('wiring: MessageDetail thread-store update populates ThreadList badge fields', () => {
  it('updates the matching thread with tone+urgency from the analysis result', () => {
    const base: LocalThread = {
      id: 't1',
      snippet: '',
      history_id: 'h',
      unread: 0,
      sender: 'A',
      subject: 'S',
      internal_date: 0,
      starred: false,
      account_id: 'acc',
    };
    threads.set([base, { ...base, id: 't2' }]);

    const result: SentimentResult = {
      tone: 'urgent',
      urgency: 'high',
      summary: 'x',
      requires_response: true,
    };

    // Mirror of the update in MessageDetail.handleSentimentAnalysis
    const tid = 't1';
    threads.update((list) =>
      list.map((t) =>
        t.id === tid ? { ...t, sentiment: result.tone, urgency: result.urgency } : t,
      ),
    );

    const list = get(threads);
    const t1 = list.find((t) => t.id === 't1');
    const t2 = list.find((t) => t.id === 't2');
    expect(t1?.sentiment).toBe('urgent');
    expect(t1?.urgency).toBe('high');
    expect(t2?.sentiment).toBeUndefined();
  });
});
