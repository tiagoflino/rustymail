<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { addToast } from "$lib/stores/toast";

  interface Props {
    accountId: string;
  }

  let { accountId }: Props = $props();

  interface Subscription {
    id: string;
    sender_name: string;
    sender_email: string;
    message_count: number;
    avg_frequency_days: number | null;
    last_seen: string | null;
    status: "active" | "unsubscribed" | "ignored";
    detection_method: string;
    unsubscribe_method: string | null;
    unsubscribe_mailto: string | null;
  }

  let subscriptions = $state<Subscription[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let filter = $state<"all" | "active" | "unsubscribed" | "ignored">("all");
  let searchQuery = $state("");
  let scanning = $state(false);

  async function loadSubscriptions() {
    loading = true;
    error = null;
    try {
      const result = await invoke<Subscription[]>("get_subscriptions", { accountId });
      subscriptions = result;
    } catch (e) {
      error = String(e);
      addToast(`Failed to load subscriptions: ${e}`, "error", 5000);
    } finally {
      loading = false;
    }
  }

  async function handleScan() {
    scanning = true;
    try {
      const result = await invoke<{ scanned: number; found: number }>("scan_subscriptions", { accountId });
      addToast(`Scanned ${result.scanned} messages, found ${result.found} subscriptions`, "success", 4000);
      await loadSubscriptions();
    } catch (e) {
      addToast(`Scan failed: ${e}`, "error", 5000);
    } finally {
      scanning = false;
    }
  }

  async function handleUnsubscribe(sub: Subscription) {
    try {
      const result = await invoke<{ method: string; mailto?: string }>("unsubscribe", { subscriptionId: sub.id });
      if (result.method === "mailto" && result.mailto) {
        window.location.href = result.mailto;
      }
      addToast("Successfully unsubscribed", "success", 3000);
      await loadSubscriptions();
    } catch (e) {
      addToast(`Failed to unsubscribe: ${e}`, "error", 5000);
    }
  }

  async function handleCorrectSubscription(sub: Subscription) {
    try {
      await invoke("correct_subscription", { subscriptionId: sub.id, isSubscription: false });
      addToast("Marked as not a subscription", "success", 3000);
      await loadSubscriptions();
    } catch (e) {
      addToast(`Failed to correct subscription: ${e}`, "error", 5000);
    }
  }

  async function handleDelete(sub: Subscription) {
    try {
      await invoke("delete_subscription", { subscriptionId: sub.id });
      addToast("Subscription deleted", "success", 3000);
      await loadSubscriptions();
    } catch (e) {
      addToast(`Failed to delete: ${e}`, "error", 5000);
    }
  }

  function formatRelativeTime(dateStr: string | null): string {
    if (!dateStr) return "Never";
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
    if (diffDays === 0) return "Today";
    if (diffDays === 1) return "Yesterday";
    if (diffDays < 7) return `${diffDays} days ago`;
    if (diffDays < 30) return `${Math.floor(diffDays / 7)} weeks ago`;
    if (diffDays < 365) return `${Math.floor(diffDays / 30)} months ago`;
    return `${Math.floor(diffDays / 365)} years ago`;
  }

  function formatFrequency(days: number | null): string {
    if (days === null) return "Unknown";
    if (days === 0) return "Daily";
    if (days === 1) return "Every day";
    return `Every ${Math.round(days)} days`;
  }

  let filteredSubscriptions = $derived(() => {
    let result = subscriptions;
    
    if (filter !== "all") {
      result = result.filter(s => s.status === filter);
    }
    
    if (searchQuery) {
      const q = searchQuery.toLowerCase();
      result = result.filter(s => 
        s.sender_name.toLowerCase().includes(q) || 
        s.sender_email.toLowerCase().includes(q)
      );
    }
    
    return result;
  });

  onMount(() => {
    loadSubscriptions();
  });
</script>

<div class="subscriptions-panel">
  <div class="subscriptions-header">
    <div class="header-title">
      <h2>Subscriptions</h2>
      <span class="count-badge">{subscriptions.length}</span>
    </div>
    <button class="scan-btn" onclick={handleScan} disabled={scanning}>
      {scanning ? "Scanning..." : "Scan"}
    </button>
  </div>

  <div class="filter-bar">
    <div class="filter-tabs">
      <button class="filter-tab" class:active={filter === "all"} onclick={() => filter = "all"}>All</button>
      <button class="filter-tab" class:active={filter === "active"} onclick={() => filter = "active"}>Active</button>
      <button class="filter-tab" class:active={filter === "unsubscribed"} onclick={() => filter = "unsubscribed"}>Unsubscribed</button>
      <button class="filter-tab" class:active={filter === "ignored"} onclick={() => filter = "ignored"}>Ignored</button>
    </div>
    <div class="search-input-wrapper">
      <input 
        type="text" 
        placeholder="Filter by sender..." 
        bind:value={searchQuery}
        class="search-input"
      />
    </div>
  </div>

  <div class="subscriptions-content">
    {#if loading}
      <div class="loading-state">
        <div class="loading-spinner"></div>
        <span>Loading subscriptions...</span>
      </div>
    {:else if error}
      <div class="error-state">
        <span>{error}</span>
        <button onclick={loadSubscriptions}>Retry</button>
      </div>
    {:else if filteredSubscriptions().length === 0}
      <div class="empty-state">
        <span>No subscriptions found</span>
      </div>
    {:else}
      <div class="subscriptions-table">
        <div class="table-header">
          <div class="col-sender">Sender</div>
          <div class="col-count">Messages</div>
          <div class="col-frequency">Frequency</div>
          <div class="col-lastseen">Last Seen</div>
          <div class="col-status">Status</div>
          <div class="col-method">Method</div>
          <div class="col-actions">Actions</div>
        </div>
        {#each filteredSubscriptions() as sub}
          <div class="table-row">
            <div class="col-sender">
              <div class="sender-info">
                <span class="sender-name">{sub.sender_name || "Unknown"}</span>
                <span class="sender-email">{sub.sender_email}</span>
              </div>
            </div>
            <div class="col-count">{sub.message_count}</div>
            <div class="col-frequency">{formatFrequency(sub.avg_frequency_days)}</div>
            <div class="col-lastseen">{formatRelativeTime(sub.last_seen)}</div>
            <div class="col-status">
              <span class="status-badge status-{sub.status}">{sub.status}</span>
            </div>
            <div class="col-method">{sub.detection_method}</div>
            <div class="col-actions">
              {#if sub.status === "active"}
                <button class="action-btn unsubscribe-btn" onclick={() => handleUnsubscribe(sub)}>Unsubscribe</button>
              {/if}
              <button class="action-btn correct-btn" onclick={() => handleCorrectSubscription(sub)}>Not a subscription</button>
              <button class="action-btn delete-btn" onclick={() => handleDelete(sub)}>Delete</button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .subscriptions-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-view);
    color: var(--text-primary);
    font-family: var(--font-family);
    font-size: 13px;
  }

  .subscriptions-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-color);
    background: var(--bg-sidebar);
  }

  .header-title {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .header-title h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.3px;
  }

  .count-badge {
    background: var(--accent-blue);
    color: white;
    font-size: 11px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 10px;
  }

  .scan-btn {
    padding: 6px 14px;
    background: var(--accent-blue);
    color: white;
    border: none;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    font-family: var(--font-family);
    transition: opacity 0.15s;
  }

  .scan-btn:hover:not(:disabled) {
    opacity: 0.9;
  }

  .scan-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .filter-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border-color);
    gap: 16px;
    flex-wrap: wrap;
  }

  .filter-tabs {
    display: flex;
    gap: 4px;
  }

  .filter-tab {
    padding: 6px 12px;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    font-family: var(--font-family);
    transition: all 0.15s;
  }

  .filter-tab:hover {
    background: var(--sidebar-hover);
  }

  .filter-tab.active {
    background: var(--accent-blue);
    color: white;
  }

  .search-input-wrapper {
    flex: 1;
    max-width: 280px;
    min-width: 180px;
  }

  .search-input {
    width: 100%;
    padding: 6px 12px;
    background: var(--bg-view);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    color: var(--text-primary);
    font-size: 12px;
    font-family: var(--font-family);
  }

  .search-input:focus {
    outline: none;
    border-color: var(--accent-blue);
  }

  .search-input::placeholder {
    color: var(--text-secondary);
  }

  .subscriptions-content {
    flex: 1;
    overflow-y: auto;
    padding: 0;
  }

  .loading-state,
  .error-state,
  .empty-state {
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

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .error-state button {
    padding: 6px 14px;
    background: var(--accent-blue);
    color: white;
    border: none;
    border-radius: 6px;
    font-size: 12px;
    cursor: pointer;
    font-family: var(--font-family);
  }

  .subscriptions-table {
    width: 100%;
  }

  .table-header {
    display: flex;
    padding: 10px 20px;
    background: var(--bg-sidebar);
    border-bottom: 1px solid var(--border-color);
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .table-row {
    display: flex;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border-color);
    align-items: center;
    transition: background 0.1s;
  }

  .table-row:hover {
    background: var(--sidebar-hover);
  }

  .col-sender {
    flex: 2;
    min-width: 180px;
  }

  .col-count,
  .col-frequency,
  .col-lastseen,
  .col-method {
    flex: 1;
    min-width: 80px;
    color: var(--text-secondary);
    font-size: 12px;
  }

  .col-status {
    flex: 0.8;
    min-width: 80px;
  }

  .col-actions {
    flex: 1.5;
    display: flex;
    gap: 6px;
    min-width: 200px;
    flex-wrap: wrap;
  }

  .sender-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .sender-name {
    font-weight: 500;
    color: var(--text-primary);
  }

  .sender-email {
    font-size: 11px;
    color: var(--text-secondary);
  }

  .status-badge {
    display: inline-block;
    padding: 3px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 500;
    text-transform: capitalize;
  }

  .status-active {
    background: rgba(52, 199, 89, 0.15);
    color: #34c759;
  }

  .status-unsubscribed {
    background: rgba(142, 142, 147, 0.15);
    color: #8e8e93;
  }

  .status-ignored {
    background: rgba(255, 204, 0, 0.15);
    color: #ffcc00;
  }

  .action-btn {
    padding: 4px 8px;
    background: transparent;
    border: 1px solid var(--border-color);
    border-radius: 4px;
    font-size: 11px;
    cursor: pointer;
    font-family: var(--font-family);
    transition: all 0.15s;
    white-space: nowrap;
  }

  .action-btn:hover {
    background: var(--sidebar-hover);
  }

  .unsubscribe-btn {
    border-color: #34c759;
    color: #34c759;
  }

  .unsubscribe-btn:hover {
    background: rgba(52, 199, 89, 0.1);
  }

  .correct-btn {
    color: var(--text-secondary);
  }

  .delete-btn {
    color: #ff453a;
    border-color: rgba(255, 69, 58, 0.3);
  }

  .delete-btn:hover {
    background: rgba(255, 69, 58, 0.1);
  }
</style>
