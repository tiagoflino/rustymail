<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { fly, fade } from "svelte/transition";
  import { iconSnooze } from "$lib/components/icons";

  interface Action {
    id: string;
    label: string;
    icon?: string;
    shortcut?: string;
  }

  let {
    show = $bindable(false),
    accounts = [],
    hasThread = false,
    onClose = () => {},
    onAction = (id: string) => {},
  }: {
    show?: boolean;
    accounts?: Array<{ id: string; email: string; display_name: string }>;
    hasThread?: boolean;
    onClose?: () => void;
    onAction?: (id: string) => void;
  } = $props();

  let query = $state("");
  let activeIndex = $state(0);
  let inputEl = $state<HTMLInputElement>();

  let baseActions: Action[] = $derived([
    { id: "compose", label: "Compose New Email", shortcut: "C" },
    { id: "sync", label: "Sync Now", shortcut: "⌘R" },
    { id: "settings", label: "Open Settings", shortcut: "⌘," },
    { id: "theme", label: "Toggle Theme" },
    { id: "sidebar", label: "Toggle Sidebar", shortcut: "[" },
    { id: "view_mail", label: "Switch to Mail View" },
    { id: "view_calendar", label: "Switch to Calendar View" },
    { id: "nav_inbox", label: "Go to Inbox" },
    { id: "nav_sent", label: "Go to Sent" },
    { id: "nav_drafts", label: "Go to Drafts" },
    { id: "nav_trash", label: "Go to Trash" },
    ...(hasThread ? [
      { id: "snooze_later_today", label: "Snooze: Later Today", icon: iconSnooze },
      { id: "snooze_tomorrow", label: "Snooze: Tomorrow Morning", icon: iconSnooze },
      { id: "snooze_next_week", label: "Snooze: Next Week", icon: iconSnooze },
    ] : []),
  ]);

  let accountActions: Action[] = $derived(accounts.slice(0, 2).map(acc => ({
    id: `switch_account_${acc.id}`,
    label: `Switch to ${acc.display_name || acc.email}`,
    icon: `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>`
  })));

  let allActions = $derived([...baseActions, ...accountActions]);
  let filteredActions = $derived(allActions.filter(a => a.label.toLowerCase().includes(query.toLowerCase())));

  $effect(() => {
    if (query) {
      activeIndex = 0;
    }
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      activeIndex = (activeIndex + 1) % filteredActions.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      activeIndex = (activeIndex - 1 + filteredActions.length) % filteredActions.length;
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (filteredActions[activeIndex]) {
        execute(filteredActions[activeIndex].id);
      }
    }
  }

  function execute(id: string) {
    onAction(id);
    onClose();
  }

  $effect(() => {
    if (show) {
      query = "";
      activeIndex = 0;
      setTimeout(() => inputEl?.focus(), 50);
    }
  });

  const iconSearch = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"></circle><line x1="21" y1="21" x2="16.65" y2="16.65"></line></svg>`;
</script>

{#if show}
  <div
    class="palette-backdrop"
    transition:fade={{ duration: 150 }}
    onclick={onClose}
    onkeydown={(e) => e.key === "Escape" && onClose()}
    role="dialog"
    tabindex="-1"
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="palette-modal"
      transition:fly={{ y: -10, duration: 200 }}
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.key === "Escape" && onClose()}
      role="document"
      tabindex="-1"
    >
      <div class="search-header">
        <span class="search-icon">{@html iconSearch}</span>
        <input 
          bind:this={inputEl}
          bind:value={query}
          onkeydown={handleKeydown}
          placeholder="What do you want to do?" 
          type="text" 
          autocomplete="off"
          spellcheck="false"
        />
      </div>
      
      {#if filteredActions.length > 0}
        <div class="actions-list">
          {#each filteredActions as action, i}
            <!-- svelte-ignore a11y_mouse_events_have_key_events -->
            <div
              class="action-item"
              class:selected={i === activeIndex}
              onmouseover={() => activeIndex = i}
              onfocus={() => activeIndex = i}
              onclick={() => execute(action.id)}
              onkeydown={(e) => (e.key === "Enter" || e.key === " ") && execute(action.id)}
              role="button"
              tabindex="0"
            >
              <div class="action-label">
                {#if action.icon}
                  <span class="action-icon">{@html action.icon}</span>
                {/if}
                {action.label}
              </div>
              {#if action.shortcut}
                <kbd class="action-shortcut">{action.shortcut}</kbd>
              {/if}
            </div>
          {/each}
        </div>
      {:else}
        <div class="empty-state">No commands found</div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .palette-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.2);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    z-index: 10000;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 15vh;
  }

  :global(body.test-dark) .palette-backdrop {
    background: rgba(255, 255, 255, 0.05);
  }

  .palette-modal {
    width: 600px;
    max-width: 90vw;
    background: var(--bg-view, #ffffff);
    border-radius: 12px;
    box-shadow: 0 20px 40px rgba(0,0,0,0.3), 0 0 0 1px rgba(128,128,128,0.15);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .search-header {
    display: flex;
    align-items: center;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-color);
  }

  .search-icon {
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    margin-right: 12px;
  }

  .search-header input {
    flex: 1;
    border: none;
    background: transparent;
    font-size: 20px;
    color: var(--text-primary);
    outline: none;
    font-weight: 400;
  }

  .search-header input::placeholder {
    color: var(--text-secondary);
    opacity: 0.6;
  }

  .actions-list {
    max-height: 350px;
    overflow-y: auto;
    padding: 8px;
  }

  .action-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-radius: 8px;
    cursor: default;
    color: var(--text-primary);
  }

  .action-item.selected {
    background: var(--accent-blue, #0A7CFF);
    color: #fff;
  }

  .action-label {
    display: flex;
    align-items: center;
    font-size: 14px;
    font-weight: 500;
  }

  .action-icon {
    margin-right: 10px;
    display: flex;
    align-items: center;
    opacity: 0.8;
  }

  .action-shortcut {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
    font-size: 12px;
    background: var(--sidebar-hover, rgba(0,0,0,0.06));
    border: 1px solid var(--border-color);
    padding: 2px 6px;
    border-radius: 4px;
    color: var(--text-secondary);
  }

  .action-item.selected .action-shortcut {
    background: rgba(255,255,255,0.2);
    border-color: transparent;
    color: #fff;
  }

  .empty-state {
    padding: 30px;
    text-align: center;
    color: var(--text-secondary);
    font-size: 14px;
  }
</style>
