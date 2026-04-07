<script lang="ts">
  import type { Update } from "@tauri-apps/plugin-updater";

  interface Props {
    update: Update;
    onClose: () => void;
    onInstall: () => void;
  }
  let { update, onClose, onInstall }: Props = $props();

  let formattedNotes = $derived((() => {
    if (!update.body) return "No release notes provided.";
    // Simple rendering for plain text / basic markdown
    // Convert newlines to breaks
    return update.body.replace(/\n/g, "<br>");
  })());

  const iconUpdate = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12A9 9 0 0 0 6 5.3L3 8"/><path d="M21 3v5h-5"/><path d="M3 12a9 9 0 0 0 15 6.7l3-2.7"/><path d="M3 21v-5h5"/></svg>`;
</script>

<div class="modal-backdrop" onclick={onClose}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="modal-content" onclick={(e) => e.stopPropagation()}>
    <div class="modal-header">
      <div class="header-title">
        <span class="icon">{@html iconUpdate}</span>
        <h2>Update Available</h2>
      </div>
      <button class="close-btn" onclick={onClose}>✕</button>
    </div>

    <div class="modal-body read-view">
      <h1 class="read-title">Rustymail {update.version}</h1>
      <div class="read-time">
         Release Date: {update.date ? new Date(update.date).toLocaleDateString() : "Just now"}
      </div>

      <div class="read-desc section-block">
         <div class="notes-header">Release Notes</div>
         <div class="html-content text-notes">
           {@html formattedNotes}
         </div>
      </div>
    </div>

    <div class="modal-footer">
      <div class="spacer"></div>
      <button class="btn-cancel" onclick={onClose}>Later</button>
      <button class="btn-save" onclick={onInstall}>Install and Relaunch</button>
    </div>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .modal-content {
    background: var(--bg-view, #ffffff);
    width: 520px;
    max-height: 80vh;
    border-radius: 12px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.2);
    display: flex;
    flex-direction: column;
    color: var(--text-primary);
    overflow: hidden;
  }
  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-color);
  }
  .header-title {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .header-title .icon {
    display: flex;
    color: var(--accent-blue);
  }
  .modal-header h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .close-btn {
    background: none;
    border: none;
    font-size: 16px;
    color: var(--text-secondary);
    cursor: pointer;
    line-height: 1;
  }
  .close-btn:hover {
    color: var(--text-primary);
  }
  .modal-body {
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }
  .modal-footer {
    padding: 16px 20px;
    border-top: 1px solid var(--border-color);
    display: flex;
    gap: 8px;
    background: var(--bg-body);
  }
  .spacer {
    flex: 1;
  }
  .modal-footer button {
    padding: 8px 16px;
    border-radius: 6px;
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    border: none;
  }
  .btn-save {
    background: var(--accent-blue);
    color: white;
  }
  .btn-save:hover {
    opacity: 0.9;
  }
  .btn-cancel {
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid var(--border-color) !important;
  }
  .btn-cancel:hover {
    background: var(--sidebar-hover);
  }
  
  .read-view { gap: 16px; padding: 24px 32px; flex: 1; overflow-y: auto;}
  .read-title { font-size: 24px; font-weight: 600; color: var(--text-primary); margin: 0; line-height: 1.3;}
  .read-time { font-size: 15px; color: var(--text-secondary); font-weight: 500; display: flex; align-items: center; gap: 8px; }
  
  .section-block { margin-top: 8px; border-top: 1px solid var(--border-color); padding-top: 16px; }
  .notes-header { font-size: 14px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; color: var(--text-secondary); margin-bottom: 12px; }
  .read-desc { font-size: 14px; color: var(--text-secondary); line-height: 1.6; }
  
  .text-notes {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    font-size: 13px;
    background: var(--sidebar-bg, #f5f5f5);
    padding: 16px;
    border-radius: 8px;
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    white-space: pre-wrap;
  }
</style>
