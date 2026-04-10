<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";

  interface Props {
    currentVersion: string;
    newVersion: string;
    releaseDate: string | null;
    releaseNotes: string | null;
    onClose: () => void;
    onInstall: () => void;
  }

  let {
    currentVersion,
    newVersion,
    releaseDate,
    releaseNotes,
    onClose,
    onInstall,
  }: Props = $props();

  let notesExpanded = $state(false);

  const formattedDate = $derived(
    releaseDate ? new Date(releaseDate).toLocaleDateString("en-US", {
      year: "numeric",
      month: "long",
      day: "numeric",
    }) : "Recently"
  );

  const notesRaw = $derived(releaseNotes ?? "");
  const hasNotes = $derived(notesRaw.trim().length > 0);

  async function goToReleases() {
    await openUrl("https://github.com/rectified64/rustymail/releases");
  }
</script>

<div class="alert-backdrop" onclick={onClose}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="alert-container" onclick={(e) => e.stopPropagation()}>
    <div class="alert-content">
      <div class="alert-icon">
        <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 48 48" fill="none">
          <rect width="48" height="48" rx="12" fill="url(#aGrad)"/>
          <path d="M14 16h20c1.66 0 3 1.34 3 3v10c0 1.66-1.34 3-3 3H14c-1.66 0-3-1.34-3-3v-10c0-1.66 1.34-3 3-3z" fill="white" fill-opacity="0.92"/>
          <path d="M24 19l6 4.5-6 4.5-6-4.5 6-4.5z" fill="#0A84FF"/>
          <path d="M19 26h10" stroke="white" stroke-opacity="0.7" stroke-width="1.5" stroke-linecap="round"/>
          <defs>
            <linearGradient id="aGrad" x1="0" y1="0" x2="48" y2="48">
              <stop stop-color="#0A84FF"/>
              <stop offset="1" stop-color="#5E5CE6"/>
            </linearGradient>
          </defs>
        </svg>
      </div>

      <div class="alert-body">
        <h2 class="alert-title">Update Available</h2>
        
        <p class="alert-message">
          New version available ({newVersion}), released on {formattedDate}.
          Your current version: {currentVersion}.
          {#if formattedDate !== "Recently"}
          {/if}
        </p>

        {#if hasNotes}
          <button 
            class="notes-toggle" 
            onclick={() => notesExpanded = !notesExpanded}
          >
            <span>Release Notes</span>
            <svg 
              xmlns="http://www.w3.org/2000/svg" 
              width="10" 
              height="10" 
              viewBox="0 0 24 24" 
              fill="none" 
              stroke="currentColor" 
              stroke-width="2.5"
              class:rotated={notesExpanded}
            >
              <polyline points="6 9 12 15 18 9"></polyline>
            </svg>
          </button>
          
          {#if notesExpanded}
            <div class="notes-content">
              {#each notesRaw.split("\n") as line}
                {#if line.startsWith("## ")}
                  <h4>{line.slice(3)}</h4>
                {:else if line.startsWith("- ")}
                  <li>{line.slice(2)}</li>
                {:else if line.trim()}
                  <p>{line}</p>
                {/if}
              {/each}
            </div>
          {/if}
        {/if}
      </div>
    </div>

    <div class="alert-actions">
      <button class="btn-learn" onclick={goToReleases}>Learn More</button>
<div class="action-group">
        <button class="btn-cancel" onclick={onClose}>Not Now</button>
        <button class="btn-default" onclick={onInstall}>Install & Restart</button>
      </div>
    </div>
  </div>
</div>

<style>
  .alert-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    padding: 20px;
  }

  .alert-container {
    background: var(--bg-view);
    width: 380px;
    max-width: 100%;
    border-radius: var(--radius-modal);
    box-shadow:
      0 20px 40px rgba(0, 0, 0, 0.2),
      0 0 0 0.5px rgba(0, 0, 0, 0.1);
    overflow: hidden;
    color: var(--text-primary);
    font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", sans-serif;
  }

  .alert-content {
    display: flex;
    gap: 14px;
    padding: 20px 20px 16px;
  }

  .alert-icon {
    flex-shrink: 0;
    width: 48px;
    height: 48px;
  }

  .alert-icon svg {
    width: 100%;
    height: 100%;
  }

  .alert-body {
    flex: 1;
    min-width: 0;
  }

  .alert-title {
    font-size: 13px;
    font-weight: 600;
    margin: 0 0 4px;
    line-height: 1.3;
  }

  .alert-message {
    font-size: 12px;
    color: var(--text-secondary);
    margin: 0;
    line-height: 1.4;
  }

  .notes-toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-top: 10px;
    padding: 0;
    background: none;
    border: none;
    font-size: 12px;
    font-weight: 400;
    color: var(--accent-blue);
    cursor: pointer;
  }

  .notes-toggle:hover {
    text-decoration: underline;
  }

  .notes-toggle svg {
    color: var(--text-secondary);
    transition: transform 0.2s ease;
  }

  .notes-toggle svg.rotated {
    transform: rotate(180deg);
  }

  .notes-content {
    margin-top: 8px;
    padding: 10px 12px;
    background: var(--bg-sidebar);
    border-radius: 6px;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-secondary);
    max-height: 150px;
    overflow-y: auto;
  }

  .notes-content h4 {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 8px 0 4px;
  }

  .notes-content h4:first-child {
    margin-top: 0;
  }

  .notes-content p {
    margin: 0 0 4px;
  }

  .notes-content li {
    margin-left: 12px;
    margin-bottom: 2px;
  }

  .alert-actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 20px 14px;
    background: var(--bg-sidebar);
    border-top: 1px solid var(--border-color);
  }

  .btn-learn {
    padding: 6px 10px;
    font-size: 12px;
    font-weight: 400;
    color: var(--accent-blue);
    background: transparent;
    border: none;
    cursor: pointer;
    border-radius: 5px;
  }

  .btn-learn:hover {
    background: var(--sidebar-hover);
  }

  .action-group {
    display: flex;
    gap: 8px;
  }

  .btn-cancel,
  .btn-default {
    padding: 7px 14px;
    font-size: 12px;
    font-weight: 400;
    border-radius: 5px;
    cursor: pointer;
    transition: background 0.1s ease;
  }

  .btn-cancel {
    background: transparent;
    border: none;
    color: var(--text-secondary);
  }

  .btn-cancel:hover {
    color: var(--text-primary);
    background: var(--sidebar-hover);
  }

  .btn-default {
    background: var(--accent-blue);
    color: white;
    border: none;
  }

  .btn-default:hover {
    opacity: 0.92;
  }

  @media (max-width: 420px) {
    .alert-backdrop {
      padding: 16px;
    }

    .alert-container {
      width: 100%;
    }

    .alert-content {
      padding: 16px;
    }

    .alert-actions {
      padding: 10px 16px 12px;
      flex-wrap: wrap;
      gap: 8px;
    }

    .btn-learn {
      order: 1;
      width: 100%;
      text-align: center;
      padding: 8px;
    }

    .action-group {
      order: 2;
      width: 100%;
      justify-content: flex-end;
    }
  }
</style>