<script lang="ts">
  import { onMount } from "svelte";

  interface Props {
    labels: { id: string; name: string }[];
    onselect: (labelId: string) => void;
    onclose: () => void;
  }
  let { labels, onselect, onclose }: Props = $props();

  let filterText = $state("");
  let selectedIndex = $state(0);
  let inputRef: HTMLInputElement;

  const filteredLabels = $derived(
    labels.filter(l => l.name.toLowerCase().includes(filterText.toLowerCase()))
  );

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") { onclose(); return; }
    if (e.key === "ArrowDown") { e.preventDefault(); selectedIndex = Math.min(selectedIndex + 1, filteredLabels.length - 1); return; }
    if (e.key === "ArrowUp") { e.preventDefault(); selectedIndex = Math.max(selectedIndex - 1, 0); return; }
    if (e.key === "Enter" && filteredLabels[selectedIndex]) {
      onselect(filteredLabels[selectedIndex].id);
      return;
    }
  }

  onMount(() => { inputRef?.focus(); });
</script>

<div class="label-picker-backdrop" onclick={onclose} onkeydown={handleKeydown} role="button" tabindex="-1"></div>
<div class="label-picker" onkeydown={handleKeydown} role="listbox" aria-label="Label picker" tabindex="-1">
  <input
    bind:this={inputRef}
    bind:value={filterText}
    class="label-filter"
    placeholder="Filter labels..."
    oninput={() => { selectedIndex = 0; }}
  />
  <div class="label-list">
    {#each filteredLabels as label, i (label.id)}
      <button
        class="label-option {i === selectedIndex ? 'focused' : ''}"
        onclick={() => onselect(label.id)}
      >
        {label.name}
      </button>
    {:else}
      <div class="label-empty">No labels found</div>
    {/each}
  </div>
</div>

<style>
  .label-picker-backdrop {
    position: fixed;
    inset: 0;
    z-index: 999;
  }
  .label-picker {
    position: absolute;
    top: 100%;
    left: 0;
    margin-top: 4px;
    background: var(--bg-view);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-standard);
    box-shadow: 0 4px 16px rgba(0,0,0,0.12);
    z-index: 1000;
    width: 200px;
    max-height: 240px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .label-filter {
    padding: 8px 10px;
    border: none;
    border-bottom: 1px solid var(--border-color);
    background: transparent;
    color: var(--text-primary);
    font-size: var(--font-size-small);
    font-family: inherit;
    outline: none;
  }
  .label-filter::placeholder { color: var(--text-secondary); opacity: 0.6; }
  .label-list {
    overflow-y: auto;
    flex: 1;
  }
  .label-option {
    display: block;
    width: 100%;
    padding: 6px 10px;
    border: none;
    background: transparent;
    color: var(--text-primary);
    font-size: var(--font-size-small);
    font-family: inherit;
    text-align: left;
    cursor: pointer;
  }
  .label-option:hover, .label-option.focused {
    background: var(--sidebar-hover);
  }
  .label-empty {
    padding: 12px 10px;
    color: var(--text-secondary);
    font-size: var(--font-size-small);
    text-align: center;
  }
</style>
