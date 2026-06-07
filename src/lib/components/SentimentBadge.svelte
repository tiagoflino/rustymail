<script lang="ts">
  // Apple HIG color system for sentiment/urgency levels
  interface Props {
    sentiment?: 'urgent' | 'angry' | 'warning' | 'positive' | 'neutral' | 'inquisitive' | null;
    urgency?: 'high' | 'medium' | 'low' | null;
    showLabel?: boolean;
    compact?: boolean;
  }

  let {
    sentiment = null,
    urgency = null,
    showLabel = false,
    compact = false,
  }: Props = $props();

  const sentimentConfig: Record<string, { color: string; bg: string; label: string; icon: string }> = {
    urgent: {
      color: '#FF3B30',  // HIG Red
      bg: 'rgba(255, 59, 48, 0.1)',
      label: 'Urgent',
      icon: `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>`,
    },
    angry: {
      color: '#FF3B30',
      bg: 'rgba(255, 59, 48, 0.08)',
      label: 'Angry',
      icon: `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M8 14s1.5 2 4 2 4-2 4-2"/><line x1="9" y1="9" x2="9.01" y2="9"/><line x1="15" y1="9" x2="15.01" y2="9"/></svg>`,
    },
    warning: {
      color: '#FF9500',  // HIG Orange
      bg: 'rgba(255, 149, 0, 0.1)',
      label: 'Attention',
      icon: `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>`,
    },
    positive: {
      color: '#34C759',  // HIG Green
      bg: 'rgba(52, 199, 89, 0.1)',
      label: 'Positive',
      icon: `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M22 11.08V12a10 10 0 11-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>`,
    },
    inquisitive: {
      color: '#5AC8FA',  // HIG Teal
      bg: 'rgba(90, 200, 250, 0.1)',
      label: 'Question',
      icon: `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 015.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>`,
    },
    neutral: {
      color: '#8E8E93',  // HIG Gray
      bg: 'rgba(142, 142, 147, 0.08)',
      label: 'Neutral',
      icon: `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="8" y1="12" x2="16" y2="12"/></svg>`,
    },
  };

  const urgencyConfig: Record<string, string> = {
    high: '#FF3B30',
    medium: '#FF9500',
    low: '#8E8E93',
  };

  let config = $derived(sentiment ? sentimentConfig[sentiment] : null);
  let urgencyColor = $derived(urgency ? urgencyConfig[urgency] : null);
</script>

{#if config || urgencyColor}
  {#if compact}
    <span
      class="sentiment-dot"
      style="background: {config?.color || urgencyColor};"
      title={config?.label || urgency || ''}
      role="img"
      aria-label={config?.label || urgency || ''}
    ></span>
  {:else}
    <span
      class="sentiment-badge"
      class:sentiment-compact={!showLabel}
      style="color: {config?.color}; background: {config?.bg}; border-color: {config?.color}33;"
      role="status"
      aria-label={config?.label || ''}
    >
      {#if config}
        <span class="sentiment-icon">{@html config.icon}</span>
      {/if}
      {#if urgencyColor}
        <span class="urgency-dot" style="background: {urgencyColor};" title="Urgency: {urgency}"></span>
      {/if}
      {#if showLabel && config}
        <span class="sentiment-label">{config.label}</span>
      {/if}
    </span>
  {/if}
{/if}

<style>
  .sentiment-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .sentiment-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 6px;
    border-radius: 4px;
    border: 0.5px solid transparent;
    font-size: 11px;
    font-weight: 500;
    letter-spacing: -0.01em;
    white-space: nowrap;
    user-select: none;
  }
  .sentiment-compact {
    padding: 2px;
    min-width: 0;
  }
  .sentiment-icon {
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }
  .urgency-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .sentiment-label {
    font-weight: 500;
  }
</style>
