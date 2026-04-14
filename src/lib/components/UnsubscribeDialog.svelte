<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";

  interface UnsubscribeTarget {
    id: number;
    sender_name: string | null;
    sender_email: string;
    unsubscribe_url: string | null;
    unsubscribe_mailto: string | null;
    supports_one_click: boolean;
  }

  interface Props {
    subscription: UnsubscribeTarget | null;
    ondismiss: () => void;
    onresult: () => void;
  }

  let { subscription, ondismiss, onresult }: Props = $props();

  let phase = $state<"initial" | "loading" | "success" | "error" | "confirming">("initial");
  let errorMessage = $state("");
  let autoCloseTimer: ReturnType<typeof setTimeout> | null = null;

  function scheduleAutoClose() {
    clearAutoClose();
    autoCloseTimer = setTimeout(() => { onresult(); ondismiss(); }, 1200);
  }

  function clearAutoClose() {
    if (autoCloseTimer) { clearTimeout(autoCloseTimer); autoCloseTimer = null; }
  }

  onDestroy(clearAutoClose);

  $effect(() => {
    if (subscription) {
      phase = "initial";
      errorMessage = "";
    } else {
      clearAutoClose();
    }
  });

  let method = $derived.by(() => {
    if (!subscription) return "none";
    if (subscription.supports_one_click && subscription.unsubscribe_url) return "one_click";
    if (subscription.unsubscribe_url) return "link";
    if (subscription.unsubscribe_mailto) return "email";
    return "none";
  });

  let senderDisplay = $derived(subscription?.sender_name || subscription?.sender_email || "");

  async function handleConfirm() {
    if (!subscription) return;

    if (method === "one_click") {
      phase = "loading";
      try {
        const result = await invoke<{ method: string; success: boolean; message: string }>(
          "unsubscribe", { subscriptionId: subscription.id }
        );
        if (result.success) {
          phase = "success";
          scheduleAutoClose();
        } else {
          errorMessage = result.message;
          phase = "error";
        }
      } catch (e) {
        errorMessage = String(e);
        phase = "error";
      }
    } else if (method === "link") {
      try {
        await invoke("unsubscribe", { subscriptionId: subscription.id });
      } catch {}
      phase = "confirming";
    } else if (method === "email" && subscription.unsubscribe_mailto) {
      window.location.href = `mailto:${subscription.unsubscribe_mailto}?subject=Unsubscribe`;
      phase = "confirming";
    }
  }

  async function handleMarkDone() {
    if (!subscription) return;
    try {
      await invoke("mark_unsubscribed", { subscriptionId: subscription.id });
      phase = "success";
      setTimeout(() => { onresult(); ondismiss(); }, 1200);
    } catch (e) {
      errorMessage = String(e);
      phase = "error";
    }
  }

  function handleDismiss() {
    if (phase === "loading") return;
    ondismiss();
  }
</script>

