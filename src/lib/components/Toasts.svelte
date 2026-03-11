<script lang="ts">
  import { toasts, removeToast } from '$lib/stores/toast';
  import { fly, fade } from 'svelte/transition';
  import { flip } from 'svelte/animate';

  const iconInfo = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>`;
  const iconSuccess = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>`;
  const iconError = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/></svg>`;
  const iconClose = `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>`;
</script>

<div class="toast-container">
  {#each $toasts as toast (toast.id)}
    <div 
      class="toast toast-{toast.type}"
      in:fly={{ y: -20, duration: 250 }}
      out:fade={{ duration: 150 }}
      animate:flip={{ duration: 250 }}
    >
      <div class="toast-icon">
        {#if toast.type === 'success'}
          {@html iconSuccess}
        {:else if toast.type === 'error'}
          {@html iconError}
        {:else}
          {@html iconInfo}
        {/if}
      </div>
      <div class="toast-message">
        {toast.message}
      </div>
      {#if toast.actionLabel && toast.onAction}
        <button class="toast-action" onclick={() => { toast.onAction?.(); removeToast(toast.id); }}>
          {toast.actionLabel}
        </button>
      {/if}
      <button class="toast-close" onclick={() => removeToast(toast.id)}>
        {@html iconClose}
      </button>
    </div>
  {/each}
</div>

<style>
  .toast-container {
    position: fixed;
    top: 36px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 10000;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: center;
    background: rgba(40, 40, 40, 0.78);
    backdrop-filter: blur(20px) saturate(180%);
    -webkit-backdrop-filter: blur(20px) saturate(180%);
    color: #f5f5f7;
    font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", sans-serif;
    padding: 10px 16px;
    border-radius: 12px;
    box-shadow: 0 4px 24px rgba(0, 0, 0, 0.2), 0 0 0 0.5px rgba(255, 255, 255, 0.08) inset;
    min-width: 240px;
    max-width: 420px;
    pointer-events: auto;
  }

  .toast-icon {
    display: flex;
    margin-right: 10px;
    flex-shrink: 0;
  }

  .toast-success .toast-icon { color: #30d158; }
  .toast-error .toast-icon { color: #ff453a; }
  .toast-info .toast-icon { color: #0a84ff; }

  .toast-message {
    flex: 1;
    font-size: 13px;
    font-weight: 500;
    line-height: 1.35;
    letter-spacing: -0.08px;
  }

  .toast-action {
    background: rgba(255, 255, 255, 0.12);
    border: none;
    color: #0a84ff;
    font-size: 12px;
    font-weight: 600;
    margin-left: 12px;
    padding: 5px 12px;
    cursor: pointer;
    border-radius: 6px;
    transition: background 0.15s;
    white-space: nowrap;
    font-family: inherit;
  }

  .toast-action:hover {
    background: rgba(255, 255, 255, 0.18);
  }

  .toast-close {
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.45);
    margin-left: 8px;
    padding: 4px;
    cursor: pointer;
    display: flex;
    align-items: center;
    border-radius: 6px;
    transition: background 0.15s, color 0.15s;
  }

  .toast-close:hover {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.8);
  }
</style>
