<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  interface SenderRoutingInfo {
    sender_email: string;
    sender_name: string | null;
    routing: string;
    created_at: number;
    updated_at: number;
  }

  const ROUTING_OPTIONS: { id: string; label: string }[] = [
    { id: "inbox", label: "Inbox" },
    { id: "feed", label: "Feed" },
    { id: "auto_archive", label: "Auto-archive" },
    { id: "blocked", label: "Block" },
  ];

  let routings = $state<SenderRoutingInfo[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      routings = await invoke<SenderRoutingInfo[]>("get_all_sender_routings");
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function changeRouting(item: SenderRoutingInfo, routing: string) {
    try {
      await invoke("set_sender_routing", {
        senderEmail: item.sender_email,
        senderName: item.sender_name,
        routing,
      });
      item.routing = routing;
    } catch (e) {
      error = String(e);
    }
  }

  async function remove(item: SenderRoutingInfo) {
    try {
      await invoke("delete_sender_routing", { senderEmail: item.sender_email });
      routings = routings.filter((r) => r.sender_email !== item.sender_email);
    } catch (e) {
      error = String(e);
    }
  }

  onMount(load);
</script>

<div class="routing-manager">
  {#if loading}
    <p class="routing-state">Loading routing decisions…</p>
  {:else if error}
    <p class="routing-state routing-error">Failed to load: {error}</p>
  {:else if routings.length === 0}
    <p class="routing-state">No routing decisions yet. They appear here once you sort a new sender.</p>
  {:else}
    <ul class="routing-list">
      {#each routings as item (item.sender_email)}
        <li class="routing-item">
          <div class="routing-sender">
            <span class="routing-name">{item.sender_name || item.sender_email}</span>
            {#if item.sender_name}
              <span class="routing-email">{item.sender_email}</span>
            {/if}
          </div>
          <div class="routing-controls">
            <select
              class="routing-select"
              value={item.routing}
              onchange={(e) => changeRouting(item, e.currentTarget.value)}
              aria-label="Routing for {item.sender_email}"
            >
              {#each ROUTING_OPTIONS as opt}
                <option value={opt.id}>{opt.label}</option>
              {/each}
            </select>
            <button class="routing-remove" onclick={() => remove(item)} aria-label="Remove routing for {item.sender_email}">
              Remove
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .routing-manager {
    margin-top: 12px;
  }
  .routing-state {
    font-size: 13px;
    color: var(--text-secondary);
    margin: 8px 0;
  }
  .routing-error {
    color: var(--accent-red, #ff3b30);
  }
  .routing-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .routing-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--border-color);
    border-radius: 8px;
    background: var(--bg-primary);
  }
  .routing-sender {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .routing-name {
    font-size: 14px;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .routing-email {
    font-size: 12px;
    color: var(--text-secondary);
    font-family: monospace;
  }
  .routing-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  .routing-select {
    padding: 4px 8px;
    border-radius: 6px;
    border: 1px solid var(--border-color);
    background: var(--bg-view);
    color: var(--text-primary);
    font-family: inherit;
    font-size: 13px;
  }
  .routing-remove {
    padding: 4px 10px;
    border-radius: 6px;
    border: none;
    background: none;
    color: var(--text-secondary);
    font-size: 12px;
    cursor: pointer;
  }
  .routing-remove:hover {
    background: var(--sidebar-hover);
    color: var(--accent-red, #ff3b30);
  }
</style>
