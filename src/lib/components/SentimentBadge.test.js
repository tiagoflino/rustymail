import { render, screen } from '@testing-library/svelte';
import '@testing-library/jest-dom';
import { describe, it, expect } from 'vitest';
import SentimentBadge from './SentimentBadge.svelte';
import { TONES, URGENCIES, TONE_CONFIG, URGENCY_CONFIG } from '$lib/utils/sentiment';

/**
 * @param {string} selector
 * @returns {HTMLElement | null}
 */
function q(selector) {
  return /** @type {HTMLElement | null} */ (document.querySelector(selector));
}

describe('SentimentBadge', () => {
  it('renders urgent badge with red color', () => {
    render(SentimentBadge, { props: { sentiment: 'urgent', showLabel: true } });
    const badge = screen.getByText('Urgent');
    expect(badge).toBeInTheDocument();
    const el = /** @type {HTMLElement | null} */ (badge.closest('.sentiment-badge'));
    expect(el).toBeTruthy();
    expect(el?.style.color).toBeTruthy();
  });

  it('renders positive badge', () => {
    render(SentimentBadge, { props: { sentiment: 'positive', showLabel: true } });
    expect(screen.getByText('Positive')).toBeInTheDocument();
  });

  it('renders angry badge', () => {
    render(SentimentBadge, { props: { sentiment: 'angry', showLabel: true } });
    expect(screen.getByText('Angry')).toBeInTheDocument();
  });

  it('renders negative badge (previously dropped backend value)', () => {
    render(SentimentBadge, { props: { sentiment: 'negative', showLabel: true } });
    expect(screen.getByText('Negative')).toBeInTheDocument();
  });

  it('renders compact dot without label', () => {
    render(SentimentBadge, { props: { sentiment: 'urgent', showLabel: false } });
    expect(q('.sentiment-badge')).toBeTruthy();
    expect(screen.queryByText('Urgent')).not.toBeInTheDocument();
  });

  it('renders as compact dot when compact prop is true', () => {
    render(SentimentBadge, { props: { sentiment: 'negative', compact: true } });
    const dot = q('.sentiment-dot');
    expect(dot).toBeTruthy();
    expect(dot?.getAttribute('aria-label')).toBe('Negative');
  });

  it('compact dot conveys meaning by shape, not color alone (WCAG 1.4.1)', () => {
    render(SentimentBadge, { props: { sentiment: 'urgent', compact: true } });
    const dot = q('.sentiment-dot');
    expect(dot?.className).toContain('shape-');
  });

  it('renders nothing for null sentiment and urgency', () => {
    render(SentimentBadge, { props: { sentiment: null, urgency: null } });
    expect(q('.sentiment-badge')).toBeFalsy();
    expect(q('.sentiment-dot')).toBeFalsy();
  });

  it('renders urgency dot when sentiment is set', () => {
    render(SentimentBadge, { props: { sentiment: 'negative', urgency: 'high' } });
    const urgencyDot = q('.urgency-dot');
    expect(urgencyDot).toBeTruthy();
    expect(urgencyDot?.style.background).toBeTruthy();
  });

  it('renders critical urgency (previously dropped backend value)', () => {
    render(SentimentBadge, { props: { sentiment: null, urgency: 'critical', compact: true } });
    const dot = q('.sentiment-dot');
    expect(dot).toBeTruthy();
    expect(dot?.getAttribute('aria-label')).toBe('Critical');
  });

  it('has proper role attributes for accessibility', () => {
    render(SentimentBadge, { props: { sentiment: 'neutral', showLabel: true } });
    const badge = q('.sentiment-badge');
    expect(badge?.getAttribute('role')).toBe('status');
  });
});

describe('SentimentBadge contract: every backend enum value maps to a config and renders', () => {
  for (const tone of TONES) {
    it(`tone "${tone}" has a badge config and renders a label`, () => {
      expect(TONE_CONFIG[tone]).toBeTruthy();
      render(SentimentBadge, { props: { sentiment: tone, showLabel: true } });
      expect(screen.getByText(TONE_CONFIG[tone].label)).toBeInTheDocument();
    });
  }

  for (const urgency of URGENCIES) {
    it(`urgency "${urgency}" has a config and renders a dot`, () => {
      expect(URGENCY_CONFIG[urgency]).toBeTruthy();
      render(SentimentBadge, { props: { sentiment: null, urgency, compact: true } });
      const dot = q('.sentiment-dot');
      expect(dot).toBeTruthy();
      expect(dot?.getAttribute('aria-label')).toBe(URGENCY_CONFIG[urgency].label);
    });
  }
});
