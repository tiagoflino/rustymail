import { render, screen } from '@testing-library/svelte';
import '@testing-library/jest-dom';
import { describe, it, expect } from 'vitest';
import SentimentBadge from './SentimentBadge.svelte';

describe('SentimentBadge', () => {
  it('renders urgent badge with red color', () => {
    render(SentimentBadge, { props: { sentiment: 'urgent', showLabel: true } });
    const badge = screen.getByText('Urgent');
    expect(badge).toBeInTheDocument();
    const el = badge.closest('.sentiment-badge');
    expect(el).toBeTruthy();
    expect(el.style.color).toBeTruthy();
  });

  it('renders positive badge', () => {
    render(SentimentBadge, { props: { sentiment: 'positive', showLabel: true } });
    expect(screen.getByText('Positive')).toBeInTheDocument();
  });

  it('renders angry badge', () => {
    render(SentimentBadge, { props: { sentiment: 'angry', showLabel: true } });
    expect(screen.getByText('Angry')).toBeInTheDocument();
  });

  it('renders inquisitive badge', () => {
    render(SentimentBadge, { props: { sentiment: 'inquisitive', showLabel: true } });
    expect(screen.getByText('Question')).toBeInTheDocument();
  });

  it('renders compact dot without label', () => {
    render(SentimentBadge, { props: { sentiment: 'urgent', showLabel: false } });
    const badge = document.querySelector('.sentiment-badge');
    expect(badge).toBeTruthy();
    expect(screen.queryByText('Urgent')).not.toBeInTheDocument();
  });

  it('renders as compact dot when compact prop is true', () => {
    render(SentimentBadge, { props: { sentiment: 'warning', compact: true } });
    const dot = document.querySelector('.sentiment-dot');
    expect(dot).toBeTruthy();
    expect(dot.getAttribute('aria-label')).toBe('Attention');
  });

  it('renders nothing for null sentiment and urgency', () => {
    render(SentimentBadge, { props: { sentiment: null, urgency: null } });
    const badge = document.querySelector('.sentiment-badge');
    const dot = document.querySelector('.sentiment-dot');
    expect(badge).toBeFalsy();
    expect(dot).toBeFalsy();
  });

  it('renders urgency dot when sentiment is set', () => {
    render(SentimentBadge, { props: { sentiment: 'warning', urgency: 'high' } });
    const urgencyDot = document.querySelector('.urgency-dot');
    expect(urgencyDot).toBeTruthy();
    expect(urgencyDot.style.background).toBeTruthy();
  });

  it('has proper role attributes for accessibility', () => {
    render(SentimentBadge, { props: { sentiment: 'neutral', showLabel: true } });
    const badge = document.querySelector('.sentiment-badge');
    expect(badge.getAttribute('role')).toBe('status');
  });
});
