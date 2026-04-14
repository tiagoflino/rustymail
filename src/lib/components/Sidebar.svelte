<script lang="ts">
  import { writable, type Writable } from "svelte/store";
  import {
    getLabelIcon,
    formatLabelName,
    iconUser,
    iconChevronDown,
    iconPlus,
    iconRefresh,
    iconSettings,
    iconCalendar,
    iconSnooze,
    iconSubscriptions,
  } from "$lib/components/icons";
  import { isSyncing, lastSyncError } from "$lib/stores/threads";
  import { buildLabelTree, type LabelTreeNode } from "$lib/utils/labelTree";

  interface AccountInfo {
    id: string;
    email: string;
    display_name: string;
    avatar_url: string;
    is_active: boolean;
  }

  interface LocalLabel {
    id: string;
    name: string;
    type: string;
    unread_count: number;
    bgColor?: string | null;
  }

  interface Props {
    activeAccount: AccountInfo | null;
    allAccounts: AccountInfo[];
    collapsed: boolean;
    isMacOS: boolean;
    themeIcon: string;
    themeLabel: string;
    sidebarCollapseIcon: string;
    sidebarExpandIcon: string;
    isUnifiedEnabled: boolean;
    labels: Writable<LocalLabel[]>;
    selectedLabelId: Writable<string>;
    snoozedCount?: number;
    oncompose: () => void;
    onsync: () => void;
    onthemecycle: () => void;
    ontogglecalendar: () => void;
    ontogglesubscriptions: () => void;
    onsettings: () => void;
    ontogglecollapse: () => void;
    onselectlabel: (labelId: string) => void;
    onswitchaccount: (accountId: string) => void;
    onaddaccount: () => void;
  }

  let {
    activeAccount,
    allAccounts,
    collapsed,
    isMacOS,
    themeIcon,
    themeLabel,
    sidebarCollapseIcon,
    sidebarExpandIcon,
    isUnifiedEnabled,
    labels,
    selectedLabelId,
    snoozedCount = 0,
    oncompose,
    onsync,
    onthemecycle,
    ontogglecalendar,
    ontogglesubscriptions,
    onsettings,
    ontogglecollapse,
    onselectlabel,
    onswitchaccount,
    onaddaccount,
  }: Props = $props();

  let showAccountDropdown = $state(false);

  let expandedLabels: Set<string> = $state(new Set());

  function toggleLabelExpand(fullPath: string) {
    if (expandedLabels.has(fullPath)) {
      expandedLabels.delete(fullPath);
    } else {
      expandedLabels.add(fullPath);
    }
    expandedLabels = new Set(expandedLabels);
  }
</script>

