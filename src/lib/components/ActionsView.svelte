<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { addToast } from "$lib/stores/toast";

  interface Props {
    accountId?: string;
    isMacOS?: boolean;
  }

  let { accountId, isMacOS = false }: Props = $props();

  interface ActionItem {
    id: number;
    account_id: string;
    thread_id: string;
    message_id: string | null;
    description: string;
    assignee: string | null;
    deadline: string | null;
    confidence: number;
    status: string;
    created_at: number;
    completed_at: number | null;
    thread_subject: string | null;
    thread_sender: string | null;
  }

  interface GroupedItems {
    threadId: string;
    threadSubject: string;
    threadSender: string;
    items: ActionItem[];
  }

  let items = $state<ActionItem[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let activeTab = $state<"pending" | "completed">("pending");
  let completingIds = $state<Set<number>>(new Set());
  let dismissingIds = $state<Set<number>>(new Set());

  async function loadItems() {
    loading = true;
    error = null;
    try {
      const result = await invoke<ActionItem[]>("get_action_items", {
        accountId: accountId ?? null,
        status: activeTab,
        limit: 100,
      });
      items = result ?? [];
    } catch (e) {
      error = String(e);
      addToast(`Failed to load action items: ${e}`, "error", 5000);
    } finally {
      loading = false;
    }
  }

  async function handleMarkComplete(actionItemId: number) {
    completingIds = new Set([...completingIds, actionItemId]);
    try {
      await invoke("mark_action_complete", { actionItemId });
      addToast("Action marked as complete", "success", 3000);
      items = items.filter(i => i.id !== actionItemId);
    } catch (e) {
      addToast(`Failed to complete action: ${e}`, "error", 5000);
    } finally {
      const next = new Set(completingIds);
      next.delete(actionItemId);
      completingIds = next;
    }
  }

  async function handleDismiss(actionItemId: number) {
    dismissingIds = new Set([...dismissingIds, actionItemId]);
    try {
      await invoke("dismiss_action_item", { actionItemId });
      addToast("Action dismissed", "info", 3000);
      items = items.filter(i => i.id !== actionItemId);
    } catch (e) {
      addToast(`Failed to dismiss action: ${e}`, "error", 5000);
    } finally {
      const next = new Set(dismissingIds);
      next.delete(actionItemId);
      dismissingIds = next;
    }
  }

  function switchTab(tab: "pending" | "completed") {
    if (tab === activeTab) return;
    activeTab = tab;
    items = [];
    loadItems();
  }

  function formatRelativeDeadline(deadline: string | null): string | null {
    if (!deadline) return null;
    const target = new Date(deadline);
    const now = new Date();
    const diffMs = target.getTime() - now.getTime();
    const diffDays = Math.round(diffMs / (1000 * 60 * 60 * 24));
    if (diffDays < 0) return `Overdue by ${Math.abs(diffDays)} day${Math.abs(diffDays) === 1 ? '' : 's'}`;
    if (diffDays === 0) return "Today";
    if (diffDays === 1) return "Tomorrow";
    if (diffDays < 7) return `In ${diffDays} days`;
    if (diffDays < 30) return `In ${Math.floor(diffDays / 7)} week${Math.floor(diffDays / 7) === 1 ? '' : 's'}`;
    return target.toLocaleDateString();
  }

  function formatConfidence(confidence: number): string {
    return `${Math.round(confidence * 100)}%`;
  }

  function confidenceColor(confidence: number): string {
    if (confidence >= 0.8) return "high";
    if (confidence >= 0.5) return "medium";
    return "low";
  }

  let groupedItems = $derived.by(() => {
    const groups = new Map<string, ActionItem[]>();
    for (const item of items) {
      const g = groups.get(item.thread_id);
      if (g) {
        g.push(item);
      } else {
        groups.set(item.thread_id, [item]);
      }
    }
    const result: GroupedItems[] = [];
    for (const [threadId, threadItems] of groups) {
      result.push({
        threadId,
        threadSubject: threadItems[0].thread_subject ?? "(No subject)",
        threadSender: threadItems[0].thread_sender ?? "",
        items: threadItems,
      });
    }
    result.sort((a, b) => b.items[0].created_at - a.items[0].created_at);
    return result;
  });

  let pendingBadge = $state(0);

  async function loadPendingCount() {
    try {
      const pendingItems = await invoke<ActionItem[]>("get_action_items", {
        accountId: accountId ?? null,
        status: "pending",
        limit: 1,
      });
      pendingBadge = pendingItems.length >= 100 ? "99+" : pendingItems.length;
    } catch {
      pendingBadge = 0;
    }
  }

  $effect(() => {
    loadItems();
    loadPendingCount();
  });
</script>

<div class="actions-panel">
  <div class="titlebar-spacer" data-tauri-drag-region>
    {#if !isMacOS}
      <div class="window-controls">
        <button class="win-ctrl win-minimize" onclick={() => getCurrentWindow().minimize()} title="Minimize">
          <svg width="10" height="1" viewBox="0 0 10 1"><rect width="10" height="1" fill="currentColor"/></svg>
        </button>
        <button class="win-ctrl win-maximize" onclick={() => getCurrentWindow().toggleMaximize()} title="Maximize">
          <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1"/></svg>
        </button>
        <button class="win-ctrl win-close" onclick={() => getCurrentWindow().close()} title="Close">
          <svg width="10" height="10" viewBox="0 0 10 10"><line x1="0" y1="0" x2="10" y2="10" stroke="currentColor" stroke-width="1.2"/><line x1="10" y1="0" x2="0" y2="10" stroke="currentColor" stroke-width="1.2"/></svg>
        </button>
      </div>
    {/if}
  </div>
  <div class="actions-header" data-tauri-drag-region>
    <div class="header-title">
      <h2>Action Items</h2>
    </div>
    <button class="refresh-btn" onclick={loadItems} disabled={loading}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class:spin={loading}>
        <polyline points="23 4 23 10 17 10"/>
        <polyline points="1 20 1 14 7 14"/>
        <path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/>
      </svg>
    </button>
  </div>

  <div class="filter-bar">
    <div class="filter-tabs">
      <button class="filter-tab" class:active={activeTab === "pending"} onclick={() => switchTab("pending")}>
        Pending
        {#if pendingBadge > 0}
          <span class="tab-badge">{pendingBadge}</span>
        {/if}
      </button>
      <button class="filter-tab" class:active={activeTab === "completed"} onclick={() => switchTab("completed")}>
        Completed
      </button>
    </div>
  </div>

  <div class="actions-content">
    {#if loading}
      <div class="state-container">
        <div class="loading-spinner"></div>
        <span>Loading action items...</span>
      </div>
    {:else if error}
      <div class="state-container">
        <span class="error-text">{error}</span>
        <button class="retry-btn" onclick={loadItems}>Retry</button>
      </div>
    {:else if groupedItems.length === 0}
      <div class="state-container">
        <span class="empty-text">
          {activeTab === "pending" ? "No pending action items" : "No completed action items"}
        </span>
      </div>
    {:else}
      <div class="groups-list">
        {#each groupedItems as group}
          <div class="thread-group">
            <div class="thread-header">
              <div class="thread-header-text">
                <span class="thread-subject">{group.threadSubject}</span>
                {#if group.threadSender}
                  <span class="thread-sender">{group.threadSender}</span>
                {/if}
              </div>
            </div>
            <div class="items-list">
              {#each group.items as item}
                <div class="action-card">
                  <div class="card-left">
                    {#if activeTab === "pending"}
                      <button
                        class="check-btn"
                        disabled={completingIds.has(item.id)}
                        onclick={() => handleMarkComplete(item.id)}
                        title="Mark complete"
                      >
                        {#if completingIds.has(item.id)}
                          <div class="spinner-xs"></div>
                        {:else}
                          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <circle cx="12" cy="12" r="10"/>
                          </svg>
                        {/if}
                      </button>
                    {/if}
                  </div>
                  <div class="card-body">
                    <span class="action-description">{item.description}</span>
                    <div class="action-meta">
                      {#if item.assignee}
                        <span class="meta-tag assignee-tag">
                          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/>
                            <circle cx="12" cy="7" r="4"/>
                          </svg>
                          {item.assignee}
                        </span>
                      {/if}
                      {#if item.deadline}
                        {@const relative = formatRelativeDeadline(item.deadline)}
                        {#if relative}
                          <span class="meta-tag deadline-tag" class:overdue={relative.startsWith("Overdue")}>
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                              <circle cx="12" cy="12" r="10"/>
                              <polyline points="12 6 12 12 16 14"/>
                            </svg>
                            {relative}
                          </span>
                        {/if}
                      {/if}
                      {#if activeTab === "completed" && item.completed_at}
                        <span class="meta-tag completed-tag">
                          Done {new Date(item.completed_at * 1000).toLocaleDateString()}
                        </span>
                      {/if}
                    </div>
                  </div>
                  <div class="card-right">
                    <span class="confidence-pill confidence-{confidenceColor(item.confidence)}">
                      {formatConfidence(item.confidence)}
                    </span>
                    {#if activeTab === "pending"}
                      <button
                        class="dismiss-btn"
                        disabled={dismissingIds.has(item.id)}
                        onclick={() => handleDismiss(item.id)}
                        title="Dismiss"
                      >
                        {#if dismissingIds.has(item.id)}
                          <div class="spinner-xs"></div>
                        {:else}
                          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <line x1="18" y1="6" x2="6" y2="18"/>
                            <line x1="6" y1="6" x2="18" y2="18"/>
                          </svg>
                        {/if}
                      </button>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .titlebar-spacer {
    height: 28px;
    flex-shrink: 0;
    -webkit-app-region: drag;
    display: flex;
    align-items: center;
    justify-content: flex-end;
  }
  .window-controls {
    display: flex;
    align-items: center;
    height: 100%;
    -webkit-app-region: no-drag;
  }
  .win-ctrl {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 46px;
    height: 100%;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.1s;
  }
  .win-ctrl:hover {
    background: var(--bg-hover, rgba(128, 128, 128, 0.2));
  }
  .win-close:hover {
    background: #e81123;
    color: #fff;
  }

  .actions-panel {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    height: 100%;
    background: var(--bg-view);
    color: var(--text-primary);
    font-family: var(--font-family);
    font-size: 13px;
    container-type: inline-size;
  }

  .actions-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 50px;
    padding: 0 20px;
    border-bottom: 1px solid var(--border-color);
  }

  .header-title h2 {
    margin: 0;
    font-size: var(--font-size-heading);
    font-weight: 600;
  }

  .refresh-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 1px solid var(--border-color);
    border-radius: 8px;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition: all 0.15s;
    font-family: inherit;
  }
  .refresh-btn:hover:not(:disabled) {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }
  .refresh-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .spin {
    animation: spin 0.6s linear infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .filter-bar {
    display: flex;
    align-items: center;
    padding: 10px 20px;
    border-bottom: 1px solid var(--border-color);
  }

  .filter-tabs {
    display: flex;
    gap: 1px;
    background: rgba(0, 0, 0, 0.08);
    border-radius: 8px;
    padding: 2px;
  }

  :global([data-theme="dark"]) .filter-tabs {
    background: rgba(255, 255, 255, 0.08);
  }

  .filter-tab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 14px;
    border: none;
    background: transparent;
    border-radius: 6px;
    font-size: var(--font-size-base);
    font-family: inherit;
    color: var(--text-primary);
    cursor: pointer;
    font-weight: 400;
    transition: all 0.15s ease;
    white-space: nowrap;
  }

  .filter-tab:hover:not(.active) {
    background: rgba(0, 0, 0, 0.04);
  }

  :global([data-theme="dark"]) .filter-tab:hover:not(.active) {
    background: rgba(255, 255, 255, 0.04);
  }

  .filter-tab.active {
    background: var(--bg-view);
    font-weight: 500;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12), 0 0 0 0.5px rgba(0, 0, 0, 0.04);
  }

  .tab-badge {
    background: var(--accent-blue);
    color: white;
    font-size: 10px;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: 8px;
    line-height: 14px;
  }

  .actions-content {
    flex: 1;
    overflow-y: auto;
    padding: 16px 20px;
  }

  .state-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 60px 20px;
    color: var(--text-secondary);
    gap: 12px;
  }

  .loading-spinner {
    width: 24px;
    height: 24px;
    border: 2px solid var(--border-color);
    border-top-color: var(--accent-blue);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  .spinner-xs {
    width: 14px;
    height: 14px;
    border: 1.5px solid var(--border-color);
    border-top-color: var(--accent-blue);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  .error-text {
    color: #ff453a;
    font-size: 13px;
    text-align: center;
  }

  .retry-btn {
    padding: 6px 14px;
    background: var(--accent-blue);
    color: white;
    border: none;
    border-radius: 6px;
    font-size: 12px;
    cursor: pointer;
    font-family: inherit;
  }

  .empty-text {
    font-size: 14px;
  }

  .groups-list {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .thread-group {
    display: flex;
    flex-direction: column;
  }

  .thread-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0 10px 0;
  }

  .thread-header-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .thread-subject {
    font-weight: 600;
    font-size: 13px;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .thread-sender {
    font-size: 11px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .items-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .action-card {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px;
    border-radius: 12px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    transition: box-shadow 0.15s;
  }

  .action-card:hover {
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.06);
  }

  .card-left {
    flex-shrink: 0;
    padding-top: 1px;
  }

  .check-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    border-radius: 50%;
    transition: all 0.15s;
  }

  .check-btn:hover:not(:disabled) {
    color: #34c759;
    background: rgba(52, 199, 89, 0.1);
  }

  .check-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .card-body {
    flex: 1;
    min-width: 0;
  }

  .action-description {
    display: block;
    font-size: 13px;
    line-height: 18px;
    color: var(--text-primary);
    word-wrap: break-word;
  }

  .action-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 6px;
  }

  .meta-tag {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    color: var(--text-secondary);
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.04);
  }

  :global([data-theme="dark"]) .meta-tag {
    background: rgba(255, 255, 255, 0.05);
  }

  .assignee-tag svg {
    flex-shrink: 0;
  }

  .deadline-tag {
    color: #ff9f0a;
  }

  .deadline-tag.overdue {
    color: #ff453a;
  }

  .completed-tag {
    color: #34c759;
  }

  .card-right {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .confidence-pill {
    display: inline-block;
    padding: 2px 7px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 500;
    white-space: nowrap;
  }

  .confidence-high {
    background: rgba(52, 199, 89, 0.12);
    color: #34c759;
  }

  .confidence-medium {
    background: rgba(255, 159, 10, 0.12);
    color: #c77c00;
  }

  :global([data-theme="dark"]) .confidence-medium {
    color: #ff9f0a;
  }

  .confidence-low {
    background: rgba(142, 142, 147, 0.12);
    color: #8e8e93;
  }

  .dismiss-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    border-radius: 6px;
    transition: all 0.15s;
  }

  .dismiss-btn:hover:not(:disabled) {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }

  .dismiss-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* Narrow: tighten spacing */
  @container (max-width: 600px) {
    .actions-header {
      padding: 0 16px;
    }
    .filter-bar {
      padding: 10px 16px;
    }
    .actions-content {
      padding: 12px 16px;
    }
  }
</style>
