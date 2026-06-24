export type Tone = 'positive' | 'neutral' | 'negative' | 'urgent' | 'angry';
export type Urgency = 'low' | 'medium' | 'high' | 'critical';

export interface SentimentResult {
  tone: Tone;
  urgency: Urgency;
  summary: string;
  requires_response: boolean;
}

export const TONES: readonly Tone[] = ['positive', 'neutral', 'negative', 'urgent', 'angry'];
export const URGENCIES: readonly Urgency[] = ['low', 'medium', 'high', 'critical'];

export type DotShape = 'circle' | 'triangle' | 'square' | 'diamond';

export interface ToneConfig {
  color: string;
  bg: string;
  label: string;
  icon: string;
  shape: DotShape;
}

const ICONS = {
  alert: `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>`,
  angry: `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M8 14s1.5 2 4 2 4-2 4-2"/><line x1="9" y1="9" x2="9.01" y2="9"/><line x1="15" y1="9" x2="15.01" y2="9"/></svg>`,
  negative: `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="8" y1="15" x2="16" y2="15"/><line x1="9" y1="9" x2="9.01" y2="9"/><line x1="15" y1="9" x2="15.01" y2="9"/></svg>`,
  positive: `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M22 11.08V12a10 10 0 11-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>`,
  neutral: `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="8" y1="12" x2="16" y2="12"/></svg>`,
} as const;

export const TONE_CONFIG: Record<Tone, ToneConfig> = {
  urgent: { color: '#FF3B30', bg: 'rgba(255, 59, 48, 0.1)', label: 'Urgent', icon: ICONS.alert, shape: 'triangle' },
  angry: { color: '#FF3B30', bg: 'rgba(255, 59, 48, 0.08)', label: 'Angry', icon: ICONS.angry, shape: 'diamond' },
  negative: { color: '#FF9500', bg: 'rgba(255, 149, 0, 0.1)', label: 'Negative', icon: ICONS.negative, shape: 'square' },
  positive: { color: '#34C759', bg: 'rgba(52, 199, 89, 0.1)', label: 'Positive', icon: ICONS.positive, shape: 'circle' },
  neutral: { color: '#8E8E93', bg: 'rgba(142, 142, 147, 0.08)', label: 'Neutral', icon: ICONS.neutral, shape: 'circle' },
};

export const URGENCY_CONFIG: Record<Urgency, { color: string; label: string }> = {
  critical: { color: '#FF3B30', label: 'Critical' },
  high: { color: '#FF3B30', label: 'High' },
  medium: { color: '#FF9500', label: 'Medium' },
  low: { color: '#8E8E93', label: 'Low' },
};

export function toneConfig(tone: Tone | null | undefined): ToneConfig | null {
  return tone ? TONE_CONFIG[tone] ?? null : null;
}

export function urgencyConfig(urgency: Urgency | null | undefined): { color: string; label: string } | null {
  return urgency ? URGENCY_CONFIG[urgency] ?? null : null;
}
