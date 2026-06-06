<script lang="ts">
  import { fly, fade } from "svelte/transition";

  interface Props {
    senderEmail: string;
    senderName?: string;
    onroute: (routing: 'inbox' | 'feed' | 'auto_archive' | 'blocked') => void;
    onclose: () => void;
  }

  let {
    senderEmail,
    senderName,
    onroute,
    onclose,
  }: Props = $props();

  const displayName = senderName || senderEmail;

  const options = [
    {
      id: 'inbox' as const,
      label: 'Inbox',
      description: 'Deliver normally to your inbox',
      icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><polyline points="22 12 16 12 14 15 10 15 8 12 2 12"/><path d="M5.45 5.11L2 12v6a2 2 0 002 2h16a2 2 0 002-2v-6l-3.45-6.89A2 2 0 0016.76 4H7.24a2 2 0 00-1.79 1.11z"/></svg>`,
      color: '#007AFF',
    },
    {
      id: 'feed' as const,
      label: 'Feed',
      description: 'Send newsletters to the reading feed',
      icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M4 11a9 9 0 019 9"/><path d="M4 4a16 16 0 0116 16"/><circle cx="5" cy="19" r="1"/></svg>`,
      color: '#AF52DE',
    },
    {
      id: 'auto_archive' as const,
      label: 'Auto-archive',
      description: 'Skip inbox, quietly archive',
      icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><polyline points="21 8 21 21 3 21 3 8"/><rect x="1" y="3" width="22" height="5"/><line x1="10" y1="12" x2="14" y2="12"/></svg>`,
      color: '#FF9500',
    },
    {
      id: 'blocked' as const,
      label: 'Block',
      description: 'Never deliver emails from this sender',
      icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><circle cx="12" cy="12" r="10"/><line x1="4.93" y1="4.93" x2="19.07" y2="19.07"/></svg>`,
      color: '#FF3B30',
    },
  ];

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onclose();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="prompt-backdrop" onclick={onclose} onkeydown={handleKeydown} role="presentation">
  <div
    class="prompt-dialog"
    transition:fly={{ y: 20, duration: 250 }}
    role="dialog"
    aria-label="New sender routing"
  >
    <div class="prompt-header">
      <h2 class="prompt-title">New Sender</h2>
      <p class="prompt-email">{displayName}</p>
      <p class="prompt-hint">Choose where emails from this sender should go</p>
    </div>
    <div class="prompt-options">
      {#each options as opt}
        <button
          class="routing-option"
          onclick={() => onroute(opt.id)}
          style="--option-color: {opt.color};"
        >
          <span class="option-icon" style="color: {opt.color};">{@html opt.icon}</span>
          <div class="option-text">
            <span class="option-label">{opt.label}</span>
            <span class="option-desc">{opt.description}</span>
          </div>
        </button>
      {/each}
    </div>
    <button class="prompt-skip" onclick={onclose}>
      Decide later — deliver to inbox
    </button>
  </div>
</div>

<style>
  .prompt-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.3);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 300;
  }
  .prompt-dialog {
    background: var(--bg-view);
    border-radius: 12px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.12);
    width: 400px;
    max-width: 90vw;
    max-height: 80vh;
    overflow-y: auto;
    padding: 24px;
  }
  :global([data-theme="dark"]) .prompt-dialog {
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  }
  .prompt-header {
    text-align: center;
    margin-bottom: 20px;
  }
  .prompt-title {
    font-size: 18px;
    font-weight: 600;
    margin: 0 0 4px;
    color: var(--text-primary);
  }
  .prompt-email {
    font-size: 14px;
    color: var(--text-secondary);
    margin: 0 0 8px;
    font-family: monospace;
  }
  .prompt-hint {
    font-size: 12px;
    color: var(--text-secondary);
    margin: 0;
  }
  .prompt-options {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 12px;
  }
  .routing-option {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 12px;
    border: 1px solid var(--border-color);
    border-radius: 8px;
    background: var(--bg-primary);
    cursor: pointer;
    text-align: left;
    color: var(--text-primary);
    font-family: inherit;
    transition: border-color 0.15s, background 0.15s;
  }
  .routing-option:hover {
    border-color: var(--option-color);
    background: color-mix(in srgb, var(--option-color) 5%, var(--bg-primary));
  }
  .routing-option:focus-visible {
    outline: 2px solid var(--accent-blue);
    outline-offset: 2px;
  }
  .option-icon {
    flex-shrink: 0;
    display: flex;
    align-items: center;
  }
  .option-text {
    display: flex;
    flex-direction: column;
  }
  .option-label {
    font-size: 14px;
    font-weight: 500;
  }
  .option-desc {
    font-size: 12px;
    color: var(--text-secondary);
  }
  .prompt-skip {
    display: block;
    width: 100%;
    padding: 8px;
    border: none;
    background: none;
    color: var(--text-secondary);
    font-size: 12px;
    cursor: pointer;
    border-radius: 6px;
    font-family: inherit;
  }
  .prompt-skip:hover {
    background: var(--sidebar-hover);
  }
</style>
