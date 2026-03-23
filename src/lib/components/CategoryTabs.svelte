<script lang="ts">
  import { type Writable } from "svelte/store";

  interface Props {
    selectedCategory: Writable<string>;
    onselectcategory: (category: string) => void;
  }

  let { selectedCategory, onselectcategory }: Props = $props();

  const categories = [
    { id: "primary", label: "Primary" },
    { id: "social", label: "Social" },
    { id: "promotions", label: "Promotions" },
    { id: "important", label: "Important" },
  ];
</script>

<div class="category-tabs" role="tablist" aria-label="Email categories">
  {#each categories as category}
    <button
      class="tab {$selectedCategory === category.id ? 'active' : ''}"
      role="tab"
      aria-selected={$selectedCategory === category.id}
      onclick={() => onselectcategory(category.id)}
    >
      <span class="tab-label">{category.label}</span>
    </button>
  {/each}
</div>

<style>
  .category-tabs {
    display: flex;
    align-items: center;
    gap: 0;
    padding: 0 8px;
    background: transparent;
    border-bottom: 1px solid var(--border-color);
  }

  .tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 12px;
    min-height: 36px;
    background: transparent;
    border: none;
    border-radius: 0;
    cursor: pointer;
    font-family: var(--font-family);
    font-size: 12px;
    line-height: 14px;
    letter-spacing: -0.05px;
    color: var(--text-secondary);
    transition: color 0.15s ease, background-color 0.15s ease;
    -webkit-font-smoothing: antialiased;
  }

  .tab::after {
    content: '';
    position: absolute;
    bottom: -1px;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--accent-blue);
    opacity: 0;
    transition: opacity 0.15s ease;
  }

  .tab:hover:not(.active) {
    color: var(--text-primary);
    background: var(--sidebar-hover);
    border-radius: 4px;
  }

  .tab.active {
    color: var(--accent-blue);
    font-weight: 500;
  }

  .tab.active::after {
    opacity: 1;
  }

  .tab-label {
    font-weight: 400;
  }

  .tab.active .tab-label {
    font-weight: 500;
  }
</style>
