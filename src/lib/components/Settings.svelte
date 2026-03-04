<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    iconClose,
    iconUser,
    iconPlus,
    iconCheck,
  } from "$lib/components/icons";

  interface SettingsProps {
    show: boolean;
    accounts: Array<{
      id: string;
      email: string;
      display_name: string;
      avatar_url: string;
      is_active: boolean;
    }>;
    onclose: () => void;
    onAccountSwitch: (id: string) => void;
    onAccountAdd: () => void;
    onAccountRemove: (id: string) => void;
  }

  let {
    show = $bindable(false),
    accounts = [],
    onclose = () => {},
    onAccountSwitch = () => {},
    onAccountAdd = () => {},
    onAccountRemove = () => {},
  }: SettingsProps = $props();

  let activeTab = $state("accounts");
  let settings: Record<string, string> = $state({});

  const tabs = [
    { id: "accounts", label: "Accounts" },
    { id: "sync", label: "Sync & Storage" },
    { id: "reading", label: "Reading" },
    { id: "compose", label: "Compose & Reply" },
    { id: "notifications", label: "Notifications" },
    { id: "shortcuts", label: "Shortcuts" },
    { id: "about", label: "About" },
  ];

  async function loadSettings() {
    try {
      const entries: Array<{ key: string; value: string }> =
        await invoke("get_settings");
      settings = {};
      for (const e of entries) {
        settings[e.key] = e.value;
      }
    } catch (e) {
      console.error("Failed to load settings", e);
    }
  }

  async function saveSetting(key: string, value: string) {
    settings[key] = value;
    try {
      await invoke("update_setting", { key, value });
    } catch (e) {
      console.error("Failed to save setting", e);
    }
  }

  $effect(() => {
    if (show) {
      loadSettings();
    }
  });

  const shortcuts = [
    { key: "E", action: "Archive conversation" },
    { key: "#", action: "Move to Trash" },
    { key: "Shift + I", action: "Mark as Unread" },
    { key: "/", action: "Focus search bar" },
    { key: "Esc", action: "Deselect / Close" },
  ];

  const syncFreqStops = [5, 10, 15, 30, 60, 120, 180, 300, 600, 900, 1800];
  const threadStops = [25, 50, 100, 150, 200, 300, 500];
  const cacheStops = [250, 500, 1000, 2000, 5000, 10000, 50000];

  function freqToSlider(val: string): number {
    const n = parseInt(val) || 30;
    const idx = syncFreqStops.indexOf(n);
    return idx >= 0 ? idx : 3;
  }
  function sliderToFreq(pos: number): string {
    return String(syncFreqStops[pos] || 30);
  }
  function freqLabel(val: string): string {
    const n = parseInt(val) || 30;
    if (n < 60) return `${n} seconds`;
    const m = n / 60;
    return m === 1 ? "1 minute" : `${m} minutes`;
  }
  function threadToSlider(val: string): number {
    const n = parseInt(val) || 100;
    const idx = threadStops.indexOf(n);
    return idx >= 0 ? idx : 2;
  }
  function sliderToThread(pos: number): string {
    return String(threadStops[pos] || 100);
  }
  function cacheToSlider(val: string): number {
    const n = parseInt(val) || 500;
    const idx = cacheStops.indexOf(n);
    return idx >= 0 ? idx : 1;
  }
  function sliderToCache(pos: number): string {
    return String(cacheStops[pos] || 500);
  }
  function cacheLabel(val: string): string {
    const n = parseInt(val) || 500;
    if (n >= 1000) return `${(n / 1000).toFixed(n % 1000 === 0 ? 0 : 1)} GB`;
    return `${n} MB`;
  }
</script>