{#if subscription}
  <div class="unsub-overlay" role="button" tabindex="-1" onclick={handleDismiss} onkeydown={(e) => { if (e.key === 'Escape') handleDismiss(); }}>
    <div class="unsub-dialog" role="dialog" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <div class="unsub-dialog-body">
        {#if phase === "loading"}
          <div class="unsub-icon unsub-icon-one_click">
            <div class="unsub-spinner"></div>
          </div>
          <p class="unsub-dialog-title">Unsubscribing...</p>
          <p class="unsub-dialog-subtitle">Sending one-click unsubscribe request.</p>

        {:else if phase === "success"}
          <div class="unsub-icon unsub-icon-one_click">
            <svg width="28" height="28" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"/></svg>
          </div>
          <p class="unsub-dialog-title">Unsubscribed</p>
          <p class="unsub-dialog-subtitle">You won't receive emails from {senderDisplay} anymore.</p>

        {:else if phase === "error"}
          <div class="unsub-icon unsub-icon-none">
            <svg width="28" height="28" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2C6.47 2 2 6.47 2 12s4.47 10 10 10 10-4.47 10-10S17.53 2 12 2zm5 13.59L15.59 17 12 13.41 8.41 17 7 15.59 10.59 12 7 8.41 8.41 7 12 10.59 15.59 7 17 8.41 13.41 12 17 15.59z"/></svg>
          </div>
          <p class="unsub-dialog-title">Something went wrong</p>
          <p class="unsub-dialog-subtitle">{errorMessage}</p>

        {:else if phase === "confirming"}
          <div class="unsub-icon unsub-icon-link">
            <svg width="28" height="28" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/></svg>
          </div>
          <p class="unsub-dialog-title">Did you unsubscribe?</p>
          <p class="unsub-dialog-subtitle">
            {#if method === "email"}
              Confirm if you sent the unsubscribe email.
            {:else}
              Confirm if you completed the unsubscribe on the page.
            {/if}
          </p>

        {:else}
          <!-- initial phase -->
          <div class="unsub-icon unsub-icon-{method}">
            {#if method === "one_click"}
              <svg width="28" height="28" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"/></svg>
            {:else if method === "link"}
              <svg width="28" height="28" viewBox="0 0 24 24" fill="currentColor"><path d="M19 19H5V5h7V3H5c-1.11 0-2 .9-2 2v14c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2v-7h-2v7zM14 3v2h3.59l-9.83 9.83 1.41 1.41L19 6.41V10h2V3h-7z"/></svg>
            {:else if method === "email"}
              <svg width="28" height="28" viewBox="0 0 24 24" fill="currentColor"><path d="M20 4H4c-1.1 0-1.99.9-1.99 2L2 18c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm0 4l-8 5-8-5V6l8 5 8-5v2z"/></svg>
            {:else}
              <svg width="28" height="28" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2C6.47 2 2 6.47 2 12s4.47 10 10 10 10-4.47 10-10S17.53 2 12 2zm5 13.59L15.59 17 12 13.41 8.41 17 7 15.59 10.59 12 7 8.41 8.41 7 12 10.59 15.59 7 17 8.41 13.41 12 17 15.59z"/></svg>
            {/if}
          </div>
          <p class="unsub-dialog-title">
            {#if method === "none"}
              Can't unsubscribe from {senderDisplay}
            {:else}
              Unsubscribe from {senderDisplay}?
            {/if}
          </p>
          <p class="unsub-dialog-subtitle">
            {#if method === "one_click"}
              This will instantly unsubscribe you. No browser needed.
            {:else if method === "link"}
              This will open the unsubscribe page in your browser.
            {:else if method === "email"}
              Send an email to this address to unsubscribe.
            {:else}
              No automatic unsubscribe method was found for this sender.
            {/if}
          </p>
          {#if method === "link" && subscription.unsubscribe_url}
            <div class="unsub-url-box">
              <span class="unsub-url-text">{subscription.unsubscribe_url}</span>
            </div>
          {/if}
          {#if method === "email" && subscription.unsubscribe_mailto}
            <div class="unsub-url-box">
              <span class="unsub-url-text">{subscription.unsubscribe_mailto}</span>
            </div>
          {/if}
        {/if}
      </div>

      <div class="unsub-dialog-actions">
        {#if phase === "loading" || phase === "success"}
          <!-- no buttons during loading/success -->
        {:else if phase === "error"}
          <button class="unsub-action unsub-action-cancel" onclick={handleDismiss}>Close</button>
          <button class="unsub-action unsub-action-confirm unsub-action-one_click" onclick={() => { phase = "initial"; }}>Retry</button>
        {:else if phase === "confirming"}
          <button class="unsub-action unsub-action-cancel" onclick={handleDismiss}>No</button>
          <button class="unsub-action unsub-action-confirm unsub-action-one_click" onclick={handleMarkDone}>Yes</button>
        {:else if method === "none"}
          <button class="unsub-action" onclick={handleDismiss}>Close</button>
        {:else}
          <button class="unsub-action unsub-action-cancel" onclick={handleDismiss}>Cancel</button>
          <button class="unsub-action unsub-action-confirm unsub-action-{method}" onclick={handleConfirm}>
            {#if method === "one_click"}
              Unsubscribe
            {:else if method === "link"}
              Open Link
            {:else}
              Open in Mail
            {/if}
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .unsub-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 9999;
    backdrop-filter: blur(var(--blur-dialog));
    -webkit-backdrop-filter: blur(var(--blur-dialog));
  }

  .unsub-dialog {
    background: var(--bg-view);
    border-radius: var(--radius-modal);
    width: 300px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.25), 0 0 0 0.5px rgba(0, 0, 0, 0.1);
    overflow: hidden;
    animation: unsub-slideUp 0.15s ease;
  }

  @keyframes unsub-slideUp {
    from { opacity: 0; transform: translateY(8px) scale(0.98); }
    to { opacity: 1; transform: none; }
  }

  .unsub-dialog-body {
    padding: 20px 20px 16px;
    text-align: center;
  }

  .unsub-icon {
    margin: 0 auto 12px;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .unsub-icon-one_click { background: rgba(52, 199, 89, 0.12); color: #34c759; }
  .unsub-icon-link { background: rgba(0, 122, 255, 0.12); color: #007aff; }
  .unsub-icon-email { background: rgba(255, 159, 10, 0.12); color: #ff9f0a; }
  .unsub-icon-none { background: rgba(142, 142, 147, 0.12); color: #8e8e93; }

  .unsub-spinner {
    width: 24px;
    height: 24px;
    border: 2.5px solid rgba(52, 199, 89, 0.25);
    border-top-color: #34c759;
    border-radius: 50%;
    animation: unsub-spin 0.6s linear infinite;
  }

  @keyframes unsub-spin {
    to { transform: rotate(360deg); }
  }

  .unsub-dialog-title {
    margin: 0 0 4px;
    font-size: var(--font-size-detail);
    font-weight: 600;
    color: var(--text-primary);
  }

  .unsub-dialog-subtitle {
    margin: 0 0 12px;
    font-size: var(--font-size-toolbar);
    line-height: 16px;
    color: var(--text-secondary);
  }

  .unsub-url-box {
    background: var(--bg-sidebar, rgba(0, 0, 0, 0.04));
    border-radius: 8px;
    padding: 8px 10px;
    margin-bottom: 4px;
  }

  .unsub-url-text {
    font-size: var(--font-size-small);
    color: var(--text-secondary);
    word-break: break-all;
    line-height: 14px;
    display: block;
    text-align: left;
    max-height: 60px;
    overflow-y: auto;
  }

  .unsub-dialog-actions {
    display: flex;
    border-top: 0.5px solid var(--border-color, rgba(0, 0, 0, 0.12));
    min-height: 44px;
  }

  .unsub-dialog-actions:empty {
    border-top: none;
    min-height: 16px;
  }

  .unsub-action {
    flex: 1;
    padding: 11px 8px;
    background: none;
    border: none;
    font-size: var(--font-size-detail);
    cursor: pointer;
    color: #007aff;
    font-family: var(--font-family);
    transition: background 0.1s;
  }

  .unsub-action:hover {
    background: rgba(0, 122, 255, 0.06);
  }

  .unsub-action:active {
    background: rgba(0, 122, 255, 0.12);
  }

  .unsub-action-cancel {
    border-right: 0.5px solid var(--border-color, rgba(0, 0, 0, 0.12));
    color: var(--text-primary);
  }

  .unsub-action-cancel:hover {
    background: rgba(128, 128, 128, 0.08);
  }

  .unsub-action-confirm {
    font-weight: 600;
  }

  .unsub-action-one_click {
    color: #34c759;
  }

  .unsub-action-one_click:hover {
    background: rgba(52, 199, 89, 0.06);
  }

  .unsub-action-link {
    color: #007aff;
  }

  .unsub-action-email {
    color: #ff9f0a;
  }

  .unsub-action-email:hover {
    background: rgba(255, 159, 10, 0.06);
  }
</style>
