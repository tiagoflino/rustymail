<script lang="ts">
  import type { LinkAnalysis } from "$lib/utils/linkSafety";

  interface Props {
    url: string | null;
    analysis: LinkAnalysis | null;
    onconfirm: () => void;
    ondismiss: () => void;
  }

  let { url, analysis, onconfirm, ondismiss }: Props = $props();
</script>

{#if url}
  <div class="link-overlay" role="button" tabindex="-1" onclick={ondismiss} onkeydown={(e) => { if (e.key === 'Escape') ondismiss(); }}>
    <div class="link-dialog" role="dialog" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <div class="link-dialog-body">
        {#if analysis}
          <div class="link-shield link-shield-{analysis.risk}">
            <svg width="28" height="28" viewBox="0 0 24 24" fill="none">
              {#if analysis.risk === 'safe'}
                <path d="M12 2L3 7v5c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V7l-9-5zm-1 14.5l-3.5-3.5 1.41-1.41L11 13.67l5.09-5.09L17.5 10 11 16.5z" fill="currentColor"/>
              {:else if analysis.risk === 'caution'}
                <path d="M12 2L3 7v5c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V7l-9-5zm-1 5h2v6h-2V7zm0 8h2v2h-2v-2z" fill="currentColor"/>
              {:else}
                <path d="M12 2L3 7v5c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V7l-9-5zm3.5 12.09L14.09 15.5 12 13.42 9.91 15.5 8.5 14.09 10.59 12 8.5 9.91 9.91 8.5 12 10.59 14.09 8.5l1.41 1.41L13.42 12l2.08 2.09z" fill="currentColor"/>
              {/if}
            </svg>
          </div>
        {/if}
        <p class="link-dialog-title">Open this link?</p>
        <p class="link-dialog-subtitle">This link will open in your default browser.</p>
        <div class="link-url-box">
          <span class="link-url-text">{url}</span>
        </div>
        {#if analysis && analysis.risk !== 'safe'}
          <div class="link-warning link-warning-{analysis.risk}">
            {#each analysis.reasons as reason}
              <p class="link-warning-line">{reason}</p>
            {/each}
          </div>
        {/if}
      </div>
      <div class="link-dialog-actions">
        <button class="link-action link-action-cancel" onclick={ondismiss}>Cancel</button>
        <button class="link-action link-action-open link-action-{analysis?.risk ?? 'safe'}" onclick={onconfirm}>
          {analysis?.risk === 'danger' ? 'Open Anyway' : 'Open Link'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .link-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 9999;
    backdrop-filter: blur(4px);
    -webkit-backdrop-filter: blur(4px);
  }
  .link-dialog {
    background: var(--bg-view);
    border-radius: 14px;
    width: 280px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.25), 0 0 0 0.5px rgba(0, 0, 0, 0.1);
    overflow: hidden;
  }
  .link-dialog-body {
    padding: 20px 20px 16px;
    text-align: center;
  }
  .link-shield {
    margin: 0 auto 12px;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .link-shield-safe { background: rgba(52, 199, 89, 0.12); color: #34c759; }
  .link-shield-caution { background: rgba(255, 159, 10, 0.12); color: #ff9f0a; }
  .link-shield-danger { background: rgba(255, 59, 48, 0.12); color: #ff3b30; }
  .link-dialog-title {
    margin: 0 0 4px;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    letter-spacing: -0.1px;
  }
  .link-dialog-subtitle {
    margin: 0 0 12px;
    font-size: 12px;
    line-height: 15px;
    color: var(--text-secondary);
  }
  .link-url-box {
    background: var(--bg-sidebar, rgba(0, 0, 0, 0.04));
    border-radius: 8px;
    padding: 8px 10px;
    margin-bottom: 12px;
  }
  .link-url-text {
    font-size: 11px;
    color: var(--text-secondary);
    word-break: break-all;
    line-height: 14px;
    display: block;
    text-align: left;
    max-height: 60px;
    overflow-y: auto;
  }
  .link-warning {
    border-radius: 8px;
    padding: 8px 10px;
    margin-bottom: 4px;
    text-align: left;
  }
  .link-warning-caution { background: rgba(255, 159, 10, 0.08); }
  .link-warning-danger { background: rgba(255, 59, 48, 0.08); }
  .link-warning-line {
    margin: 0;
    font-size: 11px;
    line-height: 14px;
  }
  .link-warning-caution .link-warning-line { color: #c87e00; }
  .link-warning-danger .link-warning-line { color: #ff3b30; }
  .link-dialog-actions {
    display: flex;
    border-top: 0.5px solid var(--border-color, rgba(0, 0, 0, 0.12));
  }
  .link-action {
    flex: 1;
    padding: 11px 8px;
    background: none;
    border: none;
    font-size: 14px;
    cursor: pointer;
    color: #007aff;
    transition: background 0.1s;
  }
  .link-action:hover {
    background: rgba(0, 122, 255, 0.06);
  }
  .link-action:active {
    background: rgba(0, 122, 255, 0.12);
  }
  .link-action-cancel {
    border-right: 0.5px solid var(--border-color, rgba(0, 0, 0, 0.12));
    color: var(--text-primary);
  }
  .link-action-cancel:hover {
    background: rgba(128, 128, 128, 0.08);
  }
  .link-action-open {
    font-weight: 600;
  }
  .link-action-danger {
    color: #ff3b30;
  }
  .link-action-danger:hover {
    background: rgba(255, 59, 48, 0.06);
  }
</style>