{#if show}
  <div
    class="settings-backdrop"
    onclick={onclose}
    role="button"
    tabindex="0"
    onkeydown={(e) => {
      if (e.key === "Escape" || e.key === "Enter" || e.key === " ") onclose();
    }}
  >
    <div
      class="settings-modal"
      onclick={(e) => e.stopPropagation()}
      role="dialog"
      aria-label="Settings"
      tabindex="-1"
    >
      <div class="settings-header">
        <h2>Settings</h2>
        <button
          class="modal-close"
          onclick={onclose}
          aria-label="Close Settings">{@html iconClose}</button
        >
      </div>

      <div class="settings-body">
        <nav class="settings-nav">
          {#each tabs as tab}
            <button
              class="nav-item {activeTab === tab.id ? 'active' : ''}"
              onclick={() => (activeTab = tab.id)}>{tab.label}</button
            >
          {/each}
        </nav>

        <div class="settings-content">
          {#if activeTab === "accounts"}
            <div class="section">
              <div class="section-title">Connected Accounts</div>
              <p class="section-desc">
                Manage your Google accounts linked to Rustymail.
              </p>
              <div class="account-list">
                {#each accounts as account}
                  <div class="account-row">
                    <div class="account-avatar">
                      {#if account.avatar_url}
                        <img
                          src={account.avatar_url}
                          alt=""
                          class="avatar-img"
                          referrerpolicy="no-referrer"
                        />
                      {:else}
                        <span class="avatar-placeholder">{@html iconUser}</span>
                      {/if}
                    </div>
                    <div class="account-info">
                      <span class="account-name"
                        >{account.display_name || account.email}</span
                      >
                      <span class="account-email">{account.email}</span>
                    </div>
                    <div class="account-actions">
                      {#if account.is_active}
                        <span class="active-badge"
                          >{@html iconCheck} Active</span
                        >
                      {:else}
                        <button
                          class="btn-sm"
                          onclick={() => onAccountSwitch(account.id)}
                          >Switch</button
                        >
                      {/if}
                      <button
                        class="btn-sm btn-danger"
                        onclick={() => onAccountRemove(account.id)}
                        >Remove</button
                      >
                    </div>
                  </div>
                {/each}
              </div>
              <button class="btn-add-account" onclick={onAccountAdd}>
                {@html iconPlus} Add Account
              </button>
            </div>
          {:else if activeTab === "sync"}
            <div class="section">
              <div class="section-title">Sync & Storage</div>
              <p class="section-desc">
                Control how Rustymail fetches and stores your email data.
              </p>

              <div class="setting-group">
                <div class="setting-label">
                  <span class="setting-name">Sync Frequency</span>
                  <span class="setting-hint"
                    >{settings.sync_frequency === "manual"
                      ? "Manual refresh only"
                      : `Check every ${freqLabel(settings.sync_frequency || "30")}`}</span
                  >
                </div>
                <div class="slider-row">
                  <input
                    type="range"
                    class="range-slider"
                    min="0"
                    max={syncFreqStops.length - 1}
                    step="1"
                    value={freqToSlider(settings.sync_frequency || "30")}
                    disabled={settings.sync_frequency === "manual"}
                    oninput={(e) =>
                      saveSetting(
                        "sync_frequency",
                        sliderToFreq(parseInt(e.currentTarget.value)),
                      )}
                  />
                  <span class="slider-value"
                    >{settings.sync_frequency === "manual"
                      ? "—"
                      : freqLabel(settings.sync_frequency || "30")}</span
                  >
                </div>
                <label class="toggle-row compact">
                  <input
                    type="checkbox"
                    class="toggle"
                    checked={settings.sync_frequency === "manual"}
                    onchange={(e) =>
                      saveSetting(
                        "sync_frequency",
                        e.currentTarget.checked ? "manual" : "30",
                      )}
                  />
                  <span class="setting-hint">Manual only</span>
                </label>
              </div>

              <div class="setting-group">
                <div class="setting-label">
                  <span class="setting-name">Threads per Sync</span>
                  <span class="setting-hint"
                    >Max threads fetched per sync cycle</span
                  >
                </div>
                <div class="slider-row">
                  <input
                    type="range"
                    class="range-slider"
                    min="0"
                    max={threadStops.length - 1}
                    step="1"
                    value={threadToSlider(settings.max_threads_sync || "100")}
                    oninput={(e) =>
                      saveSetting(
                        "max_threads_sync",
                        sliderToThread(parseInt(e.currentTarget.value)),
                      )}
                  />
                  <span class="slider-value"
                    >{settings.max_threads_sync || "100"}</span
                  >
                </div>
              </div>

              <div class="setting-group">
                <div class="setting-label">
                  <span class="setting-name">Cache Size Limit</span>
                  <span class="setting-hint"
                    >Maximum disk space for cached emails</span
                  >
                </div>
                <div class="slider-row">
                  <input
                    type="range"
                    class="range-slider"
                    min="0"
                    max={cacheStops.length - 1}
                    step="1"
                    value={cacheToSlider(settings.max_cache_mb || "500")}
                    oninput={(e) =>
                      saveSetting(
                        "max_cache_mb",
                        sliderToCache(parseInt(e.currentTarget.value)),
                      )}
                  />
                  <span class="slider-value"
                    >{cacheLabel(settings.max_cache_mb || "500")}</span
                  >
                </div>
              </div>
            </div>
          {:else if activeTab === "reading"}
            <div class="section">
              <div class="section-title">Reading Preferences</div>
              <p class="section-desc">
                Customize how you read and navigate emails.
              </p>

              <div class="setting-group">
                <div class="setting-label">
                  <span class="setting-name">Default Mailbox</span>
                  <span class="setting-hint"
                    >Which folder to open on launch</span
                  >
                </div>
                <div class="option-group">
                  {#each [["INBOX", "Inbox"], ["STARRED", "Starred"], ["SENT", "Sent"]] as [val, label]}
                    <button
                      class="option-btn {(settings.default_mailbox ||
                        'INBOX') === val
                        ? 'selected'
                        : ''}"
                      onclick={() => saveSetting("default_mailbox", val)}
                      >{label}</button
                    >
                  {/each}
                </div>
              </div>

              <div class="setting-group">
                <div class="setting-label">
                  <span class="setting-name">Mark as Read</span>
                  <span class="setting-hint"
                    >Delay before marking opened emails as read</span
                  >
                </div>
                <div class="option-group">
                  {#each [["instant", "Instant"], ["2", "2 sec"], ["5", "5 sec"], ["never", "Never"]] as [val, label]}
                    <button
                      class="option-btn {(settings.mark_read_delay || '2') ===
                      val
                        ? 'selected'
                        : ''}"
                      onclick={() => saveSetting("mark_read_delay", val)}
                      >{label}</button
                    >
                  {/each}
                </div>
              </div>
            </div>
          {:else if activeTab === "compose"}
            <div class="section">
              <div class="section-title">Compose & Reply</div>
              <p class="section-desc">
                Adjust default settings for sending messages.
              </p>

              <div class="setting-group">
                <div class="setting-label">
                  <span class="setting-name">Default Reply Behavior</span>
                  <span class="setting-hint">Choose default action</span>
                </div>
                <div class="option-group">
                  {#each [["reply", "Reply"], ["reply_all", "Reply All"]] as [val, label]}
                    <button
                      class="option-btn {(settings.default_reply || 'reply') ===
                      val
                        ? 'selected'
                        : ''}"
                      onclick={() => saveSetting("default_reply", val)}
                      >{label}</button
                    >
                  {/each}
                </div>
              </div>

              <div
                class="setting-group"
                style="flex-direction: column; align-items: flex-start; gap: 12px; border-bottom: none; border-top: 1px solid var(--border-color); padding-top: 16px; margin-top: 8px;"
              >
                <div class="setting-label">
                  <span class="setting-name">Email Signature</span>
                  <span class="setting-hint"
                    >Appended at the end of all outgoing messages (HTML
                    supported)</span
                  >
                </div>
                <textarea
                  class="signature-input"
                  placeholder="Sent from Rustymail"
                  value={settings.signature || ""}
                  oninput={(e) => {
                    settings.signature = e.currentTarget.value;
                  }}
                  onblur={(e) =>
                    saveSetting("signature", e.currentTarget.value)}
                ></textarea>
              </div>
            </div>
          {:else if activeTab === "notifications"}
            <div class="section">
              <div class="section-title">Notifications</div>
              <p class="section-desc">
                Choose when and how Rustymail notifies you.
              </p>
              <label class="toggle-row">
                <div class="toggle-label">
                  <span class="setting-name">Desktop Notifications</span>
                  <span class="setting-hint">Show alerts for new messages</span>
                </div>
                <input
                  type="checkbox"
                  class="toggle"
                  checked={settings.notifications_enabled === "true"}
                  onchange={(e) =>
                    saveSetting(
                      "notifications_enabled",
                      e.currentTarget.checked ? "true" : "false",
                    )}
                />
              </label>
              <label class="toggle-row">
                <div class="toggle-label">
                  <span class="setting-name">Notification Sounds</span>
                  <span class="setting-hint"
                    >Play a sound when messages arrive</span
                  >
                </div>
                <input
                  type="checkbox"
                  class="toggle"
                  checked={settings.notifications_sound === "true"}
                  onchange={(e) =>
                    saveSetting(
                      "notifications_sound",
                      e.currentTarget.checked ? "true" : "false",
                    )}
                />
              </label>
            </div>
          {:else if activeTab === "shortcuts"}
            <div class="section">
              <div class="section-title">Keyboard Shortcuts</div>
              <p class="section-desc">Navigate faster with these shortcuts.</p>
              <div class="shortcut-list">
                {#each shortcuts as s}
                  <div class="shortcut-row">
                    <kbd>{s.key}</kbd>
                    <span>{s.action}</span>
                  </div>
                {/each}
              </div>
            </div>
          {:else if activeTab === "about"}
            <div class="section about-section">
              <div class="about-logo">📬</div>
              <div class="about-name">Rustymail</div>
              <div class="about-version">Version 0.1.0</div>
              <p class="about-desc">
                A fast, private, cross-platform Gmail client built with Rust and
                Tauri.
              </p>
              <div class="about-links">
                <span class="about-link">Made with ❤️ by Tiago Fortunato</span>
              </div>
            </div>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .settings-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(6px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    animation: fadeIn 0.15s ease;
  }
  @keyframes fadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .settings-modal {
    background: var(--bg-view);
    border-radius: 14px;
    width: 720px;
    max-height: 540px;
    box-shadow:
      0 24px 80px rgba(0, 0, 0, 0.3),
      0 0 0 1px rgba(255, 255, 255, 0.05);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    animation: slideUp 0.2s ease;
  }
  @keyframes slideUp {
    from {
      opacity: 0;
      transform: translateY(12px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  .settings-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-color);
  }

  .settings-header h2 {
    font-size: 15px;
    font-weight: 600;
  }

  .modal-close {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-secondary);
    padding: 4px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    transition: all 0.1s;
  }
  .modal-close:hover {
    color: var(--text-primary);
    background: var(--sidebar-hover);
  }

  .settings-body {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  .settings-nav {
    width: 170px;
    border-right: 1px solid var(--border-color);
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex-shrink: 0;
  }

  .nav-item {
    background: none;
    border: none;
    padding: 7px 12px;
    text-align: left;
    font-size: 13px;
    border-radius: 6px;
    cursor: pointer;
    color: var(--text-primary);
    font-family: var(--font-family);
    transition: background 0.1s;
  }
  .nav-item:hover {
    background: var(--sidebar-hover);
  }
  .nav-item.active {
    background: var(--accent-blue);
    color: white;
  }

  .settings-content {
    flex: 1;
    padding: 24px;
    overflow-y: auto;
  }

  .section-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: 4px;
  }

  .section-desc {
    font-size: 12px;
    color: var(--text-secondary);
    margin-bottom: 20px;
    line-height: 1.4;
  }

  .setting-row:last-child {
    border-bottom: none;
  }

  .setting-group {
    padding: 12px 0;
    border-bottom: 1px solid var(--border-color);
  }
  .setting-group:last-child {
    border-bottom: none;
  }

  .setting-label {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .setting-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
  }
  .setting-hint {
    font-size: 11px;
    color: var(--text-secondary);
  }

  .option-group {
    display: flex;
    gap: 0;
    flex-wrap: wrap;
    margin-top: 8px;
    border: 1px solid var(--border-color);
    border-radius: 8px;
    overflow: hidden;
    width: fit-content;
  }

  .option-btn {
    padding: 6px 14px;
    border: none;
    border-right: 1px solid var(--border-color);
    background: transparent;
    font-size: 12px;
    color: var(--text-primary);
    cursor: pointer;
    font-family: var(--font-family);
    transition: all 0.15s;
  }
  .option-btn:last-child {
    border-right: none;
  }
  .option-btn:hover {
    background: var(--sidebar-hover);
  }
  .option-btn.selected {
    background: var(--accent-blue);
    color: white;
  }

  .slider-row {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-top: 8px;
  }
  .slider-value {
    font-size: 12px;
    font-weight: 600;
    color: var(--accent-blue);
    min-width: 70px;
    text-align: left;
    white-space: nowrap;
  }
  .range-slider {
    flex: 1;
    -webkit-appearance: none;
    appearance: none;
    height: 4px;
    background: var(--border-color);
    border-radius: 2px;
    outline: none;
    cursor: pointer;
    accent-color: var(--accent-blue);
  }
  .range-slider::-webkit-slider-runnable-track {
    height: 4px;
    background: var(--border-color);
    border-radius: 2px;
  }
  .range-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--accent-blue);
    cursor: pointer;
    border: 2px solid white;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
    transition: transform 0.1s;
    margin-top: -6px;
  }
  .range-slider::-webkit-slider-thumb:hover {
    transform: scale(1.15);
  }
  .range-slider:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .range-slider:disabled::-webkit-slider-thumb {
    background: var(--text-secondary);
    cursor: not-allowed;
  }

  .signature-input {
    width: 100%;
    min-height: 80px;
    background: var(--bg-list);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    padding: 10px;
    font-size: 13px;
    color: var(--text-primary);
    font-family: inherit;
    resize: vertical;
    outline: none;
    transition:
      border-color 0.15s ease,
      box-shadow 0.15s ease;
  }
  .signature-input:focus {
    border-color: var(--accent-blue);
    box-shadow: 0 0 0 3px rgba(10, 132, 255, 0.15);
  }

  .toggle-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 0;
    cursor: pointer;
    border-bottom: 1px solid var(--border-color);
  }
  .toggle-row:last-child {
    border-bottom: none;
  }
  .toggle-row.compact {
    padding: 6px 0;
    border-bottom: none;
    justify-content: flex-start;
    gap: 8px;
  }
  .toggle-label {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .toggle {
    width: 38px;
    height: 22px;
    appearance: none;
    -webkit-appearance: none;
    background: var(--sidebar-hover);
    border: 1px solid var(--border-color);
    border-radius: 11px;
    position: relative;
    cursor: pointer;
    transition: all 0.2s;
    flex-shrink: 0;
  }
  .toggle::after {
    content: "";
    position: absolute;
    top: 1px;
    left: 1px;
    width: 18px;
    height: 18px;
    background: white;
    border-radius: 50%;
    transition: transform 0.2s;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);
  }
  .toggle:checked {
    background: var(--accent-blue);
    border-color: var(--accent-blue);
  }
  .toggle:checked::after {
    transform: translateX(16px);
  }

  .account-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .account-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    border: 1px solid var(--border-color);
    border-radius: 10px;
    transition: background 0.1s;
  }
  .account-row:hover {
    background: var(--sidebar-hover);
  }

  .account-avatar {
    width: 36px;
    height: 36px;
    flex-shrink: 0;
  }
  .avatar-img {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    object-fit: cover;
  }
  .avatar-placeholder {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: var(--sidebar-hover);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary);
  }

  .account-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .account-name {
    font-size: 13px;
    font-weight: 500;
  }
  .account-email {
    font-size: 11px;
    color: var(--text-secondary);
  }
  .account-actions {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .active-badge {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: #34c759;
    font-weight: 500;
  }

  .btn-sm {
    padding: 4px 10px;
    font-size: 11px;
    border: 1px solid var(--border-color);
    border-radius: 5px;
    background: transparent;
    color: var(--text-primary);
    cursor: pointer;
    font-family: var(--font-family);
    transition: all 0.1s;
  }
  .btn-sm:hover {
    background: var(--sidebar-hover);
  }

  .btn-danger {
    color: #ff3b30;
    border-color: rgba(255, 59, 48, 0.3);
  }
  .btn-danger:hover {
    background: rgba(255, 59, 48, 0.08);
  }

  .btn-add-account {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 12px;
    padding: 10px 14px;
    border: 1px dashed var(--border-color);
    border-radius: 10px;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 13px;
    font-family: var(--font-family);
    width: 100%;
    justify-content: center;
    transition: all 0.15s;
  }
  .btn-add-account:hover {
    border-color: var(--accent-blue);
    color: var(--accent-blue);
    background: rgba(10, 132, 255, 0.04);
  }

  .shortcut-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .shortcut-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 0;
    font-size: 13px;
    border-bottom: 1px solid var(--border-color);
  }
  .shortcut-row:last-child {
    border-bottom: none;
  }
  kbd {
    background: var(--sidebar-hover);
    padding: 3px 10px;
    border-radius: 5px;
    font-size: 12px;
    font-family: -apple-system, monospace;
    border: 1px solid var(--border-color);
    min-width: 36px;
    text-align: center;
  }

  .about-section {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding-top: 40px;
  }
  .about-logo {
    font-size: 48px;
    margin-bottom: 12px;
  }
  .about-name {
    font-size: 20px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .about-version {
    font-size: 12px;
    color: var(--text-secondary);
    margin-top: 4px;
  }
  .about-desc {
    font-size: 13px;
    color: var(--text-secondary);
    margin-top: 16px;
    max-width: 320px;
    line-height: 1.5;
  }
  .about-links {
    margin-top: 24px;
  }
  .about-link {
    font-size: 12px;
    color: var(--text-secondary);
  }
</style>
