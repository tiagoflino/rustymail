<script lang="ts">
  import { onMount } from "svelte";
  import { scale } from "svelte/transition";
  import { snoozeOptions, formatSnoozePreview } from "$lib/utils/snooze";

  interface Props {
    onsnooze: (until: number) => void;
    onclose: () => void;
  }

  let { onsnooze, onclose }: Props = $props();
  let activeIndex = $state(0);
  let menuEl: HTMLElement | undefined = $state();
  let items: HTMLElement[] = $state([]);

  const iconLaterToday = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>`;
  const iconTomorrow = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M17 18a5 5 0 00-10 0"/><line x1="12" y1="2" x2="12" y2="9"/><line x1="4.22" y1="10.22" x2="5.64" y2="11.64"/><line x1="1" y1="18" x2="3" y2="18"/><line x1="21" y1="18" x2="23" y2="18"/><line x1="18.36" y1="11.64" x2="19.78" y2="10.22"/><line x1="23" y1="22" x2="1" y2="22"/><polyline points="8 6 12 2 16 6"/></svg>`;
  const iconNextWeek = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="18" rx="2" ry="2"/><line x1="16" y1="2" x2="16" y2="6"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="3" y1="10" x2="21" y2="10"/></svg>`;

  const icons = [iconLaterToday, iconTomorrow, iconNextWeek];

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      activeIndex = (activeIndex + 1) % snoozeOptions.length;
      items[activeIndex]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      activeIndex = (activeIndex - 1 + snoozeOptions.length) % snoozeOptions.length;
      items[activeIndex]?.focus();
    } else if (e.key === "Enter") {
      e.preventDefault();
      handleSelect(activeIndex);
    }
  }

  function handleSelect(index: number) {
    onsnooze(snoozeOptions[index].compute());
  }

  onMount(() => {
    items[0]?.focus();
  });
</script>

<div class="snooze-backdrop" onclick={onclose} role="presentation"></div>
<div
  class="snooze-popover"
  role="menu"
  tabindex="-1"
  aria-label="Snooze options"
  bind:this={menuEl}
  onkeydown={handleKeydown}
  transition:scale={{ duration: 150, start: 0.95, opacity: 0 }}
>
  <div class="snooze-header" role="presentation">Snooze until...</div>
  {#each snoozeOptions as opt, i}
    <button
      class="snooze-option"
      class:active={activeIndex === i}
      role="menuitem"
      tabindex={activeIndex === i ? 0 : -1}
      bind:this={items[i]}
      onclick={() => handleSelect(i)}
      onfocus={() => activeIndex = i}
    >
      <span class="snooze-icon">{@html icons[i]}</span>
      <div class="snooze-text">
        <span class="snooze-label">{opt.label}</span>
        <span class="snooze-preview">{formatSnoozePreview(opt.compute())}</span>
      </div>
    </button>
  {/each}
</div>

<style>
  .snooze-backdrop {
    position: fixed;
    inset: 0;
    z-index: 199;
  }
  .snooze-popover {
    background: var(--bg-view);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-modal);
    padding: 4px;
    min-width: 240px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15), 0 0 0 0.5px rgba(0, 0, 0, 0.06);
    z-index: 200;
    position: relative;
  }
  :global([data-theme="dark"]) .snooze-popover {
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4), 0 0 0 0.5px rgba(255, 255, 255, 0.08);
  }
  .snooze-header {
    padding: 8px 12px 4px;
    font-size: var(--font-size-small);
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .snooze-option {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    border: none;
    background: none;
    cursor: pointer;
    border-radius: var(--radius-standard);
    color: var(--text-primary);
    text-align: left;
    outline: none;
    font-family: inherit;
  }
  .snooze-option:hover,
  .snooze-option:focus-visible,
  .snooze-option.active {
    background: var(--sidebar-hover);
  }
  .snooze-option:focus-visible {
    box-shadow: 0 0 0 2px var(--accent-blue);
  }
  .snooze-icon {
    display: flex;
    align-items: center;
    color: var(--text-secondary);
  }
  .snooze-text {
    display: flex;
    flex-direction: column;
  }
  .snooze-label {
    font-size: var(--font-size-base);
    font-weight: 500;
  }
  .snooze-preview {
    font-size: var(--font-size-small);
    color: var(--text-secondary);
  }
</style>
