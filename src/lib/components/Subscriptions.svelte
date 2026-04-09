<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { addToast } from "$lib/stores/toast";
  import { iconBellOff, iconXCircle, iconTrash } from "$lib/components/icons";
  import UnsubscribeDialog from "$lib/components/UnsubscribeDialog.svelte";

  interface Props {
    accountId: string;
    onselectsubscription?: (senderEmail: string) => void;
  }

  let { accountId, onselectsubscription }: Props = $props();

  interface Subscription {
    id: number;
    sender_name: string | null;
    sender_email: string;
    message_count: number;
    avg_frequency_days: number | null;
    last_seen: number;
    status: "active" | "unsubscribed" | "ignored";
    detection_method: string;
    unsubscribe_url: string | null;
    unsubscribe_mailto: string | null;
    supports_one_click: boolean;
  }

  let subscriptions = $state<Subscription[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let filter = $state<"all" | "active" | "unsubscribed" | "ignored">("all");
  let searchQuery = $state("");
  let scanning = $state(false);
  let scanProgress = $state("");
  let unsubscribeTarget = $state<Subscription | null>(null);

  type SortKey = "sender" | "count" | "frequency" | "lastseen" | "method" | "status";
  let sortKey = $state<SortKey>("count");
  let sortAsc = $state(false);

  function toggleSort(key: SortKey) {
    if (sortKey === key) {
      sortAsc = !sortAsc;
    } else {
      sortKey = key;
      sortAsc = true;
    }
  }

  async function loadSubscriptions() {
    loading = true;
    error = null;
    try {
      const result = await invoke<Subscription[]>("get_subscriptions", { accountId });
      subscriptions = result ?? [];
    } catch (e) {
      error = String(e);
      addToast(`Failed to load subscriptions: ${e}`, "error", 5000);
    } finally {
      loading = false;
    }
  }

  async function handleScan() {
    scanning = true;
    scanProgress = "";
    const unlisten = await listen<string>("scan-progress", (event) => {
      scanProgress = event.payload;
    });
    try {
      const result = await invoke<{ messages_scanned: number; subscriptions_found: number; enriched: number }>("scan_subscriptions", { accountId });
      let msg = `Scanned ${result.messages_scanned} messages, found ${result.subscriptions_found} subscriptions`;
      if (result.enriched > 0) msg += `, enriched ${result.enriched} with unsubscribe methods`;
      addToast(msg, "success", 4000);
      await loadSubscriptions();
    } catch (e) {
      addToast(`Scan failed: ${e}`, "error", 5000);
    } finally {
      unlisten();
      scanning = false;
      scanProgress = "";
    }
  }

  function showUnsubscribeDialog(sub: Subscription) {
    unsubscribeTarget = sub;
  }

  function getUnsubMethod(sub: Subscription): "one_click" | "link" | "email" | "none" {
    if (sub.supports_one_click && sub.unsubscribe_url) return "one_click";
    if (sub.unsubscribe_url) return "link";
    if (sub.unsubscribe_mailto) return "email";
    return "none";
  }

  async function handleUnsubscribeResult() {
    await loadSubscriptions();
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

  function formatRelativeTime(value: number | string | null): string {
    if (value === null || value === 0) return "Never";
    const date = new Date(value);
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

  const methodOrder: Record<string, number> = { one_click: 0, link: 1, email: 2, none: 3 };

  let filteredSubscriptions = $derived(() => {
    let result = subscriptions;

    if (filter !== "all") {
      result = result.filter(s => s.status === filter);
    }

    if (searchQuery) {
      const q = searchQuery.toLowerCase();
      result = result.filter(s =>
        (s.sender_name || "").toLowerCase().includes(q) ||
        s.sender_email.toLowerCase().includes(q)
      );
    }

    if (sortKey) {
      const dir = sortAsc ? 1 : -1;
      result = [...result].sort((a, b) => {
        let cmp = 0;
        switch (sortKey) {
          case "sender":
            cmp = (a.sender_name || a.sender_email).localeCompare(b.sender_name || b.sender_email);
            break;
          case "count":
            cmp = a.message_count - b.message_count;
            break;
          case "frequency":
            cmp = (a.avg_frequency_days ?? Infinity) - (b.avg_frequency_days ?? Infinity);
            break;
          case "lastseen":
            cmp = (a.last_seen || 0) - (b.last_seen || 0);
            break;
          case "method":
            cmp = (methodOrder[getUnsubMethod(a)] ?? 9) - (methodOrder[getUnsubMethod(b)] ?? 9);
            break;
          case "status":
            cmp = a.status.localeCompare(b.status);
            break;
        }
        return cmp * dir;
      });
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
      {scanning ? (scanProgress || "Scanning...") : "Scan"}
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
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="col-sender sortable" class:sorted={sortKey === "sender"} onclick={() => toggleSort("sender")}>Sender {sortKey === "sender" ? (sortAsc ? "\u25B2" : "\u25BC") : ""}</div>
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="col-count sortable" class:sorted={sortKey === "count"} onclick={() => toggleSort("count")}>Messages {sortKey === "count" ? (sortAsc ? "\u25B2" : "\u25BC") : ""}</div>
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="col-frequency sortable" class:sorted={sortKey === "frequency"} onclick={() => toggleSort("frequency")}>Frequency {sortKey === "frequency" ? (sortAsc ? "\u25B2" : "\u25BC") : ""}</div>
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="col-lastseen sortable" class:sorted={sortKey === "lastseen"} onclick={() => toggleSort("lastseen")}>Last Seen {sortKey === "lastseen" ? (sortAsc ? "\u25B2" : "\u25BC") : ""}</div>
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="col-method sortable" class:sorted={sortKey === "method"} onclick={() => toggleSort("method")}>Method {sortKey === "method" ? (sortAsc ? "\u25B2" : "\u25BC") : ""}</div>
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="col-status sortable" class:sorted={sortKey === "status"} onclick={() => toggleSort("status")}>Status {sortKey === "status" ? (sortAsc ? "\u25B2" : "\u25BC") : ""}</div>
          <div class="col-actions"></div>
        </div>
        {#each filteredSubscriptions() as sub}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="table-row row-clickable" onclick={() => onselectsubscription?.(sub.sender_email)}>
            <div class="col-sender">
              <div class="sender-info">
                <span class="sender-name">{sub.sender_name || "Unknown"}</span>
                <span class="sender-email">{sub.sender_email}</span>
              </div>
            </div>
            <div class="col-count">{sub.message_count}</div>
            <div class="col-frequency">{formatFrequency(sub.avg_frequency_days)}</div>
            <div class="col-lastseen">{formatRelativeTime(sub.last_seen)}</div>
            <div class="col-method" title="Detected via {sub.detection_method}">
              <span class="method-pill method-pill-{getUnsubMethod(sub)}">
                {#if getUnsubMethod(sub) === "one_click"}One-Click{:else if getUnsubMethod(sub) === "link"}Link{:else if getUnsubMethod(sub) === "email"}Email{:else}Manual{/if}
              </span>
            </div>
            <div class="col-status">
              <span class="status-badge status-{sub.status}">{sub.status}</span>
            </div>
            <div class="col-actions" onclick={(e) => e.stopPropagation()}>
              {#if sub.status === "active"}
                <button class="action-btn unsubscribe-btn" title="Unsubscribe" onclick={() => showUnsubscribeDialog(sub)}>{@html iconBellOff}</button>
              {/if}
              <button class="action-btn correct-btn" title="Not a subscription" onclick={() => handleCorrectSubscription(sub)}>{@html iconXCircle}</button>
              <button class="action-btn delete-btn" title="Delete" onclick={() => handleDelete(sub)}>{@html iconTrash}</button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<UnsubscribeDialog
  subscription={unsubscribeTarget}
  onresult={handleUnsubscribeResult}
  ondismiss={() => unsubscribeTarget = null}
/>

<style>
  .subscriptions-panel {
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
    font-size: 17px;
    font-weight: 400;
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
    gap: 1px;
    background: rgba(0, 0, 0, 0.06);
    border-radius: 7px;
    padding: 2px;
  }

  :global([data-theme="dark"]) .filter-tabs {
    background: rgba(255, 255, 255, 0.1);
  }

  .filter-tab {
    padding: 4px 12px;
    background: transparent;
    border: none;
    border-radius: 5px;
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 400;
    cursor: pointer;
    font-family: var(--font-family);
    transition: all 0.2s;
  }

  .filter-tab:hover {
    color: var(--text-primary);
  }

  .filter-tab.active {
    background: var(--bg-view);
    color: var(--text-primary);
    font-weight: 500;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12), 0 0 0 0.5px rgba(0, 0, 0, 0.04);
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
    font-weight: 400;
    color: var(--text-secondary);
  }

  .sortable {
    cursor: pointer;
    user-select: none;
    transition: color 0.15s;
  }

  .sortable:hover {
    color: var(--text-primary);
  }

  .sortable.sorted {
    color: var(--text-primary);
    font-weight: 500;
  }

  .table-row {
    display: flex;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border-color);
    align-items: center;
    transition: background 0.1s;
  }

  .table-row:nth-child(even) {
    background: rgba(0, 0, 0, 0.02);
  }

  :global([data-theme="dark"]) .table-row:nth-child(even) {
    background: rgba(255, 255, 255, 0.02);
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
    flex: 1;
    min-width: 105px;
  }

  .col-actions {
    flex: 0.8;
    display: flex;
    gap: 4px;
    min-width: 100px;
    justify-content: flex-end;
  }

  .row-clickable {
    cursor: pointer;
  }

  .row-clickable:hover .sender-name {
    color: var(--accent-blue);
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
    background: rgba(255, 159, 10, 0.15);
    color: #c77c00;
  }

  :global([data-theme="dark"]) .status-ignored {
    color: #ff9f0a;
  }

  .method-pill {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 500;
  }

  .method-pill-one_click {
    background: rgba(52, 199, 89, 0.12);
    color: #34c759;
  }

  .method-pill-link {
    background: rgba(0, 122, 255, 0.12);
    color: #007aff;
  }

  .method-pill-email {
    background: rgba(255, 159, 10, 0.12);
    color: #c77c00;
  }

  :global([data-theme="dark"]) .method-pill-email {
    color: #ff9f0a;
  }

  .method-pill-none {
    background: rgba(142, 142, 147, 0.12);
    color: #8e8e93;
  }

  .action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    color: var(--text-secondary);
    transition: all 0.15s;
  }

  .action-btn:hover {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }

  .unsubscribe-btn {
    color: #34c759;
  }

  .unsubscribe-btn:hover {
    background: rgba(52, 199, 89, 0.1);
    color: #34c759;
  }

  .correct-btn {
    color: var(--text-secondary);
  }

  .delete-btn {
    color: var(--text-secondary);
  }

  .delete-btn:hover {
    background: rgba(255, 69, 58, 0.1);
    color: #ff453a;
  }

  /* Medium: hide less important columns, tighten spacing */
  @container (max-width: 800px) {
    .subscriptions-header {
      padding: 12px 16px;
    }

    .filter-bar {
      padding: 10px 16px;
    }

    .table-header {
      padding: 8px 16px;
    }

    .table-row {
      padding: 10px 16px;
    }

    .col-frequency,
    .col-method {
      display: none;
    }

    .col-sender {
      min-width: 140px;
    }

    .col-count,
    .col-lastseen {
      min-width: 70px;
    }

    .col-status {
      min-width: 70px;
    }

    .col-actions {
      min-width: 90px;
    }
  }

  /* Narrow: card layout */
  @container (max-width: 550px) {
    .subscriptions-header {
      padding: 12px;
    }

    .header-title h2 {
      font-size: 15px;
    }

    .filter-bar {
      flex-direction: column;
      align-items: stretch;
      padding: 10px 12px;
      gap: 8px;
    }

    .filter-tabs {
      flex-wrap: wrap;
    }

    .search-input-wrapper {
      max-width: none;
      min-width: 0;
    }

    .table-header {
      display: none;
    }

    .table-row {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 6px 12px;
      padding: 12px;
      margin: 8px 12px;
      border-radius: 8px;
      border: 1px solid var(--border-color);
    }

    .col-sender {
      grid-column: 1 / -1;
      min-width: 0;
    }

    .col-count,
    .col-lastseen,
    .col-status {
      display: flex;
      flex-direction: column;
      gap: 2px;
      min-width: 0;
      font-size: 12px;
    }

    .col-count::before,
    .col-lastseen::before,
    .col-status::before {
      font-size: 10px;
      font-weight: 400;
      color: var(--text-secondary);
    }

    .col-count::before { content: "Messages"; }
    .col-lastseen::before { content: "Last Seen"; }
    .col-status::before { content: "Status"; }

    .col-frequency,
    .col-method {
      display: none;
    }

    .col-actions {
      grid-column: 1 / -1;
      min-width: 0;
      padding-top: 6px;
      border-top: 1px solid var(--border-color);
      margin-top: 4px;
    }
  }
</style>
