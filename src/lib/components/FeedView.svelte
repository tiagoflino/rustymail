<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { addToast } from "$lib/stores/toast";
  import { selectedThreadId } from "$lib/stores/messages";
  import type { LocalThread } from "$lib/stores/threads";

  interface Props {
    isMacOS?: boolean;
    onselectthread?: (threadId: string) => void;
  }

  let { isMacOS = false, onselectthread }: Props = $props();

  let threads = $state<LocalThread[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  type Grouped = { sender: string; threads: LocalThread[] };
  let grouped = $derived.by<Grouped[]>(() => {
    const map = new Map<string, LocalThread[]>();
    for (const t of threads) {
      const list = map.get(t.sender) ?? [];
      list.push(t);
      map.set(t.sender, list);
    }
    const result: Grouped[] = [];
    for (const [sender, ts] of map) {
      ts.sort((a, b) => b.internal_date - a.internal_date);
      result.push({ sender, threads: ts });
    }
    result.sort((a, b) => b.threads[0].internal_date - a.threads[0].internal_date);
    return result;
  });

  async function loadFeed() {
    loading = true;
    error = null;
    try {
      threads = await invoke<LocalThread[]>("get_feed_threads", { offset: 0, limit: 100 });
    } catch (e) {
      error = String(e);
      addToast(`Failed to load feed: ${e}`, "error", 5000);
    } finally {
      loading = false;
    }
  }

  function handleSelectThread(threadId: string) {
    if ($selectedThreadId === threadId) {
      selectedThreadId.set(null);
    } else {
      onselectthread?.(threadId);
    }
  }

  async function handleUnsubscribe(sender: string) {
    try {
      const subs = await invoke<Array<{ id: number; sender_email: string }>>("get_subscriptions", { accountId: null, status: "active" });
      const sLower = sender.toLowerCase();
      const sub = subs.find((s) => {
        const se = s.sender_email.toLowerCase();
        return sLower.includes(se) || se.includes(sLower);
      });
      if (sub) {
        await invoke("unsubscribe", { subscriptionId: sub.id });
        addToast(`Unsubscribed from ${sender}`, "success", 4000);
        loadFeed();
      } else {
        addToast(`No active subscription found for ${sender}`, "info", 4000);
      }
    } catch (e) {
      addToast(`Failed to unsubscribe: ${e}`, "error", 5000);
    }
  }

  async function handleMarkAllRead(senderEmail: string) {
    const senderThreads = threads.filter((t) => t.sender === senderEmail && t.unread > 0);
    if (senderThreads.length === 0) return;
    try {
      const ids = senderThreads.map((t) => t.id);
      await invoke("batch_mark_read_status", { threadIds: ids, isRead: true });
      threads = threads.map((t) =>
        t.sender === senderEmail ? { ...t, unread: 0 } : t
      );
      addToast(`Marked ${ids.length} from ${senderEmail} as read`, "info", 3000);
    } catch (e) {
      addToast(`Failed to mark as read: ${e}`, "error", 5000);
    }
  }

  function formatDate(ts: number): string {
    const d = new Date(ts);
    const now = new Date();
    const diff = now.getTime() - d.getTime();
    if (diff < 86400000) {
      return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
    }
    if (diff < 604800000) {
      return d.toLocaleDateString(undefined, { weekday: "short" });
    }
    return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  }

  onMount(() => {
    loadFeed();
  });
</script>

<div class="feed-view">
  {#if loading}
    <div class="feed-center">
      <div class="loading-spinner"></div>
    </div>
  {:else if error}
    <div class="feed-center">
      <p class="feed-error">{error}</p>
      <button class="feed-retry" onclick={loadFeed}>Retry</button>
    </div>
  {:else if grouped.length === 0}
    <div class="feed-center">
      <div class="feed-empty-icon">{@render feedIcon()}</div>
      <h2 class="feed-empty-title">No newsletters yet</h2>
      <p class="feed-empty-text">
        When you receive newsletters they will appear here
      </p>
    </div>
  {:else}
    <div class="feed-scroll">
      {#each grouped as group}
        <div class="feed-group">
          <div class="feed-group-header">
            <h3 class="feed-sender-name">{group.sender}</h3>
            <div class="feed-group-actions">
              <button
                class="feed-action-btn"
                onclick={() => handleMarkAllRead(group.threads[0].sender)}
                title="Mark all as read"
              >
                {@render markReadIcon()}
              </button>
              <button
                class="feed-action-btn"
                onclick={() => handleUnsubscribe(group.threads[0].sender)}
                title="Unsubscribe"
              >
                {@render unsubscribeIcon()}
              </button>
            </div>
          </div>
          <div class="feed-cards">
            {#each group.threads as thread}
              <button
                class="feed-card"
                onclick={() => handleSelectThread(thread.id)}
              >
                <div class="feed-card-top">
                  {#if thread.unread > 0}
                    <span class="feed-unread-dot"></span>
                  {/if}
                  <span class="feed-card-subject" class:unread={thread.unread > 0}>{thread.subject}</span>
                  <span class="feed-card-date">{formatDate(thread.internal_date)}</span>
                </div>
                <p class="feed-card-snippet">{thread.snippet}</p>
              </button>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<svelte:window onkeydown={(e) => {
  if (e.key === "Escape" && !loading) {
  }
}} />

{#snippet feedIcon()}
  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
    <path d="M4 11a9 9 0 0 1 9 9" />
    <path d="M4 4a16 16 0 0 1 16 16" />
    <circle cx="5" cy="19" r="1" />
  </svg>
{/snippet}

{#snippet markReadIcon()}
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
    <polyline points="22 12 18 12 15 21 9 3 6 12 2 12" />
  </svg>
{/snippet}

{#snippet unsubscribeIcon()}
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
    <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
    <line x1="1" y1="1" x2="23" y2="23" />
  </svg>
{/snippet}

<style>
  .feed-view {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--bg-view);
  }

  .feed-center {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 40px 20px;
    text-align: center;
  }

  .feed-empty-icon {
    color: var(--text-tertiary);
    margin-bottom: 4px;
  }

  .feed-empty-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .feed-empty-text {
    font-size: 13px;
    color: var(--text-secondary);
    margin: 0;
    max-width: 240px;
    line-height: 18px;
  }

  .feed-error {
    color: #ff453a;
    font-size: 13px;
    margin: 0;
  }

  .feed-retry {
    padding: 6px 16px;
    border-radius: 6px;
    border: 1px solid var(--border-color);
    background: var(--bg-primary);
    color: var(--text-primary);
    cursor: pointer;
    font-size: 12px;
    font-family: inherit;
  }

  .feed-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 16px 20px;
  }

  .feed-group {
    margin-bottom: 20px;
  }

  .feed-group-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
    padding: 0 4px;
  }

  .feed-sender-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
    letter-spacing: -0.08px;
  }

  .feed-group-actions {
    display: flex;
    gap: 4px;
  }

  .feed-action-btn {
    width: 26px;
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-tertiary);
    cursor: pointer;
    transition: background 0.1s, color 0.1s;
  }

  .feed-action-btn:hover {
    background: var(--sidebar-hover);
    color: var(--text-secondary);
  }

  .feed-cards {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .feed-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    width: 100%;
    padding: 10px 14px;
    border-radius: 12px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    cursor: pointer;
    text-align: left;
    font-family: inherit;
    transition: background 0.1s;
  }

  .feed-card:hover {
    background: var(--sidebar-hover);
  }

  .feed-card-top {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
  }

  .feed-unread-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent-blue);
    flex-shrink: 0;
  }

  .feed-card-subject {
    font-size: 13px;
    font-weight: 400;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }

  .feed-card-subject.unread {
    font-weight: 600;
  }

  .feed-card-date {
    font-size: 11px;
    color: var(--text-tertiary);
    flex-shrink: 0;
  }

  .feed-card-snippet {
    font-size: 12px;
    color: var(--text-secondary);
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    width: 100%;
    line-height: 16px;
  }

  .loading-spinner {
    width: 20px;
    height: 20px;
    border: 2px solid var(--border-color);
    border-top-color: var(--accent-blue);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
