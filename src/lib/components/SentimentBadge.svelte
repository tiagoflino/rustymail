<script lang="ts">
  import { toneConfig, urgencyConfig, type Tone, type Urgency } from "$lib/utils/sentiment";

  interface Props {
    sentiment?: Tone | null;
    urgency?: Urgency | null;
    showLabel?: boolean;
    compact?: boolean;
  }

  let {
    sentiment = null,
    urgency = null,
    showLabel = false,
    compact = false,
  }: Props = $props();

  let config = $derived(toneConfig(sentiment));
  let urgencyInfo = $derived(urgencyConfig(urgency));
</script>

{#if config || urgencyInfo}
  {#if compact}
    <span
      class="sentiment-dot shape-{config?.shape ?? 'circle'}"
      style="background: {config?.color || urgencyInfo?.color}; color: {config?.color || urgencyInfo?.color};"
      title={config?.label || urgencyInfo?.label || ''}
      role="img"
      aria-label={config?.label || urgencyInfo?.label || ''}
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
      {#if urgencyInfo}
        <span class="urgency-dot" style="background: {urgencyInfo.color};" title="Urgency: {urgencyInfo.label}"></span>
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
    width: 8px;
    height: 8px;
    flex-shrink: 0;
  }
  .shape-circle {
    border-radius: 50%;
  }
  .shape-square {
    border-radius: 1px;
  }
  .shape-diamond {
    border-radius: 1px;
    transform: rotate(45deg);
  }
  .shape-triangle {
    background: transparent !important;
    width: 0;
    height: 0;
    border-left: 4px solid transparent;
    border-right: 4px solid transparent;
    border-bottom: 8px solid currentColor;
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
