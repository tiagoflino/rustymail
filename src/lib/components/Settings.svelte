<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import {
    iconClose,
    iconUser,
    iconPlus,
    iconCheck,
  } from "$lib/components/icons";
  import { checkForUpdates } from "$lib/utils/updater";

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
    onThemeChange?: (mode: string) => void;
  }

  let {
    show = $bindable(false),
    accounts = [],
    onclose = () => {},
    onAccountSwitch = () => {},
    onAccountAdd = () => {},
    onAccountRemove = () => {},
    onThemeChange = () => {},
  }: SettingsProps = $props();

  let activeTab = $state("accounts");
  let settings: Record<string, string> = $state({});
  let appVersion = $state("...");

  getVersion().then((v) => (appVersion = v));

  const navIcons: Record<string, string> = {
    accounts: `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>`,
    sync: `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/></svg>`,
    reading: `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2 3h6a4 4 0 014 4v14a3 3 0 00-3-3H2z"/><path d="M22 3h-6a4 4 0 00-4 4v14a3 3 0 013-3h7z"/></svg>`,
    compose: `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>`,
    notifications: `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M13.73 21a2 2 0 01-3.46 0"/></svg>`,
    shortcuts: `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="4" width="20" height="16" rx="2"/><path d="M6 8h.001M10 8h.001M14 8h.001M18 8h.001M8 12h.001M12 12h.001M16 12h.001M7 16h10"/></svg>`,
    about: `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>`,
  };

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
      if (!settings.link_behavior) settings.link_behavior = "open";
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
    { key: "/", action: "Focus search bar" },
    { key: "Esc", action: "Deselect / Close" },
    { key: "[", action: "Toggle sidebar" },
    { key: "E", action: "Archive conversation" },
    { key: "#", action: "Move to Trash" },
    { key: "Shift + I", action: "Mark as Unread" },
    { key: "R", action: "Reply to last message" },
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
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      aria-label="Settings"
      tabindex="-1"
    >
      <button
        class="modal-close"
        onclick={onclose}
        aria-label="Close Settings">{@html iconClose}</button
      >

      <div class="settings-body">
        <nav class="settings-nav">
          {#each tabs as tab}
            <button
              class="nav-item {activeTab === tab.id ? 'active' : ''}"
              onclick={() => (activeTab = tab.id)}
            >
              <span class="nav-icon">{@html navIcons[tab.id]}</span>
              {tab.label}
            </button>
          {/each}
        </nav>

        <div class="settings-content">
          {#if activeTab === "accounts"}
            <div class="section">
              <div class="section-title">Connected Accounts</div>
              <p class="section-desc">
                Manage your Google accounts linked to Rustymail.
              </p>
              <div class="setting-card">
                {#each accounts as account, i}
                  <div class="account-row" class:last={i === accounts.length - 1}>
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

              <div class="setting-card">
                <div class="card-row">
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
                        ? "\u2014"
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

                <div class="card-row">
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

                <div class="card-row">
                  <label class="toggle-row" style="border-bottom: none; padding: 0;">
                    <div class="toggle-label">
                      <span class="setting-name">Pre-fetch Message Bodies</span>
                      <span class="setting-hint">Download full message content during sync so emails open instantly. Uses more bandwidth and storage.</span>
                    </div>
                    <input
                      type="checkbox"
                      class="toggle"
                      checked={settings.prefetch_bodies === "true"}
                      onchange={(e) =>
                        saveSetting(
                          "prefetch_bodies",
                          e.currentTarget.checked ? "true" : "false",
                        )}
                    />
                  </label>
                </div>

                <div class="card-row last">
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
            </div>
          {:else if activeTab === "reading"}
            <div class="section">
              <div class="section-title">Reading Preferences</div>
              <p class="section-desc">
                Customize how you read and navigate emails.
              </p>

              <div class="setting-card">
                <div class="card-row">
                  <div class="setting-row-inline">
                    <div class="setting-label">
                      <span class="setting-name">Appearance</span>
                      <span class="setting-hint">Choose light, dark, or follow your system</span>
                    </div>
                    <div class="option-group">
                      {#each [["system", "System"], ["light", "Light"], ["dark", "Dark"]] as [val, label]}
                        <button
                          class="option-btn {(settings.theme || 'system') === val ? 'selected' : ''}"
                          onclick={() => {
                            saveSetting("theme", val);
                            onThemeChange?.(val);
                          }}
                        >{label}</button>
                      {/each}
                    </div>
                  </div>
                </div>

                <div class="card-row">
                  <div class="setting-row-inline">
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
                </div>

                <div class="card-row">
                  <div class="setting-row-inline">
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

                <div class="card-row last">
                  <div class="setting-row-inline">
                    <div class="setting-label">
                      <span class="setting-name">Link Behavior</span>
                      <span class="setting-hint">How to handle links in emails</span>
                    </div>
                    <div class="option-group">
                      {#each [["open", "Browser"], ["ask", "Ask first"], ["disable", "Disabled"]] as [val, label]}
                        <button
                          class="option-btn {(settings.link_behavior || 'open') === val ? 'selected' : ''}"
                          onclick={() => saveSetting("link_behavior", val)}
                        >{label}</button>
                      {/each}
                    </div>
                  </div>
                </div>
              </div>
            </div>
          {:else if activeTab === "compose"}
            <div class="section">
              <div class="section-title">Compose & Reply</div>
              <p class="section-desc">
                Adjust default settings for sending messages.
              </p>

              <div class="setting-card">
                <div class="card-row">
                  <div class="setting-row-inline">
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
                </div>

                <div class="card-row last">
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
            </div>
          {:else if activeTab === "notifications"}
            <div class="section">
              <div class="section-title">Notifications</div>
              <p class="section-desc">
                Choose when and how Rustymail notifies you.
              </p>
              <div class="setting-card">
                <div class="card-row">
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
                </div>
                <div class="card-row last">
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
              </div>
            </div>
          {:else if activeTab === "shortcuts"}
            <div class="section">
              <div class="section-title">Keyboard Shortcuts</div>
              <p class="section-desc">Navigate faster with these shortcuts.</p>
              <div class="setting-card">
                {#each shortcuts as s, i}
                  <div class="card-row shortcut-row" class:last={i === shortcuts.length - 1}>
                    <kbd>{s.key}</kbd>
                    <span class="shortcut-action">{s.action}</span>
                  </div>
                {/each}
              </div>
            </div>
          {:else if activeTab === "about"}
            <div class="section about-section">
              <img class="about-logo-img" src="/app-icon.png" alt="Rustymail" />
              <div class="about-name">Rustymail</div>
              <div class="about-version">Version {appVersion}</div>
              <button class="btn-check-update" onclick={() => checkForUpdates(false)}>
                Check for Updates
              </button>
              <p class="about-desc">
                A light and fast cross-platform Gmail client.
              </p>
              <div class="about-links">
                <span class="about-link">Made with care by Tiago Fortunato</span>
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
    position: relative;
    background: var(--bg-view);
    border-radius: 14px;
    width: 740px;
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

  .modal-close {
    position: absolute;
    top: 12px;
    right: 12px;
    z-index: 10;
    background: var(--sidebar-hover, rgba(0, 0, 0, 0.06));
    border: none;
    cursor: pointer;
    color: var(--text-secondary);
    padding: 5px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s;
    width: 26px;
    height: 26px;
  }
  .modal-close:hover {
    color: var(--text-primary);
    background: var(--sidebar-hover, rgba(0, 0, 0, 0.1));
  }

  .settings-body {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  .settings-nav {
    width: 180px;
    border-right: 1px solid var(--border-color);
    padding: 12px 8px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex-shrink: 0;
  }

  .nav-item {
    background: none;
    border: none;
    padding: 7px 10px;
    text-align: left;
    font-size: 13px;
    border-radius: 8px;
    cursor: pointer;
    color: var(--text-primary);
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    transition: background 0.15s;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .nav-item:hover {
    background: var(--sidebar-hover, rgba(0, 0, 0, 0.04));
  }
  .nav-item.active {
    background: rgba(10, 132, 255, 0.12);
    color: var(--accent-blue, #0a84ff);
  }

  .nav-icon {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    opacity: 0.7;
  }
  .nav-item.active .nav-icon {
    opacity: 1;
    color: var(--accent-blue, #0a84ff);
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
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  }

  .section-desc {
    font-size: 12px;
    color: var(--text-secondary);
    margin-bottom: 16px;
    line-height: 1.4;
  }

  /* Card containers for grouped settings */
  .setting-card {
    background: var(--bg-sidebar, rgba(0, 0, 0, 0.03));
    border-radius: 10px;
    padding: 0;
    margin-bottom: 16px;
    overflow: hidden;
  }

  .card-row {
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-color, rgba(0, 0, 0, 0.06));
  }
  .card-row.last,
  .card-row:last-child {
    border-bottom: none;
  }

  .setting-row-inline {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
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
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  }
  .setting-hint {
    font-size: 11px;
    color: var(--text-secondary);
  }

  /* Segmented control */
  .option-group {
    display: inline-flex;
    background: var(--border-color, rgba(0, 0, 0, 0.06));
    border-radius: 8px;
    padding: 2px;
    gap: 1px;
    flex-shrink: 0;
  }

  .option-btn {
    flex: 1;
    padding: 5px 14px;
    border: none;
    border-radius: 6px;
    background: transparent;
    font-size: 12px;
    color: var(--text-secondary);
    cursor: pointer;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    transition: all 0.15s;
    white-space: nowrap;
  }
  .option-btn:hover {
    color: var(--text-primary);
  }
  .option-btn.selected {
    background: var(--bg-view, white);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1), 0 0 0 0.5px rgba(0, 0, 0, 0.04);
    color: var(--text-primary);
    font-weight: 500;
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
    background: var(--bg-view, white);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    padding: 10px;
    font-size: 13px;
    color: var(--text-primary);
    font-family: inherit;
    resize: vertical;
    outline: none;
    margin-top: 10px;
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
    padding: 0;
    cursor: pointer;
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

  /* Account rows inside card */
  .account-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-color, rgba(0, 0, 0, 0.06));
    transition: background 0.1s;
  }
  .account-row.last,
  .account-row:last-child {
    border-bottom: none;
  }
  .account-row:hover {
    background: rgba(0, 0, 0, 0.02);
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
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
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
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-top: 10px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--accent-blue, #0a84ff);
    cursor: pointer;
    font-size: 13px;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    transition: opacity 0.15s;
  }
  .btn-add-account:hover {
    opacity: 0.7;
  }

  /* Shortcuts */
  .shortcut-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 13px;
  }
  .shortcut-action {
    color: var(--text-secondary);
    font-size: 13px;
  }
  kbd {
    background: var(--bg-view, white);
    padding: 3px 10px;
    border-radius: 5px;
    font-size: 12px;
    font-family: -apple-system, monospace;
    border: 1px solid var(--border-color);
    min-width: 36px;
    text-align: center;
    box-shadow: 0 1px 1px rgba(0, 0, 0, 0.06);
  }

  /* About section */
  .about-section {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding-top: 32px;
  }
  .about-logo-img {
    width: 80px;
    height: 80px;
    border-radius: 18px;
    margin-bottom: 16px;
    object-fit: contain;
  }
  .about-name {
    font-size: 20px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  }
  .about-version {
    font-size: 12px;
    color: var(--text-tertiary, var(--text-secondary));
    margin-top: 4px;
    opacity: 0.6;
  }
  .about-desc {
    font-size: 13px;
    color: var(--text-secondary);
    margin-top: 20px;
    max-width: 320px;
    line-height: 1.5;
  }
  .about-links {
    margin-top: 32px;
  }
  .about-link {
    font-size: 12px;
    color: var(--text-secondary);
    opacity: 0.6;
  }

  .btn-check-update {
    margin-top: 10px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--accent-blue, #0a84ff);
    cursor: pointer;
    font-size: 13px;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    transition: opacity 0.15s;
  }
  .btn-check-update:hover {
    opacity: 0.7;
    text-decoration: underline;
  }
</style>
