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
      in:fly={{ y: 20, duration: 250 }}
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
      <button class="toast-close" onclick={() => removeToast(toast.id)}>
        {@html iconClose}
      </button>
    </div>
  {/each}
</div>

<style>
  .toast-container {
    position: fixed;
    bottom: 24px;
    left: 24px;
    z-index: 10000;
    display: flex;
    flex-direction: column;
    gap: 8px;
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: center;
    background: var(--bg-view, #ffffff);
    color: var(--text-primary, #1c1c1e);
    border: 1px solid var(--border-color, #e0e0e2);
    font-family: var(--font-family, -apple-system, sans-serif);
    padding: 12px 16px;
    border-radius: 8px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.12);
    min-width: 280px;
    max-width: 400px;
    pointer-events: auto;
  }

  .toast-icon {
    display: flex;
    margin-right: 12px;
  }

  .toast-success .toast-icon { color: #34A853; }
  .toast-error .toast-icon { color: #EA4335; }
  .toast-info .toast-icon { color: #4285F4; }

  .toast-message {
    flex: 1;
    font-size: 13px;
    font-weight: 500;
    line-height: 1.4;
  }

  .toast-close {
    background: transparent;
    border: none;
    color: var(--text-secondary, #9aa0a6);
    margin-left: 12px;
    padding: 4px;
    cursor: pointer;
    display: flex;
    align-items: center;
    border-radius: 4px;
    transition: background 0.15s, color 0.15s;
  }

  .toast-close:hover {
    background: rgba(255,255,255,0.1);
    color: var(--text-primary, #ffffff);
  }
</style>