<aside class="pane-sidebar" class:collapsed>
  <div class="titlebar-spacer sidebar-titlebar" data-tauri-drag-region></div>
  <div class="sidebar-brand">
    <button
      class="account-switcher"
      onclick={() => (showAccountDropdown = !showAccountDropdown)}
    >
      <div class="account-avatar-small">
        {#if activeAccount?.avatar_url}
          <img
            src={activeAccount.avatar_url}
            alt=""
            class="avatar-img-sm"
            referrerpolicy="no-referrer"
          />
        {:else}
          <span class="avatar-placeholder-sm">{@html iconUser}</span>
        {/if}
      </div>
      <div class="account-text">
        <span class="brand-name"
          >{activeAccount?.display_name || "Rustymail"}</span
        >
        <span class="brand-email">{activeAccount?.email || ""}</span>
      </div>
      <span class="chevron">{@html iconChevronDown}</span>
    </button>

    {#if showAccountDropdown}
      <div class="account-dropdown">
        {#each allAccounts as account}
          <button
            class="dropdown-item {account.is_active
              ? 'active-account'
              : ''}"
            onclick={() => { showAccountDropdown = false; onswitchaccount(account.id); }}
          >
            <div class="dropdown-avatar">
              {#if account.avatar_url}
                <img
                  src={account.avatar_url}
                  alt=""
                  class="avatar-img-xs"
                  referrerpolicy="no-referrer"
                />
              {:else}
                <span class="avatar-placeholder-xs">{@html iconUser}</span>
              {/if}
            </div>
            <div class="dropdown-text">
              <span class="dropdown-name"
                >{account.display_name || account.email}</span
              >
              <span class="dropdown-email">{account.email}</span>
            </div>
          </button>
        {/each}
        <div class="dropdown-divider"></div>
        <button class="dropdown-item add-item" onclick={() => { showAccountDropdown = false; onaddaccount(); }}
          >{@html iconPlus} Add Account</button
        >
      </div>
    {/if}
  </div>

  <div class="sidebar-compose">
    <button
      class="btn-sidebar flex-grow sidebar-compose-btn"
      onclick={oncompose}
    >
      <span class="icon">{@html iconPlus}</span><span class="sidebar-text"> Compose</span>
    </button>
    <button
      class="btn-sidebar sidebar-calendar-btn"
      onclick={ontogglecalendar}
      title="Toggle Calendar"
    >
      {@html iconCalendar}
    </button>
    <button
      class="btn-sidebar sidebar-subscriptions-btn"
      onclick={ontogglesubscriptions}
      title="Toggle Subscriptions"
    >
      {@html iconSubscriptions}
    </button>
  </div>

  <div class="sidebar-content">
    {#if isUnifiedEnabled && allAccounts.length > 1 && !collapsed}
      <div class="unified-section">
        <h2 class="sidebar-heading">Unified</h2>
        <ul class="sidebar-menu">
          {#each [
            { id: 'UNIFIED_INBOX', icon: 'UNIFIED_INBOX', name: 'INBOX', display: 'Inbox' },
            { id: 'UNIFIED_SENT', name: 'SENT', display: 'Sent' },
            { id: 'UNIFIED_DRAFT', name: 'DRAFT', display: 'Drafts' },
            { id: 'UNIFIED_TRASH', name: 'TRASH', display: 'Trash' },
            { id: 'UNIFIED_SNOOZED', name: 'SNOOZED', display: 'Snoozed' }
          ] as uLabel}
            <li>
              <div
                class="sidebar-item {$selectedLabelId === uLabel.id ? 'active' : ''}"
                role="button" tabindex="0"
                onclick={() => onselectlabel(uLabel.id)}
                onkeydown={(e) => {
                  if (e.key === "Enter" || e.key === " ") onselectlabel(uLabel.id);
                }}
              >
                <span class="icon">{@html getLabelIcon(uLabel.icon || uLabel.name)}</span>
                <span class="label-text">{uLabel.display}</span>
              </div>
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    <h2 class="sidebar-heading">{activeAccount?.email || "Mailboxes"}</h2>
    <ul class="sidebar-menu">
      {#each $labels.filter((l) => l.type === "system" && !l.name.startsWith("CATEGORY_") && l.name !== "UNREAD") as label}
        <li>
          <div
            class="sidebar-item {$selectedLabelId === label.id
              ? 'active'
              : ''}"
            role="button"
            tabindex="0"
            onclick={() => onselectlabel(label.id)}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") onselectlabel(label.id);
            }}
          >
            <span class="icon {label.name === 'STARRED' ? 'icon-starred' : ''}">{@html getLabelIcon(label.name)}</span>
            <span class="label-text">{formatLabelName(label.name)}</span>
            {#if label.unread_count > 0}<span class="badge"
                >{label.unread_count}</span
              >{/if}
          </div>
        </li>
      {/each}
      <li>
        <div
          class="sidebar-item {$selectedLabelId === 'SNOOZED' ? 'active' : ''}"
          role="button"
          tabindex="0"
          onclick={() => onselectlabel("SNOOZED")}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") onselectlabel("SNOOZED");
          }}
        >
          <span class="icon">{@html iconSnooze}</span>
          <span class="label-text">Snoozed</span>
          {#if snoozedCount > 0}<span class="badge">{snoozedCount}</span>{/if}
        </div>
      </li>
    </ul>

    {#snippet labelTreeNode(nodes: LabelTreeNode[], depth: number)}
      {#each nodes as node}
        {@const hasChildren = node.children.length > 0}
        {@const isExpanded = expandedLabels.has(node.fullPath)}
        {@const isActive = node.label && $selectedLabelId === node.label.id}
        <li>
          <div
            class="sidebar-item {isActive ? 'active' : ''}"
            style="padding-left: {12 + depth * 16}px;"
            role="button"
            tabindex="0"
            onclick={() => {
              if (node.label) onselectlabel(node.label.id);
              if (hasChildren) toggleLabelExpand(node.fullPath);
            }}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                if (node.label) onselectlabel(node.label.id);
                if (hasChildren) toggleLabelExpand(node.fullPath);
              }
            }}
          >
            {#if hasChildren}
              <span class="icon tree-chevron" class:expanded={isExpanded}>{@html iconChevronDown}</span>
            {:else}
              <span class="icon" style={node.label?.bgColor ? `color: ${node.label.bgColor};` : ''}>{@html getLabelIcon("FOLDER")}</span>
            {/if}
            <span class="label-text">{node.name}</span>
            {#if node.label && node.label.unread_count > 0}
              <span class="badge">{node.label.unread_count}</span>
            {/if}
          </div>
          {#if hasChildren && isExpanded}
            <ul class="sidebar-menu nested">
              {@render labelTreeNode(node.children, depth + 1)}
            </ul>
          {/if}
        </li>
      {/each}
    {/snippet}

    {#if !collapsed && $labels.filter((l) => l.type === "user").length > 0}
      <h2 class="sidebar-heading">Labels</h2>
      <ul class="sidebar-menu">
        {@render labelTreeNode(buildLabelTree($labels.filter((l) => l.type === "user")), 0)}
      </ul>
    {/if}
  </div>

  <div class="sidebar-bottom">
    <div class="sidebar-bottom-row">
      <button
        onclick={onsync}
        disabled={$isSyncing}
        class="btn-sidebar flex-grow"
      >
        <span class="icon {$isSyncing ? 'spin' : ''}"
          >{@html iconRefresh}</span
        >
        <span class="sidebar-text">{$isSyncing ? "Syncing\u2026" : "Refresh"}</span>
      </button>
      <button
        onclick={onthemecycle}
        class="btn-sidebar btn-theme"
        title="{themeLabel}"
      >
        <span class="icon">{@html themeIcon}</span>
      </button>
    </div>
    <div class="sidebar-bottom-row">
      <button onclick={onsettings} class="btn-sidebar flex-grow">
        <span class="icon">{@html iconSettings}</span><span class="sidebar-text">Settings</span>
      </button>
      <button
        onclick={ontogglecollapse}
        class="btn-sidebar btn-theme"
        title={collapsed ? "Expand sidebar ([)" : "Collapse sidebar ([)"}
      >
        <span class="icon">{@html collapsed ? sidebarExpandIcon : sidebarCollapseIcon}</span>
      </button>
    </div>
    {#if $lastSyncError}<div class="error sidebar-error">
        {$lastSyncError}
      </div>{/if}
  </div>
</aside>

<style>
  .pane-sidebar {
    position: relative;
    transition: width 0.2s ease, min-width 0.2s ease;
  }
  .pane-sidebar.collapsed {
    width: 56px;
    min-width: 56px;
  }
  .pane-sidebar.collapsed .account-text,
  .pane-sidebar.collapsed .chevron,
  .pane-sidebar.collapsed .sidebar-heading,
  .pane-sidebar.collapsed .label-text,
  .pane-sidebar.collapsed .badge,
  .pane-sidebar.collapsed .sidebar-text,
  .pane-sidebar.collapsed .sidebar-error {
    display: none;
  }
  .pane-sidebar.collapsed .account-switcher {
    justify-content: center;
    padding: 10px 0;
  }
  .pane-sidebar.collapsed .sidebar-compose {
    flex-direction: column;
    padding: 8px 6px 4px !important;
    gap: 4px !important;
  }
  .pane-sidebar.collapsed .sidebar-compose .btn-sidebar {
    width: 100% !important;
    padding: 8px 0 !important;
    min-width: 0;
    flex: none !important;
  }
  .pane-sidebar.collapsed .sidebar-compose .btn-sidebar .icon {
    margin-right: 0;
  }
  .pane-sidebar.collapsed .sidebar-content {
    padding: 8px 6px;
  }
  .pane-sidebar.collapsed .sidebar-item {
    justify-content: center;
    padding: 8px 0;
  }
  .pane-sidebar.collapsed .sidebar-item .icon {
    margin-right: 0;
  }
  .pane-sidebar.collapsed .sidebar-bottom {
    padding: 6px;
  }
  .pane-sidebar.collapsed .sidebar-bottom-row {
    flex-direction: column;
  }
  .pane-sidebar.collapsed .btn-sidebar {
    padding: 6px 0;
    justify-content: center;
  }
  .pane-sidebar.collapsed .btn-sidebar.btn-theme {
    width: 100%;
    flex: unset;
  }
  .pane-sidebar.collapsed .btn-sidebar.flex-grow {
    flex: unset;
  }
  .titlebar-spacer {
    height: 28px;
    flex-shrink: 0;
    -webkit-app-region: drag;
    display: flex;
    align-items: center;
    justify-content: flex-end;
  }
  .sidebar-titlebar {
    background: var(--bg-sidebar);
  }
  .account-switcher {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    width: 100%;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-primary);
    text-align: left;
    border-radius: 0;
    transition: background 0.1s;
    border-bottom: 1px solid var(--border-color);
    font-family: var(--font-family);
  }
  .account-switcher:hover {
    background: var(--sidebar-hover);
  }
  .account-avatar-small {
    width: 28px;
    height: 28px;
    flex-shrink: 0;
  }
  .avatar-img-sm {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    object-fit: cover;
  }
  .avatar-placeholder-sm {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--sidebar-hover);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary);
  }
  .account-text {
    flex: 1;
    overflow: hidden;
  }
  .brand-name {
    display: block;
    font-weight: 600;
    font-size: var(--font-size-base);
    line-height: 16px;
    letter-spacing: -0.08px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .brand-email {
    display: block;
    font-size: var(--font-size-small);
    line-height: 13px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chevron {
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }
  .sidebar-brand {
    position: relative;
  }
  .account-dropdown {
    position: absolute;
    left: 0;
    right: 0;
    top: 100%;
    background: var(--bg-view);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-modal);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.15);
    z-index: 100;
    overflow: hidden;
  }
  .dropdown-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    width: 100%;
    background: none;
    border: none;
    cursor: pointer;
    font-size: var(--font-size-toolbar);
    color: var(--text-primary);
    font-family: var(--font-family);
    text-align: left;
    transition: background 0.1s;
  }
  .dropdown-item:hover {
    background: var(--sidebar-hover);
  }
  .dropdown-item.active-account {
    background: rgba(10, 132, 255, 0.08);
  }
  .dropdown-avatar {
    width: 24px;
    height: 24px;
    flex-shrink: 0;
  }
  .avatar-img-xs {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    object-fit: cover;
  }
  .avatar-placeholder-xs {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--sidebar-hover);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: var(--font-size-small);
    color: var(--text-secondary);
  }
  .dropdown-text {
    flex: 1;
    overflow: hidden;
  }
  .dropdown-name {
    display: block;
    font-weight: 500;
    font-size: var(--font-size-toolbar);
    line-height: 15px;
  }
  .dropdown-email {
    display: block;
    font-size: var(--font-size-small);
    line-height: 13px;
    color: var(--text-secondary);
  }
  .dropdown-divider {
    height: 1px;
    background: var(--border-color);
    margin: 4px 0;
  }
  .add-item {
    color: var(--accent-blue);
    gap: 6px;
  }
  .sidebar-content {
    flex: 1;
    overflow-y: auto;
    padding: 12px;
  }
  .sidebar-heading {
    font-size: var(--font-size-small);
    line-height: 14px;
    text-transform: none;
    color: var(--text-secondary);
    letter-spacing: normal;
    padding: 16px 12px 4px;
    margin: 0;
    font-weight: 500;
  }
  .sidebar-menu {
    list-style: none;
    margin-bottom: 8px;
  }
  .unified-section {
    background: rgba(0, 0, 0, 0.03);
    margin: 0 -8px 16px -8px;
    padding: 4px 8px 8px 8px;
    border-radius: 12px;
    border: 1px solid rgba(0, 0, 0, 0.04);
  }
  :global(body.test-dark) .unified-section {
    background: rgba(255, 255, 255, 0.03);
    border-color: rgba(255, 255, 255, 0.05);
  }
  .unified-section .sidebar-heading {
    padding: 8px 12px 6px 12px;
    color: var(--text-primary);
    font-weight: 600;
    opacity: 0.8;
  }
  .unified-section .sidebar-menu {
    margin-bottom: 0;
  }
  .sidebar-item {
    display: flex;
    align-items: center;
    padding: 6px 12px;
    margin: 2px 0;
    border-radius: var(--radius-standard);
    font-size: var(--font-size-base);
    line-height: 16px;
    letter-spacing: -0.08px;
    color: var(--text-primary);
    cursor: pointer;
    width: 100%;
    background: none;
    border: none;
    text-align: left;
    font-family: var(--font-family);
    transition: background 0.1s ease;
    font-weight: 400;
    transform: translateZ(0);
    backface-visibility: hidden;
    -webkit-font-smoothing: antialiased;
  }
  .sidebar-item .icon {
    width: 18px;
    height: 18px;
    margin-right: 10px;
    opacity: 0.7;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .sidebar-item .icon-starred {
    color: #f2a600;
    opacity: 1;
  }
  .sidebar-item .label-text {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 400;
  }
  .sidebar-item .badge {
    font-size: var(--font-size-small);
    line-height: 14px;
    font-weight: 600;
    color: var(--text-secondary);
    min-width: 20px;
    text-align: right;
  }
  .sidebar-item:hover {
    background: var(--sidebar-hover);
    font-weight: 400;
  }
  .sidebar-item.active {
    background: rgba(10, 132, 255, 0.12);
    color: var(--accent-blue);
    font-weight: 400;
  }
  .sidebar-item.active .icon {
    opacity: 1;
    color: var(--accent-blue);
  }
  .sidebar-item.active .badge {
    color: var(--accent-blue);
  }
  .sidebar-bottom {
    flex-shrink: 0;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border-top: 1px solid var(--border-color);
    overflow: hidden;
  }
  .sidebar-bottom-row {
    display: flex;
    gap: 4px;
    min-width: 0;
  }
  .btn-sidebar {
    width: 100%;
    padding: 6px 10px;
    background: transparent;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-standard);
    color: var(--text-primary);
    cursor: pointer;
    font-size: var(--font-size-toolbar);
    line-height: 15px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    font-family: var(--font-family);
    transition: background 0.1s ease;
  }
  .btn-sidebar .icon {
    width: 14px;
    height: 14px;
    display: flex;
    align-items: center;
  }
  .btn-sidebar:hover {
    background: var(--sidebar-hover);
  }
  .sidebar-compose {
    padding: 12px 12px 4px 12px;
    display: flex;
    gap: 8px;
  }
  .sidebar-compose-btn {
    font-size: var(--font-size-base);
    font-weight: 500;
    padding: 7px 14px;
    background: var(--accent-blue);
    color: white;
    border: none;
    border-radius: var(--radius-standard);
    box-shadow: none;
  }
  .sidebar-compose-btn:hover {
    background: var(--accent-blue);
    opacity: 0.9;
  }
  .sidebar-calendar-btn {
    width: 36px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-view);
    border: 1px solid var(--border-color);
  }
  .sidebar-subscriptions-btn {
    width: 36px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-view);
    border: 1px solid var(--border-color);
  }
  .btn-sidebar.flex-grow {
    flex: 1;
    min-width: 0;
  }
  .btn-sidebar.btn-theme {
    padding: 6px 8px;
    width: 34px;
    min-width: 34px;
    flex: 0 0 34px;
  }
  .error {
    color: #ff453a;
    font-size: var(--font-size-toolbar);
    line-height: 15px;
    font-weight: 300;
  }
  .sidebar-error {
    margin-top: 8px;
    font-size: var(--font-size-small);
    line-height: 14px;
    padding: 0 4px;
  }
  .tree-chevron {
    transition: transform 0.15s ease;
    transform: rotate(-90deg);
    font-size: var(--font-size-small);
  }
  .tree-chevron.expanded {
    transform: rotate(0deg);
  }
  .sidebar-menu.nested {
    padding: 0;
    margin: 0;
    list-style: none;
  }
  .spin {
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
