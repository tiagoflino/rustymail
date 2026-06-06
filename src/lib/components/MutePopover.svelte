<script lang="ts">
  import { onMount } from "svelte";
  import { scale } from "svelte/transition";
  import { muteOptions } from "$lib/utils/mute";

  interface Props {
    onmute: (until: number | null) => void;
    onclose: () => void;
  }

  let { onmute, onclose }: Props = $props();
  let activeIndex = $state(0);
  let menuEl: HTMLElement | undefined = $state();
  let items: HTMLElement[] = $state([]);

  const icons = [
    `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5L6 9H2v6h4l5 4V5z"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>`,
    `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5L6 9H2v6h4l5 4V5z"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>`,
    `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5L6 9H2v6h4l5 4V5z"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>`,
    `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"/><path d="M9 9v3a3 3 0 005.12 2.12M15 9.34V4a3 3 0 00-5.94-.6"/><path d="M17 16.95A7 7 0 015 12v-2m14 0v2a7 7 0 01-.11 1.23M12 19v4"/><line x1="3" y1="3" x2="21" y2="21"/></svg>`,
  ];

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      activeIndex = (activeIndex + 1) % muteOptions.length;
      items[activeIndex]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      activeIndex = (activeIndex - 1 + muteOptions.length) % muteOptions.length;
      items[activeIndex]?.focus();
    } else if (e.key === "Enter") {
      e.preventDefault();
      handleSelect(activeIndex);
    }
  }

  function handleSelect(index: number) {
    onmute(muteOptions[index].compute());
  }

  onMount(() => {
    items[0]?.focus();
  });
</script>

<div class="mute-backdrop" onclick={onclose} role="presentation"></div>
<div
  class="mute-popover"
  role="menu"
  tabindex="-1"
  aria-label="Mute options"
  bind:this={menuEl}
  onkeydown={handleKeydown}
  transition:scale={{ duration: 150, start: 0.95, opacity: 0 }}
>
  <div class="mute-header" role="presentation">Mute for...</div>
  {#each muteOptions as opt, i}
    <button
      class="mute-option"
      class:active={activeIndex === i}
      role="menuitem"
      tabindex={activeIndex === i ? 0 : -1}
      bind:this={items[i]}
      onclick={() => handleSelect(i)}
      onfocus={() => activeIndex = i}
    >
      <span class="mute-icon">{@html icons[i]}</span>
      <div class="mute-text">
        <span class="mute-label">{opt.label}</span>
        <span class="mute-desc">{opt.description}</span>
      </div>
    </button>
  {/each}
</div>

<style>
  .mute-backdrop {
    position: fixed;
    inset: 0;
    z-index: 199;
  }
  .mute-popover {
    background: var(--bg-view);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-modal);
    padding: 4px;
    min-width: 240px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15), 0 0 0 0.5px rgba(0, 0, 0, 0.06);
    z-index: 200;
    position: relative;
  }
  :global([data-theme="dark"]) .mute-popover {
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4), 0 0 0 0.5px rgba(255, 255, 255, 0.08);
  }
  .mute-header {
    padding: 8px 12px 4px;
    font-size: var(--font-size-small);
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .mute-option {
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
  .mute-option:hover,
  .mute-option:focus-visible,
  .mute-option.active {
    background: var(--sidebar-hover);
  }
  .mute-option:focus-visible {
    box-shadow: 0 0 0 2px var(--accent-blue);
  }
  .mute-icon {
    display: flex;
    align-items: center;
    color: var(--text-secondary);
  }
  .mute-text {
    display: flex;
    flex-direction: column;
  }
  .mute-label {
    font-size: var(--font-size-base);
    font-weight: 500;
  }
  .mute-desc {
    font-size: var(--font-size-small);
    color: var(--text-secondary);
  }
</style>
