<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { fade, scale } from "svelte/transition";
  import { addToast } from "$lib/stores/toast";
  import { loadContacts } from "$lib/stores/contacts";

  let { onClose }: { onClose: () => void } = $props();

  let importData = $state("");
  let importFormat = $state<"vcard" | "csv">("vcard");
  let isImporting = $state(false);
  let exportResult = $state("");
  let isExporting = $state(false);

  async function handleImport() {
    if (!importData.trim()) {
      addToast("Please paste contact data to import", "error");
      return;
    }
    isImporting = true;
    try {
      const result = await invoke<any[]>("import_contacts", {
        data: importData,
        format: importFormat,
        accountId: null,
      });
      addToast(`Imported ${result.length} contacts`, "success");
      await loadContacts();
      onClose();
    } catch (e: any) {
      addToast(e?.toString() || "Import failed", "error");
    } finally {
      isImporting = false;
    }
  }

  async function handleExport(format: "vcard" | "csv") {
    isExporting = true;
    try {
      const data = await invoke<string>("export_contacts", {
        format,
        contactIds: null,
        accountId: null,
      });
      exportResult = data;
      addToast(`Exported contacts as ${format === "vcard" ? "vCard" : "CSV"}`, "success");
    } catch (e: any) {
      addToast(e?.toString() || "Export failed", "error");
    } finally {
      isExporting = false;
    }
  }

  function copyToClipboard() {
    navigator.clipboard.writeText(exportResult);
    addToast("Copied to clipboard", "success");
  }
</script>

<div class="modal-backdrop" transition:fade={{ duration: 150 }} onclick={onClose} onkeydown={(e) => e.key === "Escape" && onClose()} role="button" tabindex="0">
  <div class="modal-content" transition:scale={{ duration: 150, start: 0.95, opacity: 0 }} onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" aria-modal="true" aria-labelledby="modal-title" tabindex="-1">
    <div class="modal-header">
      <h2 id="modal-title">Import / Export Contacts</h2>
      <button class="close-btn" onclick={onClose} aria-label="Close">
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><line x1="1" y1="1" x2="11" y2="11"/><line x1="11" y1="1" x2="1" y2="11"/></svg>
      </button>
    </div>

    <div class="modal-body">
      <div class="section">
        <h3 class="section-title">Import</h3>
        <div class="format-selector">
          <label class="radio-label">
            <input type="radio" name="import-format" value="vcard" bind:group={importFormat} />
            vCard
          </label>
          <label class="radio-label">
            <input type="radio" name="import-format" value="csv" bind:group={importFormat} />
            CSV
          </label>
        </div>
        <textarea
          class="data-textarea"
          placeholder={importFormat === "vcard" ? "Paste vCard data here (BEGIN:VCARD ...)" : "Paste CSV data here (name,email,phone,...)"}
          bind:value={importData}
        ></textarea>
        <button class="btn btn-primary" onclick={handleImport} disabled={isImporting}>
          {isImporting ? "Importing..." : "Import Contacts"}
        </button>
      </div>

      <div class="divider"></div>

      <div class="section">
        <h3 class="section-title">Export</h3>
        <div class="export-buttons">
          <button class="btn btn-secondary" onclick={() => handleExport("vcard")} disabled={isExporting}>
            Export as vCard
          </button>
          <button class="btn btn-secondary" onclick={() => handleExport("csv")} disabled={isExporting}>
            Export as CSV
          </button>
        </div>
        {#if exportResult}
          <textarea class="data-textarea" readonly value={exportResult}></textarea>
          <button class="btn btn-secondary" onclick={copyToClipboard}>
            Copy to Clipboard
          </button>
        {/if}
      </div>
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
    background: rgba(0, 0, 0, 0.3);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal-content {
    position: relative;
    z-index: 1;
    background: var(--bg-view);
    width: 520px;
    max-width: calc(100vw - 40px);
    max-height: calc(100vh - 80px);
    border-radius: var(--radius-modal);
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.12), 0 0 0 0.5px rgba(0, 0, 0, 0.06);
    display: flex;
    flex-direction: column;
    color: var(--text-primary);
    overflow: hidden;
  }

  :global([data-theme="dark"]) .modal-content {
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.35), 0 0 0 0.5px rgba(255, 255, 255, 0.08);
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
  }

  .modal-header h2 {
    margin: 0;
    font-size: var(--font-size-title);
    font-weight: 600;
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--text-secondary);
    cursor: pointer;
    padding: 4px;
    border-radius: var(--radius-standard);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s, color 0.15s;
  }

  .close-btn:hover {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }

  .modal-body {
    padding: 0 16px 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    overflow-y: auto;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .section-title {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .format-selector {
    display: flex;
    gap: 12px;
  }

  .radio-label {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .radio-label input[type="radio"] {
    margin: 0;
  }

  .data-textarea {
    width: 100%;
    min-height: 120px;
    padding: 8px 10px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-standard);
    background: var(--input-bg);
    color: var(--text-primary);
    font-family: monospace;
    font-size: 12px;
    line-height: 1.4;
    resize: vertical;
    outline: none;
    transition: border-color 0.15s;
  }

  .data-textarea:focus {
    border-color: var(--accent-color);
  }

  .data-textarea::placeholder {
    color: var(--text-tertiary);
  }

  .divider {
    height: 1px;
    background: var(--border-color);
  }

  .export-buttons {
    display: flex;
    gap: 8px;
  }

  .btn {
    padding: 6px 12px;
    border: none;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s, opacity 0.15s;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-primary {
    background: var(--accent-color);
    color: #fff;
  }

  .btn-primary:hover:not(:disabled) {
    opacity: 0.9;
  }

  .btn-secondary {
    background: var(--hover-bg);
    color: var(--text-primary);
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--selected-bg);
  }
</style>
