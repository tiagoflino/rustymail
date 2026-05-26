<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import { sendNotification, isPermissionGranted, requestPermission, onAction } from "@tauri-apps/plugin-notification";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { checkForUpdates, setupPeriodicUpdateCheck } from "$lib/utils/updater";
  import { analyzeLinkSafety, type LinkAnalysis } from "$lib/utils/linkSafety";
  import { onMount, onDestroy } from "svelte";
  import { isAuthenticated } from "$lib/stores/auth";
  import {
    threads,
    isSyncing,
    lastSyncError,
    type LocalThread,
  } from "$lib/stores/threads";
  import {
    selectedThreadId,
    currentMessages,
    isMessagesLoading,
    messagesError,
    selectedThreadIds,
    clearSelection,
    selectAll,
    type LocalMessage,
  } from "$lib/stores/messages";
  import { writable, get } from "svelte/store";
  import {
    formatLabelName,
  } from "$lib/components/icons";
  import { availableSuperstars } from "$lib/stores/superstars";
  import { getNextStar } from "$lib/components/starIcons";
  import ImapAccountForm from "$lib/components/ImapAccountForm.svelte";
  import Settings from "$lib/components/Settings.svelte";
  import Compose from "$lib/components/Compose.svelte";
  import FullCalendar from "$lib/components/FullCalendar.svelte";
  import Subscriptions from "$lib/components/Subscriptions.svelte";
  import FeedView from "$lib/components/FeedView.svelte";
  import Contacts from "$lib/components/Contacts.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import ThreadList from "$lib/components/ThreadList.svelte";
  import MessageDetail from "$lib/components/MessageDetail.svelte";
  import LinkSafetyDialog from "$lib/components/LinkSafetyDialog.svelte";
  import CommandPalette from "$lib/components/CommandPalette.svelte";
  import UpdateModal from "$lib/components/UpdateModal.svelte";
  import LabelPicker from "$lib/components/LabelPicker.svelte";
  import SnoozePopover from "$lib/components/SnoozePopover.svelte";
  import { shortcutManager } from "$lib/shortcut-manager";
  import { addToast } from "$lib/stores/toast";
  import { pendingUpdate } from "$lib/utils/updater";
  import {
    formatTime,
    prepareQuotedHtml,
  } from "$lib/utils/formatters.js";
  import { snoozeOptions } from "$lib/utils/snooze";

  interface LocalLabel {
    id: string;
    name: string;
    type: string;
    unread_count: number;
    threads_total: number;
    threads_unread: number;
  }
  interface AccountInfo {
    id: string;
    email: string;
    display_name: string;
    avatar_url: string;
    is_active: boolean;
    credential_source?: string;
    provider_type?: string;
  }

  interface ProviderCapabilities {
    has_labels: boolean;
    has_categories: boolean;
    has_superstars: boolean;
    has_important: boolean;
    has_server_threading: boolean;
    has_drive_upload: boolean;
    has_calendar: boolean;
  }

  const labels = writable<LocalLabel[]>([]);
  const selectedLabelId = writable<string>("INBOX");
  const searchQuery = writable<string>("");
  const isSearching = writable<boolean>(false);
  const selectedCategory = writable<string>("primary");

  let activeAccount = $state<AccountInfo | null>(null);
  let allAccounts = $state<AccountInfo[]>([]);
  let capabilities = $state<ProviderCapabilities>({
    has_labels: false,
    has_categories: false,
    has_superstars: false,
    has_important: false,
    has_server_threading: false,
    has_drive_upload: false,
    has_calendar: false,
  });
  let showSettings = $state(false);
  let showAddAccount = $state(false);
  let isLoading = $state(false);
  let isLoadingThreads = $state(false);
  let showCompose = $state(false);
  let showCommandPalette = $state(false);
  let viewMode = $state<"mail" | "calendar" | "subscriptions" | "contacts">("mail");
  let imapConnectionStates = $state<Record<string, string>>({});
  let snoozePopoverOpen = $state(false);
  let batchSnoozeOpen = $state(false);
  let labelPickerOpen = $state(false);
  let snoozedCount = $state(0);
  let scheduledCount = $state(0);
  let hasSubscriptions = $state(false);

  let isMacOS = $state(false);
  let sidebarCollapsed = $state(false);
  let density = $state("default");
  let readingPane = $state("right");
  let linkBehavior = $state("ask");
  let pendingLinkUrl = $state<string | null>(null);
  let pendingLinkAnalysis = $state<LinkAnalysis | null>(null);
  const iframeWindows = new Map<Window, HTMLIFrameElement>();

  let accountProviderMap = $derived(
    Object.fromEntries(allAccounts.map(a => [a.id, a.provider_type ?? 'gmail']))
  );

  let threadListRef = $state<ThreadList>();

  function handleIframeMessage(event: MessageEvent) {
    const data = event.data;
    if (!data || typeof data !== 'object') return;

    if (data.type === 'rustymail-link' && typeof data.url === 'string') {
      const url: string = data.url;
      if (url.startsWith('mailto:')) return;
      if (linkBehavior === 'disable') return;
      const sender = $currentMessages?.[0]?.sender ?? "";
      if (linkBehavior === 'ask') {
        pendingLinkUrl = url;
        pendingLinkAnalysis = analyzeLinkSafety(url, sender);
        return;
      }
      invoke('open_external_url', { url });
    }
  }

  function handleIframeLoad(iframe: HTMLIFrameElement) {
    if (iframe.contentWindow) {
      iframeWindows.set(iframe.contentWindow, iframe);
    }
  }

  function dismissLinkDialog() {
    pendingLinkUrl = null;
    pendingLinkAnalysis = null;
  }

  function confirmOpenLink() {
    if (pendingLinkUrl) {
      invoke('open_external_url', { url: pendingLinkUrl });
    }
    dismissLinkDialog();
  }

  let appState = $state<"loading" | "onboarding" | "authenticated">("loading");
  let onboardingView = $state<'providers' | 'imap'>('providers');
  let onboardingLoading = $state<string>('');
  let showGoogleOnboardOptions = $state(false);
  let showOnboardByoFields = $state(false);
  let onboardByoClientId = $state('');
  let onboardByoClientSecret = $state('');
  let onboardByoIdError = $derived(
    onboardByoClientId && !onboardByoClientId.trim().endsWith('.apps.googleusercontent.com')
      ? 'Must end with .apps.googleusercontent.com'
      : ''
  );
  let onboardByoSecretError = $derived(
    onboardByoClientSecret && onboardByoClientSecret.trim().length < 10
      ? 'Secret seems too short'
      : ''
  );
  let onboardByoValid = $derived(
    onboardByoClientId.trim().endsWith('.apps.googleusercontent.com')
    && onboardByoClientSecret.trim().length >= 10
    && !onboardByoIdError && !onboardByoSecretError
  );

  let currentPage = $state(0);
  let threadsPerPage = $state(100);
  let totalCount = $state(0);
  let hasMoreRemote = $state(true);
  let gmailTotal = $state<number | null>(null);
  let isBackgroundFilling = $state(false);
  let bgSyncDone = $state(false);
  let globalSyncInterval: ReturnType<typeof setInterval> | null = null;
  let currentBgInterval: ReturnType<typeof setInterval> | null = null;
  const labelLastSyncMap: Record<string, number> = {};
  const categoryLastSyncMap: Record<string, number> = {};
  const CATEGORY_SYNC_INTERVAL = 300000;
  let syncLock = false;
  let unifiedIndicatorSetting = $state("avatar");
  let isUnifiedEnabledSetting = $state(true);

  let composeKey = $state(0);
  let composeProps = $state({
    initialTo: "",
    initialCc: "",
    initialSubject: "",
    initialBodyHTML: "",
    threadId: null as string | null,
    inReplyTo: null as string | null,
    references: null as string | null,
    initialDraftId: null as string | null,
    accountId: null as string | null,
  });

  async function openCompose(props: Partial<typeof composeProps> = {}) {
    // If replying to a thread, find existing draft to update instead of creating a new one
    let draftId = props.initialDraftId ?? null;
    if (!draftId && props.threadId) {
      const draftMsg = $currentMessages.find(m => m.is_draft);
      if (draftMsg) {
        try {
          const existingId = await invoke("get_draft_id_by_message_id", { messageId: draftMsg.id }) as string | null;
          if (existingId) draftId = existingId;
        } catch (_) {}
      }
    }

    composeProps = {
      initialTo: "",
      initialCc: "",
      initialSubject: "",
      initialBodyHTML: "",
      threadId: null,
      inReplyTo: null,
      references: null,
      initialDraftId: draftId,
      accountId: null,
      ...props,
    };
    composeKey++;
    showCompose = true;
  }

  function handleCommandAction(id: string) {
    switch (id) {
      case 'compose': showCompose = true; break;
      case 'sync': performSync(true); break;
      case 'settings': showSettings = true; break;
      case 'theme': 
        cycleTheme();
        break;
      case 'sidebar': toggleSidebar(); break;
      case 'view_mail': viewMode = 'mail'; break;
      case 'view_calendar': viewMode = 'calendar'; break;
      case 'view_subscriptions': viewMode = 'subscriptions'; break;
      case 'nav_inbox': selectLabel('INBOX'); break;
      case 'nav_sent': selectLabel('SENT'); break;
      case 'nav_drafts': selectLabel('DRAFT'); break;
      case 'nav_trash': selectLabel('TRASH'); break;
      case 'nav_unified_inbox': selectLabel('UNIFIED_INBOX'); break;
      case 'nav_unified_sent': selectLabel('UNIFIED_SENT'); break;
      case 'nav_unified_drafts': selectLabel('UNIFIED_DRAFT'); break;
      case 'nav_unified_trash': selectLabel('UNIFIED_TRASH'); break;
      case 'snooze_later_today':
      case 'snooze_tomorrow':
      case 'snooze_next_week':
        handleSnoozeFromPalette(id);
        break;
      default:
        if (id.startsWith('switch_account_')) {
          const accId = id.replace('switch_account_', '');
          switchAccount(accId);
        }
        break;
    }
  }

  type ThemeMode = "system" | "light" | "dark";
  let themeMode: ThemeMode = $state("system");
  const iconSun =
    '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>';
  const iconMoon =
    '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>';
  const iconSidebarCollapse = '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/><polyline points="14 9 11 12 14 15"/></svg>';
  const iconSidebarExpand = '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/><polyline points="13 9 16 12 13 15"/></svg>';

  function applyTheme(mode: ThemeMode, persist = true) {
    themeMode = mode;
    const root = document.documentElement;
    if (mode === "light") root.setAttribute("data-theme", "light");
    else if (mode === "dark") root.setAttribute("data-theme", "dark");
    else root.removeAttribute("data-theme");
    localStorage.setItem("rustymail-theme", mode);
    if (persist) {
      invoke("update_setting", { key: "theme", value: mode }).catch(() => {});
    }
  }
  function cycleTheme() {
    const isDark = themeMode === "dark" ||
      (themeMode === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
    applyTheme(isDark ? "light" : "dark");
  }
  function toggleSidebar() {
    sidebarCollapsed = !sidebarCollapsed;
    localStorage.setItem("rustymail-sidebar-collapsed", sidebarCollapsed ? "1" : "0");
  }
  let themeIcon = $derived((() => {
    const m: string = themeMode;
    const isDark = m === "dark" || (m === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
    return isDark ? iconSun : iconMoon;
  })());
  let themeLabel = $derived((() => {
    const m: string = themeMode;
    const isDark = m === "dark" || (m === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
    return isDark ? "Light mode" : "Dark mode";
  })());

  async function login() {
    try {
      isLoading = true;
      await invoke("authenticate_gmail");
      isAuthenticated.set(true);
      await refreshAccountState();
      appState = "authenticated";
      await performSync(true);
    } catch (e: any) {
      console.error(e);
      addToast(String(e), "error", 6000);
      isLoading = false;
    }
  }

  async function refreshAccountState() {
    try {
      const status: {
        authenticated: boolean;
        active_account: AccountInfo | null;
        accounts: AccountInfo[];
      } = await invoke("check_auth_status");
      if (status.authenticated && status.active_account) {
        isAuthenticated.set(true);
        activeAccount = status.active_account;
        allAccounts = status.accounts;
        try {
          const caps = await invoke("get_provider_capabilities", { accountId: status.active_account.id }) as ProviderCapabilities | null;
          if (caps && typeof caps.has_labels === 'boolean') {
            capabilities = caps;
          }
        } catch {
          // defaults are already Gmail-like
        }
        appState = "authenticated";
      } else {
        appState = allAccounts.length > 0 ? "authenticated" : "onboarding";
      }
    } catch (e) {
      appState = "onboarding";
    }
  }

  async function performSync(isManual = false) {
    if (syncLock) return;
    syncLock = true;

    const syncLabelId = get(selectedLabelId) || "INBOX";
    const isUnified = syncLabelId.startsWith("UNIFIED_");
    const realLabelId = isUnified ? syncLabelId.replace("UNIFIED_", "") : syncLabelId;

    const isSnoozedView = syncLabelId === "SNOOZED" || syncLabelId === "UNIFIED_SNOOZED" || syncLabelId === "SCHEDULED";

    try {
      await checkSnoozedThreads();
      await checkScheduledSends();
      await refreshScheduledCount();
      await loadSubscriptionCount();

      // Snoozed/Scheduled are virtual labels — skip Gmail sync, just reload local data
      if (isSnoozedView) {
        await loadThreads(true);
        return;
      }

      isSyncing.set(true);
      lastSyncError.set(null);

      let allNewMessageIds: string[] = [];
      let allNewThreadIds: string[] = [];

      if (isUnified) {
        // Sync each account sequentially
        for (const acc of allAccounts) {
          try {
            const result = await invoke<{ new_message_ids: string[]; new_thread_ids: string[] }>("sync_gmail_data", {
              labelId: realLabelId,
              accountId: acc.id,
            });
            allNewMessageIds.push(...result.new_message_ids);
            allNewThreadIds.push(...result.new_thread_ids);
          } catch (e) {
            console.warn(`[UnifiedSync] Failed to sync account ${acc.id}:`, e);
          }
        }
      } else {
        const result = await invoke<{ new_message_ids: string[]; new_thread_ids: string[] }>("sync_gmail_data", { labelId: realLabelId, accountId: null });
        allNewMessageIds = result.new_message_ids;
        allNewThreadIds = result.new_thread_ids;
      }

      if (get(selectedLabelId) !== syncLabelId) return;

      if (!get(searchQuery)) {
        if (isManual) {
          currentPage = 0;
          await loadThreads(true);
        } else {
          await loadThreads(false, true);
        }
      }

      await loadLabels();

      if (isManual) {
        pollBackgroundSync();
      }

      // Desktop notifications for new messages (background syncs only)
      if (!isManual && allNewMessageIds.length > 0) {
        try {
          const enabled = await invoke<string>("get_setting", { key: "notifications_enabled" });
          if (enabled === "true") {
            const count = allNewMessageIds.length;
            const preview = await invoke<string>("get_setting", { key: "notifications_preview" });
            let title = "Rustymail";
            let body = count === 1 ? "You have a new message" : `You have ${count} new messages`;

            if (preview === "true") {
              try {
                const previews = await invoke<Array<{ sender: string; subject: string }>>(
                  "get_message_previews",
                  { threadIds: allNewThreadIds }
                );
                if (previews.length > 0) {
                  const latest = previews[0];
                  const sender = latest.sender.replace(/<[^>]+>/g, "").trim();
                  body = `${sender}: ${latest.subject}`;
                  if (count > 1) {
                    title = `Rustymail - ${count} new messages`;
                  }
                }
              } catch (_) {}
            }

            sendNotification({ title, body });
          }
        } catch (_) {}
      }
    } catch (e) {
      lastSyncError.set(String(e));
      addToast("Sync failed — showing cached data", "error", 4000);
    } finally {
      isSyncing.set(false);
      syncLock = false;
    }
  }

  async function pollBackgroundSync() {
    bgSyncDone = false;
    if (currentBgInterval) clearInterval(currentBgInterval);

    currentBgInterval = setInterval(async () => {
      try {
        await loadLabels();
        const progress: { total: number; hydrated: number } = await invoke(
          "get_hydration_progress",
        );
        if (progress.total > 0 && progress.hydrated >= progress.total) {
          bgSyncDone = true;
          if (currentBgInterval) clearInterval(currentBgInterval);
          currentBgInterval = null;
        }
      } catch (_) {
        if (currentBgInterval) clearInterval(currentBgInterval);
        currentBgInterval = null;
      }
    }, 3000);

    setTimeout(() => {
      bgSyncDone = true;
      if (currentBgInterval) clearInterval(currentBgInterval);
      currentBgInterval = null;
    }, 120000);
  }

  async function checkAndSetupSync() {
    if (globalSyncInterval) {
      clearInterval(globalSyncInterval);
      globalSyncInterval = null;
    }
    try {
      const freqStr = await invoke<string>("get_setting", {
        key: "sync_frequency",
      });
      console.log("[SyncSetup] sync_frequency setting:", freqStr);
      if (freqStr && freqStr !== "manual") {
        const secs = Math.max(parseInt(freqStr) || 30, 30);
        console.log("[SyncSetup] Setting sync interval to", secs, "seconds");
        globalSyncInterval = setInterval(async () => {
          if (!syncLock) await performSync(false);
        }, secs * 1000);
      }
    } catch (e) {
      console.error("Could not fetch sync freq", e);
    }
  }

  let isLabelFetching = $state(false);

  async function updateThreadCount() {
    const invocationLabelId = get(selectedLabelId) || null;
    const category = $selectedLabelId === "INBOX" && capabilities.has_categories ? get(selectedCategory) : null;
    const isUnified = invocationLabelId?.startsWith("UNIFIED_") ?? false;
    const realLabelId = isUnified ? (invocationLabelId ?? "").replace("UNIFIED_", "") : invocationLabelId;
    const accountIds = isUnified ? allAccounts.map(a => a.id) : [];

    if (category) {
      const inboxLabel = $labels.find((l) => l.id === "INBOX");
      gmailTotal = inboxLabel?.threads_total ?? null;
      if (gmailTotal === 0) gmailTotal = null;

      try {
        const result: { count: number; has_more_remote: boolean } = isUnified
          ? await invoke("get_unified_thread_count", {
              accountIds,
              labelId: realLabelId,
              category: category,
            })
          : await invoke("get_thread_count", {
              labelId: realLabelId,
              category: category,
            });
        totalCount = result.count;
        hasMoreRemote = result.has_more_remote || (gmailTotal !== null && result.count < gmailTotal);
      } catch (_) {}
    } else {
      const label = $labels.find((l) => l.id === invocationLabelId);
      gmailTotal = label?.threads_total ?? null;
      if (gmailTotal === 0) gmailTotal = null;

      try {
        const result: { count: number; has_more_remote: boolean } = isUnified
          ? await invoke("get_unified_thread_count", {
              accountIds,
              labelId: realLabelId,
              category: null,
            })
          : await invoke("get_thread_count", {
              labelId: realLabelId,
              category: null,
            });
        totalCount = result.count;
        hasMoreRemote = result.has_more_remote || (gmailTotal !== null && result.count < gmailTotal);
      } catch (_) {}
    }
  }

  async function loadThreads(reset = false, silent = false) {
    if (isLoadingThreads && !silent) return;
    if (!silent) isLoadingThreads = true;

    const invocationLabelId = get(selectedLabelId) || null;
    const isUnified = invocationLabelId?.startsWith("UNIFIED_") ?? false;
    const realLabelId = isUnified ? (invocationLabelId ?? "").replace("UNIFIED_", "") : invocationLabelId;
    const accountIds = isUnified ? allAccounts.map(a => a.id) : [];

    if (reset && !silent) {
      if (get(searchQuery) && !isLabelFetching) return;
      currentPage = 0;
      threads.set([]);
    }

    if ($selectedLabelId === "SNOOZED" || $selectedLabelId === "UNIFIED_SNOOZED") {
      try {
        const snoozedInfo: any[] = await invoke("get_snoozed_threads");
        const snoozedList = snoozedInfo.map(s => ({
          id: s.thread_id,
          subject: s.subject,
          sender: s.sender,
          snippet: `Snoozed until ${new Date(s.snoozed_until * 1000).toLocaleString()}`,
          date: new Date(s.created_at * 1000).toISOString(),
          unread: 0,
          starred: false,
          star_type: null,
          important: false,
          labels: ["SNOOZED"],
          message_count: 0,
          has_attachments: false,
          account_id: s.account_id,
          history_id: "",
          internal_date: s.created_at,
        }));
        threads.set(snoozedList);
        snoozedCount = snoozedList.length;
        totalCount = snoozedList.length;
        hasMoreRemote = false;
      } catch (e) {
        console.error("Failed to load snoozed threads", e);
        threads.set([]);
        totalCount = 0;
        hasMoreRemote = false;
      } finally {
        isLoadingThreads = false;
      }
      return;
    }

    if ($selectedLabelId === "SCHEDULED") {
      try {
        const scheduled: any[] = await invoke("get_scheduled_sends");
        const scheduledList = scheduled.map(s => ({
          id: `scheduled-${s.id}`,
          subject: s.subject,
          sender: s.to_recipients,
          snippet: `Scheduled for ${new Date(s.send_at * 1000).toLocaleString()}`,
          date: new Date(s.created_at * 1000).toISOString(),
          unread: 0,
          starred: false,
          star_type: null,
          important: false,
          labels: ["SCHEDULED"],
          message_count: 0,
          internal_date: s.send_at * 1000,
          history_id: "",
          has_attachments: false,
          account_id: s.account_id,
        }));
        threads.set(scheduledList);
        scheduledCount = scheduledList.length;
        totalCount = scheduledList.length;
        hasMoreRemote = false;
      } catch (e) {
        console.error("Failed to load scheduled sends", e);
        threads.set([]);
        totalCount = 0;
        hasMoreRemote = false;
      } finally {
        isLoadingThreads = false;
      }
      return;
    }

    try {
      const category = $selectedLabelId === "INBOX" && capabilities.has_categories ? get(selectedCategory) : null;
      const offset = currentPage * threadsPerPage;
      const fetched = isUnified
        ? (await invoke("get_unified_threads", {
            accountIds,
            labelId: realLabelId,
            category: category,
            offset: offset,
            limit: threadsPerPage,
          })) as LocalThread[]
        : (await invoke("get_threads", {
            labelId: realLabelId,
            category: category,
            offset: offset,
            limit: threadsPerPage,
          })) as LocalThread[];

      const starredThreads = fetched.filter(t => t.starred);
      if (starredThreads.length > 0) console.log("[DEBUG loadThreads] starred threads:", starredThreads.map(t => ({ id: t.id, starred: t.starred, star_type: t.star_type })));

      if ((get(selectedLabelId) || null) !== invocationLabelId) return;

      if (reset && fetched.length === 0 && invocationLabelId && !silent) {
        isLabelFetching = true;
        try {
          if (category) {
            if (isUnified) {
              for (const accId of accountIds) await invoke("fetch_category_threads", { category, accountId: accId });
            } else {
              await invoke("fetch_category_threads", { category, accountId: null });
            }
          } else if (realLabelId) {
            if (isUnified) {
              for (const accId of accountIds) await invoke("fetch_label_threads", { labelId: realLabelId, accountId: accId });
            } else {
              await invoke("fetch_label_threads", { labelId: realLabelId, accountId: null });
            }
          }

          if ((get(selectedLabelId) || null) !== invocationLabelId) return;

          if (invocationLabelId)
            labelLastSyncMap[invocationLabelId] = Date.now();
          const retried = isUnified
            ? (await invoke("get_unified_threads", {
                accountIds,
                labelId: realLabelId,
                category: category,
                offset: 0,
                limit: threadsPerPage,
              })) as LocalThread[]
            : (await invoke("get_threads", {
                labelId: realLabelId,
                category: category,
                offset: 0,
                limit: threadsPerPage,
              })) as LocalThread[];

          if ((get(selectedLabelId) || null) !== invocationLabelId) return;

          threads.set(retried);
        } catch (_) {
        } finally {
          isLabelFetching = false;
        }
        await updateThreadCount();
        if (get(threads).length < threadsPerPage && hasMoreRemote) {
          isBackgroundFilling = true;
          backgroundFillPage();
        }
        return;
      }

      if (silent) {
        threads.update((current) => {
          const fetchedMap = new Map(fetched.map((t) => [t.id, t]));
          const kept: LocalThread[] = [];

          for (const t of current) {
            const fresh = fetchedMap.get(t.id);
            if (fresh) {
              Object.assign(t, fresh);
              kept.push(t);
              fetchedMap.delete(t.id);
            }
          }

          const newOnes = [...fetchedMap.values()];

          if (newOnes.length === 0 && kept.length === current.length) return kept;
          return [...newOnes, ...kept].sort((a, b) => (b.internal_date || 0) - (a.internal_date || 0));
        });
      } else {
        threads.set(fetched);
      }

      await updateThreadCount();

      if (!silent && get(threads).length < threadsPerPage && hasMoreRemote) {
        isBackgroundFilling = true;
        backgroundFillPage();
      }
    } catch (e) {
      console.error("Failed to load threads", e);
    } finally {
      if (!silent) isLoadingThreads = false;
    }
  }

  let backgroundFillGeneration = 0;

  async function backgroundFillPage() {
    isBackgroundFilling = true;
    const gen = ++backgroundFillGeneration;

    const invocationLabelId = get(selectedLabelId) || null;
    const category = $selectedLabelId === "INBOX" && capabilities.has_categories ? get(selectedCategory) : null;
    const targetPage = currentPage;
    const isUnified = invocationLabelId?.startsWith("UNIFIED_") ?? false;
    const realLabelId = isUnified ? (invocationLabelId ?? "").replace("UNIFIED_", "") : invocationLabelId;
    const accountIds = isUnified ? allAccounts.map(a => a.id) : [];

    try {
      let more = true;
      while (more && get(threads).length < threadsPerPage) {
        if (gen !== backgroundFillGeneration) break;
        if ((get(selectedLabelId) || null) !== invocationLabelId) break;
        if (currentPage !== targetPage) break;

        if (category) {
          if (isUnified) {
            more = false;
            for (const accId of accountIds) {
              const m = await invoke("fetch_category_threads", { category, accountId: accId }) as boolean;
              if (m) more = true;
            }
          } else {
            more = await invoke("fetch_category_threads", { category, accountId: null }) as boolean;
          }
        } else if (realLabelId) {
          if (isUnified) {
            more = false;
            for (const accId of accountIds) {
              const m = await invoke("fetch_label_threads", { labelId: realLabelId, accountId: accId }) as boolean;
              if (m) more = true;
            }
          } else {
            more = await invoke("fetch_label_threads", { labelId: realLabelId, accountId: null }) as boolean;
          }
        } else {
          break;
        }

        if (gen !== backgroundFillGeneration) break;

        const offset = targetPage * threadsPerPage;
        const fetched = isUnified
          ? (await invoke("get_unified_threads", {
              accountIds,
              labelId: realLabelId,
              category: category,
              offset: offset,
              limit: threadsPerPage,
            })) as LocalThread[]
          : (await invoke("get_threads", {
              labelId: realLabelId,
              category: category,
              offset: offset,
              limit: threadsPerPage,
            })) as LocalThread[];

        if (gen !== backgroundFillGeneration) break;

        threads.set(fetched);
        await updateThreadCount();
      }

      if (!more && category && get(threads).length < threadsPerPage && gen === backgroundFillGeneration) {
        let inboxMore = true;
        while (inboxMore && get(threads).length < threadsPerPage) {
          if (gen !== backgroundFillGeneration) break;

          if (isUnified) {
            inboxMore = false;
            for (const accId of accountIds) {
              const m = await invoke("fetch_label_threads", { labelId: "INBOX", accountId: accId }) as boolean;
              if (m) inboxMore = true;
            }
          } else {
            inboxMore = await invoke("fetch_label_threads", { labelId: "INBOX", accountId: null }) as boolean;
          }

          if (gen !== backgroundFillGeneration) break;

          const offset = targetPage * threadsPerPage;
          const fetched = isUnified
            ? (await invoke("get_unified_threads", {
                accountIds,
                labelId: realLabelId,
                category: category,
                offset: offset,
                limit: threadsPerPage,
              })) as LocalThread[]
            : (await invoke("get_threads", {
                labelId: realLabelId,
                category: category,
                offset: offset,
                limit: threadsPerPage,
              })) as LocalThread[];

          threads.set(fetched);
          await updateThreadCount();
        }
      }
      if (more && gen === backgroundFillGeneration) {
        let prefetchCount = 0;
        const prefetchTarget = threadsPerPage;
        while (more && prefetchCount < prefetchTarget) {
          if (gen !== backgroundFillGeneration) break;
          if (category) {
            if (isUnified) {
              let anyMore = false;
              for (const accId of accountIds) {
                const m = await invoke("fetch_category_threads", { category, accountId: accId }) as boolean;
                if (m) anyMore = true;
              }
              more = anyMore;
            } else {
              more = await invoke("fetch_category_threads", { category, accountId: null }) as boolean;
            }
          } else if (realLabelId) {
            if (isUnified) {
              let anyMore = false;
              for (const accId of accountIds) {
                const m = await invoke("fetch_label_threads", { labelId: realLabelId, accountId: accId }) as boolean;
                if (m) anyMore = true;
              }
              more = anyMore;
            } else {
              more = await invoke("fetch_label_threads", { labelId: realLabelId, accountId: null }) as boolean;
            }
          } else {
            break;
          }
          prefetchCount += 30;
        }
        await updateThreadCount();
      }
    } catch (_) {}

    isBackgroundFilling = false;
  }

  async function goToFirstPage() {
    if (currentPage === 0) return;
    backgroundFillGeneration++;
    isBackgroundFilling = false;
    currentPage = 0;
    threadListRef?.resetScroll();
    await loadThreads();
  }

  async function goToNextPage() {
    backgroundFillGeneration++;
    isBackgroundFilling = false;
    currentPage++;
    threadListRef?.resetScroll();
    await loadThreads();
  }

  async function goToPrevPage() {
    if (currentPage <= 0) return;
    backgroundFillGeneration++;
    isBackgroundFilling = false;
    currentPage--;
    threadListRef?.resetScroll();
    await loadThreads();
  }

  async function loadLabels() {
    try {
      const fetched: LocalLabel[] = await invoke("get_labels");
      labels.set(fetched);
      updateTrayUnread(fetched);
    } catch (e) {
      console.error("Failed to load labels", e);
    }
  }

  function updateTrayUnread(labelList: LocalLabel[]) {
    const inbox = labelList.find((l) => l.id === "INBOX");
    const count = inbox?.unread_count ?? 0;
    invoke("update_tray_unread", { count }).catch(() => {});
    // Also set badge directly from frontend as backup
    getCurrentWindow().setBadgeCount(count > 0 ? count : undefined).catch(() => {});
  }

  async function selectLabel(labelId: string) {
    viewMode = "mail";
    const prev = $selectedLabelId;
    const isReselect = prev === labelId;
    selectedLabelId.set(labelId);
    selectedThreadId.set(null);
    currentMessages.set([]);
    threadListRef?.clearSearchInput();
    threadListRef?.resetScroll();
    searchQuery.set("");
    currentPage = 0;
    hasMoreRemote = true;

    if (labelId === "INBOX") {
      selectedCategory.set("primary");
    } else {
      selectedCategory.set("primary");
    }

    if (!isReselect) {
      threads.set([]);
    }

    // FEED is a virtual label — FeedView handles its own data loading
    if (labelId === "FEED") return;

    await loadThreads(true);

    // SNOOZED and SCHEDULED are virtual labels — no Gmail sync needed
    if (labelId === "SNOOZED" || labelId === "UNIFIED_SNOOZED" || labelId === "SCHEDULED") return;

    const lastSync = labelLastSyncMap[labelId] || 0;
    if (isReselect || Date.now() - lastSync > 300000) {
      const isUnifiedLabel = labelId.startsWith("UNIFIED_");
      const realLabel = isUnifiedLabel ? labelId.replace("UNIFIED_", "") : labelId;
      isSyncing.set(true);

      const syncPromise = isUnifiedLabel
        ? (async () => {
            for (const acc of allAccounts) {
              await invoke("fetch_label_threads", { labelId: realLabel, accountId: acc.id }).catch(() => {});
            }
          })()
        : invoke("fetch_label_threads", { labelId: realLabel, accountId: null });

      syncPromise
        .then(() => { labelLastSyncMap[labelId] = Date.now(); })
        .catch((e) => { addToast(`Sync failed: ${e}`, "error", 4000); })
        .finally(async () => {
          isSyncing.set(false);
          await loadThreads(false, true);
        });
    }
  }

  async function selectCategory(category: string) {
    if ($selectedCategory === category) return;
    selectedCategory.set(category);
    threadListRef?.resetScroll();
    currentPage = 0;
    hasMoreRemote = true;
    threads.set([]);

    await loadThreads(true);

    const needsSync = !categoryLastSyncMap[category] ||
                       Date.now() - categoryLastSyncMap[category] > CATEGORY_SYNC_INTERVAL;

    if (needsSync) {
      isSyncing.set(true);
      invoke("fetch_category_threads", { category, accountId: null })
        .then(() => { categoryLastSyncMap[category] = Date.now(); })
        .catch((e) => { addToast(`Sync failed: ${e}`, "error", 4000); })
        .finally(async () => {
          isSyncing.set(false);
          await loadThreads(false, true);
        });
    }
  }

  async function handleSearch(query: string) {
    if (!query) {
      searchQuery.set("");
      await loadThreads(true);
      return;
    }
    searchQuery.set(query);
    isSearching.set(true);
    hasMoreRemote = false;
    try {
      await invoke("save_recent_search", { query });
      const results: LocalThread[] = (await invoke("search_messages", {
        query,
      })) as LocalThread[];
      threads.set(results);
    } catch (e) {
      console.error("Search failed", e);
    } finally {
      isSearching.set(false);
    }
  }

  function clearSearch() {
    searchQuery.set("");
    loadThreads(true);
  }

  async function handleSelectSubscription(senderEmail: string) {
    viewMode = "mail";
    await handleSearch(`from:${senderEmail}`);
  }

  async function executeAction(action: "archive" | "trash" | "unread" | "untrash" | string) {
    const threadId = $selectedThreadId;
    if (!threadId) return;

    if (
      action === "trash" &&
      showCompose &&
      composeProps.threadId === threadId
    ) {
      showCompose = false;
      if (composeProps.initialDraftId) {
        try {
          await invoke("delete_draft", {
            draftId: composeProps.initialDraftId,
          });
          addToast("Draft discarded.", "info");
        } catch (e) {
          addToast(`Failed to discard draft: ${e}`, "error", 5000);
        }
      } else {
        addToast("Draft discarded.", "info");
      }
      return;
    }

    const currentList = $threads;

    if (action === "unsnooze") {
      const previousList = currentList;
      threads.set(currentList.filter((t) => t.id !== threadId));
      selectedThreadId.set(null);
      currentMessages.set([]);
      try {
        await invoke("unsnooze_thread", { threadId: threadId });
        addToast("Conversation unsnoozed.", "info");
        if ($selectedLabelId === "SNOOZED" || $selectedLabelId === "UNIFIED_SNOOZED") {
          snoozedCount = Math.max(0, snoozedCount - 1);
        }
        if ($selectedLabelId === "INBOX" || $selectedLabelId === "UNIFIED_INBOX") {
          await loadThreads(true);
        }
      } catch (e) {
        console.error("unsnooze failed", e);
        addToast(`Failed to unsnooze: ${e}`, "error", 5000);
        threads.set(previousList);
      }
      return;
    }

    if (action.startsWith("snooze:")) {
      const until = Number(action.split(":")[1]);
      const previousList = currentList;
      threads.set(currentList.filter((t) => t.id !== threadId));
      selectedThreadId.set(null);
      currentMessages.set([]);
      try {
        await invoke("snooze_thread", { threadId: threadId, snoozedUntil: until });
        snoozedCount += 1;
        addToast("Conversation snoozed.", "info", 6000, {
          label: "Undo",
          onClick: async () => {
            try {
              await invoke("unsnooze_thread", { threadId: threadId });
              snoozedCount = Math.max(0, snoozedCount - 1);
              await loadThreads(true);
            } catch (e) {
              console.error("Undo snooze failed", e);
              addToast(`Failed to undo snooze: ${e}`, "error", 5000);
            }
          },
        });
      } catch (e) {
        console.error("snooze failed", e);
        addToast(`Failed to snooze: ${e}`, "error", 5000);
        threads.set(previousList);
      }
      return;
    }

    if (action === "archive" || action === "trash" || action === "untrash") {
      threads.set(currentList.filter((t) => t.id !== threadId));
      selectedThreadId.set(null);
      currentMessages.set([]);
    } else if (action === "unread") {
      threads.set(
        currentList.map((t) => (t.id === threadId ? { ...t, unread: 1 } : t)),
      );
      selectedThreadId.set(null);
      currentMessages.set([]);
    }
    try {
      if (action === "archive")
        await invoke("archive_thread", { threadId: threadId });
      else if (action === "trash") {
        if ($selectedLabelId === "DRAFT") {
          try {
            await invoke("delete_draft_by_thread", { threadId: threadId });
          } catch (e) {
            await invoke("move_thread_to_trash", { threadId: threadId });
          }
        } else {
          await invoke("move_thread_to_trash", { threadId: threadId });
          delete labelLastSyncMap["TRASH"];
          addToast("Conversation moved to Trash.", "info");
        }
      } else if (action === "untrash") {
        await invoke("untrash_thread", { threadId: threadId });
        addToast("Conversation restored from Trash.", "success");
      } else if (action === "unread")
        await invoke("mark_thread_read_status", {
          threadId: threadId,
          isRead: false,
        });
    } catch (e) {
      console.error(`${action} failed`, e);
      addToast(`Failed to ${action}: ${e}`, "error", 5000);
      threads.set(currentList);
    }
  }

  const SYSTEM_LABEL_IDS = new Set([
    "INBOX", "SENT", "DRAFT", "TRASH", "SPAM", "STARRED", "IMPORTANT",
    "UNREAD", "CHAT", "VOICEMAIL", "SNOOZED",
  ]);

  let userLabels = $derived(
    $labels
      .filter(l => l.type === "user" && !l.id.startsWith("CATEGORY_") && !SYSTEM_LABEL_IDS.has(l.id))
      .map(l => ({ id: l.id, name: l.name }))
  );

  function forceRefreshThreads() {
    isLoadingThreads = false;
    loadThreads(true);
  }

  async function executeBatchAction(action: string, extraArgs?: any) {
    const ids = [...$selectedThreadIds];
    if (ids.length === 0) return;
    const idSet = new Set(ids);
    const previousList = [...$threads];

    clearSelection();

    // Optimistic removal for actions that remove threads from current view
    if (["archive", "trash", "restore", "snooze", "unsnooze", "movetolabel"].includes(action)) {
      threads.update(ts => ts.filter(t => !idSet.has(t.id)));
      selectedThreadId.set(null);
      currentMessages.set([]);
    }
    // Optimistic update for star/read (threads stay in list, just change state)
    if (action === "star") {
      threads.update(ts => ts.map(t => idSet.has(t.id) ? { ...t, starred: extraArgs, star_type: extraArgs ? (t.star_type || "YELLOW_STAR") : null } : t));
    }
    if (action === "read") {
      threads.update(ts => ts.map(t => idSet.has(t.id) ? { ...t, unread: extraArgs ? 0 : 1 } : t));
    }

    try {
      let result: any;
      switch (action) {
        case "archive":
          result = await invoke("batch_archive_threads", { threadIds: ids });
          if (result.succeeded > 0) {
            const archivedIds = ids.filter(id => !result.failed_ids.includes(id));
            addToast(`${result.succeeded} thread${result.succeeded > 1 ? 's' : ''} archived`, "info", 6000, {
              label: "Undo",
              onClick: async () => {
                await invoke("batch_move_to_label", { threadIds: archivedIds, addLabels: ["INBOX"], removeLabels: [] });
                forceRefreshThreads();
              }
            });
          }
          break;
        case "trash":
          result = await invoke("batch_trash_threads", { threadIds: ids });
          if (result.succeeded > 0) {
            const trashedIds = ids.filter(id => !result.failed_ids.includes(id));
            addToast(`${result.succeeded} thread${result.succeeded > 1 ? 's' : ''} moved to trash`, "info", 6000, {
              label: "Undo",
              onClick: async () => {
                for (const id of trashedIds) {
                  await invoke("untrash_thread", { threadId: id });
                }
                forceRefreshThreads();
              }
            });
          }
          break;
        case "restore":
          let restoreSucceeded = 0;
          const restoreFailed: string[] = [];
          for (const id of ids) {
            try {
              await invoke("untrash_thread", { threadId: id });
              restoreSucceeded++;
            } catch { restoreFailed.push(id); }
          }
          result = { succeeded: restoreSucceeded, failed_ids: restoreFailed };
          if (restoreSucceeded > 0) {
            addToast(`${restoreSucceeded} thread${restoreSucceeded > 1 ? 's' : ''} restored`, "success");
            delete labelLastSyncMap["INBOX"];
            delete labelLastSyncMap["UNIFIED_INBOX"];
          }
          break;
        case "read":
          result = await invoke("batch_mark_read_status", { threadIds: ids, isRead: extraArgs });
          if (result.succeeded > 0) addToast(`${result.succeeded} thread${result.succeeded > 1 ? 's' : ''} marked as ${extraArgs ? 'read' : 'unread'}`, "info");
          break;
        case "star":
          result = await invoke("batch_star_threads", { threadIds: ids, starred: extraArgs });
          if (result.succeeded > 0) addToast(`${result.succeeded} thread${result.succeeded > 1 ? 's' : ''} ${extraArgs ? 'starred' : 'unstarred'}`, "info");
          break;
        case "snooze":
          result = await invoke("batch_snooze_threads", { threadIds: ids, snoozedUntil: extraArgs });
          if (result.succeeded > 0) addToast(`${result.succeeded} thread${result.succeeded > 1 ? 's' : ''} snoozed`, "info");
          break;
        case "unsnooze":
          let unsnoozeSucceeded = 0;
          const unsnoozeFailed: string[] = [];
          for (const id of ids) {
            try {
              await invoke("unsnooze_thread", { threadId: id });
              unsnoozeSucceeded++;
            } catch { unsnoozeFailed.push(id); }
          }
          result = { succeeded: unsnoozeSucceeded, failed_ids: unsnoozeFailed };
          if (unsnoozeSucceeded > 0) {
            addToast(`${unsnoozeSucceeded} thread${unsnoozeSucceeded > 1 ? 's' : ''} unsnoozed`, "info");
            delete labelLastSyncMap["INBOX"];
            delete labelLastSyncMap["UNIFIED_INBOX"];
          }
          break;
        case "movetolabel":
          result = await invoke("batch_move_to_label", { threadIds: ids, addLabels: [extraArgs], removeLabels: ["INBOX"] });
          if (result.succeeded > 0) addToast(`${result.succeeded} thread${result.succeeded > 1 ? 's' : ''} moved`, "info");
          break;
      }
      if (result?.failed_ids?.length > 0) {
        addToast(`${result.failed_ids.length} thread${result.failed_ids.length > 1 ? 's' : ''} failed`, "error");
        // Restore optimistically removed threads on partial failure
        if (["archive", "trash", "restore", "snooze", "unsnooze", "movetolabel"].includes(action) && result.failed_ids.length > 0) {
          forceRefreshThreads();
        }
      }
    } catch (e: any) {
      addToast(`Batch action failed: ${e}`, "error");
      threads.set(previousList);
    }
  }

  async function checkSnoozedThreads() {
    try {
      const unsnoozed: string[] = await invoke("check_snoozed_threads");
      if (unsnoozed.length > 0) {
        console.log("[Snooze] Un-snoozed threads:", unsnoozed);
        snoozedCount = Math.max(0, snoozedCount - unsnoozed.length);
      }
    } catch (e) {
      console.error("[Snooze] check failed:", e);
    }
  }

  async function checkScheduledSends() {
    try {
      const sent: string[] = (await invoke("check_scheduled_sends")) ?? [];
      if (sent.length > 0) {
        for (const subject of sent) {
          addToast(`Scheduled email sent: ${subject}`, "success", 5000);
        }
        scheduledCount = Math.max(0, scheduledCount - sent.length);
      }
    } catch (e) {
      console.error("Failed to check scheduled sends:", e);
    }
  }

  async function refreshScheduledCount() {
    try {
      scheduledCount = await invoke("get_scheduled_count") as number;
    } catch {}
  }

  async function loadSubscriptionCount() {
    try {
      const subs = await invoke<Array<{ id: number; sender_email: string }>>("get_subscriptions", { accountId: null as string | null, status: "active" });
      hasSubscriptions = subs.length > 0;
    } catch {
      hasSubscriptions = false;
    }
  }

  function handleSnoozeFromPalette(id: string) {
    const map: Record<string, number> = { snooze_later_today: 0, snooze_tomorrow: 1, snooze_next_week: 2 };
    const index = map[id];
    if (index === undefined) return;
    executeAction("snooze:" + snoozeOptions[index].compute());
  }

  const STAR_CLICK_DELAY = 500;
  let lastStarClick = $state<Record<string, number>>({});

  async function cycleStar(threadId: string, currentStarType: string | null) {
    const available = get(availableSuperstars);
    if (!available.length) return;

    const now = Date.now();
    const lastClick = lastStarClick[threadId] ?? 0;
    lastStarClick[threadId] = now;
    const isQuickClick = now - lastClick < STAR_CLICK_DELAY && lastClick > 0;

    let nextStar: string | null;
    if (!currentStarType) {
      nextStar = available[0];
    } else if (isQuickClick) {
      nextStar = getNextStar(currentStarType, available);
    } else {
      nextStar = null;
    }

    const newStarred = nextStar !== null;
    const currentList = get(threads);

    threads.update((list) =>
      list.map((t) => (t.id === threadId ? { ...t, starred: newStarred, star_type: nextStar } : t)),
    );
    try {
      await invoke("set_thread_star", {
        threadId: threadId,
        starLabelId: nextStar,
        accountId: null,
      });
    } catch (e) {
      console.error("Failed to set star", e);
      threads.set(currentList);
      addToast("Failed to update star", "error", 3000);
    }
  }

  async function toggleImportant(threadId: string, currentImportant: boolean) {
    const newState = !currentImportant;
    threads.update((list) =>
      list.map((t) => (t.id === threadId ? { ...t, important: newState } : t)),
    );
    try {
      await invoke("toggle_thread_important", {
        threadId: threadId,
        important: newState,
      });
    } catch (e) {
      console.error("Failed to toggle important:", e);
      threads.update((list) =>
        list.map((t) =>
          t.id === threadId ? { ...t, important: currentImportant } : t,
        ),
      );
    }
  }

  async function selectThread(threadId: string) {
    selectedThreadId.set(threadId);
    isMessagesLoading.set(true);
    messagesError.set(null);
    currentMessages.set([]);
    try {
      const cachedMsgs: LocalMessage[] = await invoke("get_messages", {
        threadId: threadId,
      });
      if (cachedMsgs.length > 0) {
        currentMessages.set(cachedMsgs);
        isMessagesLoading.set(false);
      }
      invoke("sync_thread_messages", { threadId: threadId })
        .then(async () => {
          if ($selectedThreadId !== threadId) return;
          const freshMsgs: LocalMessage[] = await invoke("get_messages", {
            threadId: threadId,
          });
          if ($selectedThreadId === threadId) {
            currentMessages.set(freshMsgs);
            isMessagesLoading.set(false);
          }
        })
        .catch((err) => {
          console.error("[SyncMessages] Failed:", err);
          if ($selectedThreadId === threadId) {
            isMessagesLoading.set(false);
          }
        });
      const msgs = cachedMsgs;

      const delaySetting = (await invoke("get_setting", {
        key: "mark_read_delay",
      }).catch(() => "2")) as string;
      console.log("[MarkRead] delay setting:", delaySetting);
      if (delaySetting !== "never") {
        const delayMs =
          delaySetting === "instant" ? 0 : (parseInt(delaySetting) || 2) * 1000;
        setTimeout(() => {
          if ($selectedThreadId === threadId) {
            threads.set(
              $threads.map((t) =>
                t.id === threadId ? { ...t, unread: 0 } : t,
              ),
            );
            invoke("mark_thread_read_status", {
              threadId: threadId,
              isRead: true,
            }).catch(() => {});
          }
        }, delayMs);
      }
    } catch (e) {
      messagesError.set(String(e));
      addToast(`Failed to load messages: ${e}`, "error", 6000);
    } finally {
      isMessagesLoading.set(false);
    }
  }

  async function switchAccount(accountId: string) {
    try {
      await invoke("switch_account", { accountId: accountId });
      await refreshAccountState();
      await performSync(true);

      // Reload superstars for new account
      invoke<string[]>("get_available_superstars", { accountId: null })
        .then((stars) => availableSuperstars.set(stars))
        .catch(() => availableSuperstars.set(["YELLOW_STAR"]));
    } catch (e) {
      console.error("Switch account failed", e);
    }
  }

  async function addAccount(credentialSource?: string, clientId?: string, clientSecret?: string) {
    if (credentialSource === 'custom' && clientId && clientSecret) {
      await invoke("update_setting", { key: "oauth_custom_client_id", value: clientId });
      await invoke("update_setting", { key: "oauth_custom_client_secret", value: clientSecret });
    } else {
      await invoke("update_setting", { key: "oauth_custom_client_id", value: "" });
      await invoke("update_setting", { key: "oauth_custom_client_secret", value: "" });
    }
    showSettings = false;
    await login();
  }

  async function removeAccount(accountId: string) {
    try {
      await invoke("remove_account", { accountId: accountId });
      await refreshAccountState();
      if (allAccounts.length === 0) {
        appState = "onboarding";
        isAuthenticated.set(false);
      } else {
        await performSync(true);
      }
    } catch (e) {
      console.error("Remove account failed", e);
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    shortcutManager.handleKeydown(event);
    if (event.defaultPrevented) return;
    if (
      event.target instanceof HTMLInputElement ||
      event.target instanceof HTMLTextAreaElement ||
      (event.target instanceof HTMLElement && event.target.isContentEditable)
    )
      return;
    
    // Batch selection shortcuts
    if (event.key === "a" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      selectAll($threads.map(t => t.id));
      return;
    }
    if (event.key === "Escape" && $selectedThreadIds.size > 0) {
      clearSelection();
      return;
    }
    if ($selectedThreadIds.size > 0) {
      if (event.key === "e") { executeBatchAction("archive"); return; }
      if (event.key === "#") { executeBatchAction("trash"); return; }
      if (event.key === "I" && event.shiftKey) {
        const selected = $threads.filter(t => $selectedThreadIds.has(t.id));
        const hasUnread = selected.some(t => t.unread > 0);
        executeBatchAction("read", hasUnread);
        return;
      }
    }

    // Commands e, #, Shift+I, r remain as contextual thread actions
    if (!$selectedThreadId) return;
    if (event.key === "e") executeAction("archive");
    else if (event.key === "#") executeAction("trash");
    else if (event.key === "I" && event.shiftKey) executeAction("unread");
    else if (event.key === "h") snoozePopoverOpen = true;
    else if (event.key === "r") {
      const msgs = $currentMessages;
      if (msgs.length > 0) handleReply(msgs[msgs.length - 1]);
    }
  }

  function extractAddress(str: string): string {
    const match = str.match(/<([^>]+)>/);
    return match ? match[1] : str;
  }

  function handleSmartReply(text: string, msg: LocalMessage) {
    const thread = $threads.find((t) => t.id === msg.thread_id);
    let subject = msg.subject || thread?.subject || "";
    if (!subject.toLowerCase().startsWith("re:")) subject = `Re: ${subject}`;
    const to = msg.sender || "";
    const body = `<p>${text.replace(/\n/g, "<br>")}</p>`;

    openCompose({
      initialTo: to,
      initialSubject: subject,
      initialBodyHTML: body,
      threadId: msg.thread_id,
      inReplyTo: msg.id,
      references: msg.id,
      accountId: thread?.account_id ?? null,
    });
  }

  function handleReply(msg: LocalMessage) {
    const thread = $threads.find((t) => t.id === msg.thread_id);
    let subject = msg.subject || thread?.subject || "";
    if (!subject.toLowerCase().startsWith("re:")) subject = `Re: ${subject}`;

    const to = msg.sender || "";

    let quote = ``;
    if (msg.body_html) {
      const quotedContent = prepareQuotedHtml(msg.body_html);
      quote = `<br><br><div>On ${formatTime(msg.internal_date)}, ${msg.sender.replace(/</g, "&lt;").replace(/>/g, "&gt;")} wrote:</div>${quotedContent}`;
    } else if (msg.body_plain) {
      quote =
        `\n\nOn ${formatTime(msg.internal_date)}, ${msg.sender} wrote:\n> ` +
        msg.body_plain.replace(/\n/g, "\n> ");
      quote = `<pre>${quote}</pre>`;
    }

    openCompose({
      initialTo: to,
      initialSubject: subject,
      initialBodyHTML: quote,
      threadId: msg.thread_id,
      inReplyTo: msg.id,
      references: msg.id,
      accountId: thread?.account_id ?? null,
    });
  }

  function handleReplyAll(msg: LocalMessage) {
    const thread = $threads.find((t) => t.id === msg.thread_id);
    let subject = msg.subject || thread?.subject || "";
    if (!subject.toLowerCase().startsWith("re:")) subject = `Re: ${subject}`;

    let allRecipients = (
      msg.sender + (msg.recipients ? `, ${msg.recipients}` : "")
    )
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    const selfEmail = activeAccount?.email || "";

    allRecipients = allRecipients.filter(
      (r) => extractAddress(r) !== selfEmail,
    );
    allRecipients = [...new Set(allRecipients)];

    const to = allRecipients.join(", ");

    let quote = ``;
    if (msg.body_html) {
      const quotedContent = prepareQuotedHtml(msg.body_html);
      quote = `<br><br><div>On ${formatTime(msg.internal_date)}, ${msg.sender.replace(/</g, "&lt;").replace(/>/g, "&gt;")} wrote:</div>${quotedContent}`;
    } else if (msg.body_plain) {
      quote =
        `\n\nOn ${formatTime(msg.internal_date)}, ${msg.sender} wrote:\n> ` +
        msg.body_plain.replace(/\n/g, "\n> ");
      quote = `<pre>${quote}</pre>`;
    }

    openCompose({
      initialTo: to,
      initialSubject: subject,
      initialBodyHTML: quote,
      threadId: msg.thread_id,
      inReplyTo: msg.id,
      references: msg.id,
      accountId: thread?.account_id ?? null,
    });
  }

  function handleForward(msg: LocalMessage) {
    const thread = $threads.find((t) => t.id === msg.thread_id);
    let subject = msg.subject || thread?.subject || "";
    if (!subject.toLowerCase().startsWith("fwd:")) subject = `Fwd: ${subject}`;

    let quote = ``;
    let headerStr = `---------- Forwarded message ---------\nFrom: ${msg.sender}\nDate: ${formatTime(msg.internal_date)}\nSubject: ${msg.subject}\nTo: ${msg.recipients || ""}\n\n`;

    if (msg.body_html) {
      let htmlHeader = `<div>---------- Forwarded message ---------<br>From: ${msg.sender.replace(/</g, "&lt;").replace(/>/g, "&gt;")}<br>Date: ${formatTime(msg.internal_date)}<br>Subject: ${msg.subject}<br>To: ${msg.recipients || ""}<br><br></div>`;
      const quotedContent = prepareQuotedHtml(msg.body_html);
      quote = `<br><br>${htmlHeader}${quotedContent}`;
    } else if (msg.body_plain) {
      quote = `<pre>${headerStr}${msg.body_plain}</pre>`;
    }

    openCompose({
      initialTo: "",
      initialSubject: subject,
      initialBodyHTML: quote,
    });
  }

  async function handleEditDraft(msg: LocalMessage) {
    try {
      const draftId = (await invoke("get_draft_id_by_message_id", {
        messageId: msg.id,
      })) as string;

      openCompose({
        initialTo: msg.recipients,
        initialSubject: msg.subject,
        initialBodyHTML: msg.body_html || msg.body_plain,
        threadId: msg.thread_id,
        initialDraftId: draftId,
      });
    } catch (e) {
      console.error("Failed to get draft ID", e);
      addToast(`Could not edit draft: ${e}`, "error", 5000);
    }
  }

  async function refreshLinkBehavior() {
    const val = await invoke("get_setting", { key: "link_behavior" }).catch(() => "") as string;
    linkBehavior = val || "ask";
  }

  $effect(() => {
    if (!showSettings) {
      refreshLinkBehavior();
      invoke("get_setting", { key: "unified_indicator" }).catch(() => "").then((val) => {
        unifiedIndicatorSetting = (val as string) || "avatar";
      });
      invoke("get_setting", { key: "enable_unified_inbox" }).catch(() => "").then((val) => {
        isUnifiedEnabledSetting = (val as string) !== "false";
        if (!isUnifiedEnabledSetting && $selectedLabelId.startsWith("UNIFIED_")) {
          selectLabel("INBOX");
        }
      });
    }
  });

  onMount(async () => {
    window.addEventListener('message', handleIframeMessage);
    isMacOS = navigator.platform.toUpperCase().includes("MAC");
    if (!isMacOS) {
      await getCurrentWindow().setDecorations(false);
    }
    sidebarCollapsed = localStorage.getItem("rustymail-sidebar-collapsed") === "1";

    const savedLinkBehavior = await invoke("get_setting", { key: "link_behavior" }).catch(() => "") as string;
    linkBehavior = savedLinkBehavior || "ask";

    const dbTheme = await invoke("get_setting", { key: "theme" }).catch(() => "") as string;
    const saved = (dbTheme || localStorage.getItem("rustymail-theme") || "system") as ThemeMode;
    applyTheme(saved, false);

    const savedDensity = await invoke("get_setting", { key: "density" }).catch(() => "") as string;
    density = savedDensity || "default";
    const savedPane = await invoke("get_setting", { key: "reading_pane" }).catch(() => "") as string;
    readingPane = savedPane || "right";
    const savedTpp = await invoke("get_setting", { key: "threads_per_page" }).catch(() => "") as string;
    threadsPerPage = parseInt(savedTpp) || 100;
    const savedIndicator = await invoke("get_setting", { key: "unified_indicator" }).catch(() => "") as string;
    unifiedIndicatorSetting = savedIndicator || "avatar";
    const savedUnified = await invoke("get_setting", { key: "enable_unified_inbox" }).catch(() => "") as string;
    isUnifiedEnabledSetting = savedUnified !== "false";

    await refreshAccountState();
    if (appState === "authenticated") {
      await loadLabels();
      await loadThreads(true);
      await checkAndSetupSync();
      await checkScheduledSends();
      await refreshScheduledCount();
      await loadSubscriptionCount();

      // Load available superstars
      invoke<string[]>("get_available_superstars", { accountId: null })
        .then((stars) => availableSuperstars.set(stars))
        .catch(() => availableSuperstars.set(["YELLOW_STAR"]));

      // Check for outdated OAuth scopes
      const scopeToastShown = sessionStorage.getItem('scope_toast_shown');
      if (!scopeToastShown) {
        try {
          const outdated = await invoke<Array<{id: string, email: string, provider_type: string}>>('check_scopes_outdated');
          if (outdated.length > 0) {
            sessionStorage.setItem('scope_toast_shown', 'true');
            for (const account of outdated) {
              addToast(
                `${account.email} needs updated permissions for contact sync`,
                'info',
                0,
                { label: 'Re-authenticate', onClick: () => { invoke('authenticate_gmail'); } }
              );
            }
          }
        } catch {}
      }

    }

    setTimeout(() => checkForUpdates(true), 5000);

    // Request notification permission
    isPermissionGranted().then((granted) => {
      if (!granted) requestPermission();
    }).catch(() => {});

    // Bring app to foreground when notification is clicked
    onAction(() => {
      const win = getCurrentWindow();
      win.show();
      win.unminimize();
      win.setFocus();
    }).catch(() => {});

    shortcutManager.loadSettings();
    shortcutManager.on('palette', () => showCommandPalette = true);
    shortcutManager.on('compose', () => openCompose());
    shortcutManager.on('sync', () => performSync(true));
    shortcutManager.on('settings', () => showSettings = true);
    shortcutManager.on('search', () => threadListRef?.focusSearch());
    shortcutManager.on('sidebar', () => toggleSidebar());
    shortcutManager.on('escape', () => {
      if (showCommandPalette) showCommandPalette = false;
      else if (showSettings) showSettings = false;
      else if (selectedThreadId) {
        selectedThreadId.set(null);
        currentMessages.set([]);
      }
    });

    // Tray event listeners
    listen("tray-compose", async () => {
      await openCompose();
    }).then((fn) => (unlistenTrayCompose = fn));
    listen("tray-check-mail", async () => {
      await performSync(true);
    }).then((fn) => (unlistenTrayCheckMail = fn));
    listen("imap-new-mail", async () => {
      await performSync(false);
    });
    listen("imap-connection-state", (event: any) => {
      const { account_id, state } = event.payload;
      imapConnectionStates[account_id] = state;
    });

    // Quit confirmation — fetch fresh counts from backend (not stale component state)
    listen("quit-requested", async () => {
      let freshScheduled = 0;
      let freshSnoozed = 0;
      try { freshScheduled = await invoke("get_scheduled_count") as number; } catch {}
      try {
        const snoozedList: any[] = await invoke("get_snoozed_threads");
        freshSnoozed = snoozedList.length;
      } catch {}

      if (freshScheduled === 0 && freshSnoozed === 0) {
        await invoke("confirm_quit");
        return;
      }

      const parts: string[] = [];
      if (freshScheduled > 0) parts.push(`${freshScheduled} scheduled email${freshScheduled > 1 ? 's' : ''}`);
      if (freshSnoozed > 0) parts.push(`${freshSnoozed} snoozed thread${freshSnoozed > 1 ? 's' : ''}`);

      const confirmed = await ask(
        `You have ${parts.join(' and ')}. If you quit, scheduled emails won't be sent on time and snoozed threads won't resurface until you reopen the app.`,
        { title: "Quit Rustymail?", kind: "warning", okLabel: "Quit Anyway", cancelLabel: "Cancel" }
      );

      if (confirmed) {
        await invoke("confirm_quit");
      }
    }).then((fn) => (unlistenQuitRequested = fn));
  });

  let unlistenTrayCompose: (() => void) | null = null;
  let unlistenTrayCheckMail: (() => void) | null = null;
  let unlistenQuitRequested: (() => void) | null = null;
  let stopUpdateCheck: (() => void) | null = null;
  $effect(() => {
    if ($isAuthenticated && !stopUpdateCheck) {
      stopUpdateCheck = setupPeriodicUpdateCheck();
    }
  });

  onDestroy(() => {
    window.removeEventListener('message', handleIframeMessage);
    iframeWindows.clear();
    if (globalSyncInterval) {
      clearInterval(globalSyncInterval);
      globalSyncInterval = null;
    }
    if (currentBgInterval) {
      clearInterval(currentBgInterval);
      currentBgInterval = null;
    }
    syncLock = false;
    if (stopUpdateCheck) stopUpdateCheck();
    if (unlistenTrayCompose) unlistenTrayCompose();
    if (unlistenTrayCheckMail) unlistenTrayCheckMail();
    if (unlistenQuitRequested) unlistenQuitRequested();
  });

  function getActiveLabelName(): string {
    const lid = $selectedLabelId;
    if (lid === "UNIFIED_INBOX") return "Inbox";
    if (lid === "UNIFIED_SENT") return "Sent";
    if (lid === "UNIFIED_DRAFT") return "Drafts";
    if (lid === "UNIFIED_TRASH") return "Trash";
    const label = $labels.find((l) => l.id === lid);
    return label ? formatLabelName(label.name) : "Inbox";
  }

</script>

<svelte:window onkeydown={handleKeydown} />

{#if appState === "loading"}
  <main class="loading-container">
    <div class="loading-spinner large"></div>
  </main>
{:else if appState === "onboarding"}
  <main class="onboarding">
    <div class="onboard-content slide-in">
      {#if onboardingView === 'providers'}
        <img src="/app-icon.png" alt="Rustymail" class="onboard-icon" />
        <h1 class="onboard-title">Welcome to Rustymail</h1>
        <p class="onboard-subtitle">Fast, private email client</p>

        <div class="provider-buttons">
          {#if showGoogleOnboardOptions}
            <div style="display: flex; flex-direction: column; gap: 8px;">
              <button class="btn-provider" disabled={!!onboardingLoading} onclick={async () => {
                onboardingLoading = 'google';
                try {
                  await invoke("update_setting", { key: "oauth_custom_client_id", value: "" });
                  await invoke("update_setting", { key: "oauth_custom_client_secret", value: "" });
                  await invoke("authenticate_gmail");
                  await refreshAccountState();
                  appState = "authenticated";
                  await performSync(true);
                } catch (e) {
                  addToast(String(e), "error", 6000);
                }
                onboardingLoading = '';
                showGoogleOnboardOptions = false;
              }}>
                {onboardingLoading === 'google' ? 'Connecting...' : 'Continue with built-in credentials'}
              </button>
              <button class="btn-provider-link" onclick={() => showOnboardByoFields = !showOnboardByoFields}>
                Use your own OAuth credentials {showOnboardByoFields ? '▾' : '▸'}
              </button>
              {#if showOnboardByoFields}
                <div style="display: flex; flex-direction: column; gap: 6px; padding: 8px 0;">
                  <input type="text" class="onboard-credential-input" class:invalid={onboardByoIdError}
                    placeholder="Client ID (123456789-abc.apps.googleusercontent.com)"
                    bind:value={onboardByoClientId} />
                  {#if onboardByoIdError}<span class="onboard-field-error">{onboardByoIdError}</span>{/if}
                  <input type="password" class="onboard-credential-input" class:invalid={onboardByoSecretError}
                    placeholder="Client Secret (GOCSPX-...)"
                    bind:value={onboardByoClientSecret} />
                  {#if onboardByoSecretError}<span class="onboard-field-error">{onboardByoSecretError}</span>{/if}
                  <button class="btn-provider" disabled={!onboardByoValid || !!onboardingLoading}
                    onclick={async () => {
                      onboardingLoading = 'google';
                      try {
                        await invoke("update_setting", { key: "oauth_custom_client_id", value: onboardByoClientId.trim() });
                        await invoke("update_setting", { key: "oauth_custom_client_secret", value: onboardByoClientSecret.trim() });
                        await invoke("authenticate_gmail");
                        await refreshAccountState();
                        appState = "authenticated";
                        await performSync(true);
                      } catch (e) {
                        addToast(String(e), "error", 6000);
                      }
                      onboardingLoading = '';
                      showGoogleOnboardOptions = false; showOnboardByoFields = false;
                      onboardByoClientId = ''; onboardByoClientSecret = '';
                    }}>
                    {onboardingLoading === 'google' ? 'Connecting...' : 'Sign in with custom credentials'}
                  </button>
                </div>
              {/if}
              <button class="btn-provider-link" onclick={() => { showGoogleOnboardOptions = false; showOnboardByoFields = false; }}>&larr; Back</button>
            </div>
          {:else}
            <button class="btn-provider" disabled={!!onboardingLoading} onclick={() => showGoogleOnboardOptions = true}>
              <svg width="18" height="18" viewBox="0 0 48 48"><path fill="#EA4335" d="M24 9.5c3.54 0 6.71 1.22 9.21 3.6l6.85-6.85C35.9 2.38 30.47 0 24 0 14.62 0 6.51 5.38 2.56 13.22l7.98 6.19C12.43 13.72 17.74 9.5 24 9.5z"/><path fill="#4285F4" d="M46.98 24.55c0-1.57-.15-3.09-.38-4.55H24v9.02h12.94c-.58 2.96-2.26 5.48-4.78 7.18l7.73 6c4.51-4.18 7.09-10.36 7.09-17.65z"/><path fill="#FBBC05" d="M10.53 28.59c-.48-1.45-.76-2.99-.76-4.59s.27-3.14.76-4.59l-7.98-6.19C.92 16.46 0 20.12 0 24c0 3.88.92 7.54 2.56 10.78l7.97-6.19z"/><path fill="#34A853" d="M24 48c6.48 0 11.93-2.13 15.89-5.81l-7.73-6c-2.15 1.45-4.92 2.3-8.16 2.3-6.26 0-11.57-4.22-13.47-9.91l-7.98 6.19C6.51 42.62 14.62 48 24 48z"/></svg>
              {onboardingLoading === 'google' ? 'Connecting...' : 'Sign in with Google'}
            </button>

            <button class="btn-provider" disabled={!!onboardingLoading} onclick={async () => {
              onboardingLoading = 'microsoft';
              try {
                await invoke("authenticate_microsoft");
                await refreshAccountState();
                appState = "authenticated";
                await performSync(true);
              } catch (e) {
                addToast(String(e), "error", 6000);
              }
              onboardingLoading = '';
            }}>
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none"><rect x="1" y="1" width="10" height="10" fill="#F25022"/><rect x="13" y="1" width="10" height="10" fill="#7FBA00"/><rect x="1" y="13" width="10" height="10" fill="#00A4EF"/><rect x="13" y="13" width="10" height="10" fill="#FFB900"/></svg>
              {onboardingLoading === 'microsoft' ? 'Connecting...' : 'Sign in with Microsoft'}
            </button>

            <div class="provider-divider">or</div>

            <button class="btn-provider-link" onclick={() => onboardingView = 'imap'}>
              Other email account (IMAP) &rarr;
            </button>
          {/if}
        </div>

        <p class="onboard-footer">Your data stays on your device.</p>
      {:else}
        <ImapAccountForm
          onSuccess={async () => {
            await refreshAccountState();
            appState = "authenticated";
            await performSync(true);
          }}
          onCancel={() => onboardingView = 'providers'}
        />
      {/if}
    </div>
  </main>
{:else}
  <div class="app-container density-{density} pane-{readingPane}">
    <Sidebar
      {activeAccount}
      {allAccounts}
      collapsed={sidebarCollapsed}
      {isMacOS}
      {themeIcon}
      {themeLabel}
      sidebarCollapseIcon={iconSidebarCollapse}
      sidebarExpandIcon={iconSidebarExpand}
      isUnifiedEnabled={isUnifiedEnabledSetting}
      {labels}
      {selectedLabelId}
      snoozedCount={snoozedCount}
      scheduledCount={scheduledCount}
      oncompose={() => openCompose()}
      onsync={() => performSync(true)}
      onthemecycle={cycleTheme}
      showCalendarToggle={capabilities.has_calendar}
      connectionState={activeAccount?.provider_type === 'imap' ? (imapConnectionStates[activeAccount.id] || '') : ''}
      ontogglecalendar={() => viewMode = viewMode === "calendar" ? "mail" : "calendar"}
      ontogglesubscriptions={() => viewMode = viewMode === "subscriptions" ? "mail" : "subscriptions"}
      ontogglecontacts={() => { viewMode = viewMode === "contacts" ? "mail" : "contacts"; }}
      onfeed={() => selectLabel('FEED')}
      {hasSubscriptions}
      onsettings={() => (showSettings = true)}
      ontogglecollapse={toggleSidebar}
      onselectlabel={selectLabel}
      onswitchaccount={switchAccount}
      onaddaccount={() => { showSettings = true; showAddAccount = true; }}
    />

    {#if viewMode === "mail"}
      {#if $selectedLabelId === 'FEED'}
        <FeedView {isMacOS} onselectthread={selectThread} />
      {:else}
      <ThreadList
        bind:this={threadListRef}
        {isLoadingThreads}
        {isLabelFetching}
        {isMacOS}
        {currentPage}
        {threadsPerPage}
        {totalCount}
        {hasMoreRemote}
        {gmailTotal}
        {isBackgroundFilling}
        activeLabelName={getActiveLabelName()}
        {searchQuery}
        {isSearching}
        showCategoryTabs={$selectedLabelId === "INBOX" && capabilities.has_categories}
        {selectedCategory}
        unifiedIndicator={unifiedIndicatorSetting}
        {allAccounts}
        isUnifiedView={$selectedLabelId.startsWith("UNIFIED_")}
        onselectthread={selectThread}
        ontogglestar={cycleStar}
        ontoggleimportant={toggleImportant}
        onfirstpage={goToFirstPage}
        onprevpage={goToPrevPage}
        onnextpage={goToNextPage}
        onsearch={handleSearch}
        onclearsearch={clearSearch}
        onselectcategory={selectCategory}
        onbatcharchive={() => executeBatchAction("archive")}
        onbatchtrash={() => executeBatchAction("trash")}
        onbatchrestore={() => executeBatchAction("restore")}
        onbatchread={(_ids, isRead) => executeBatchAction("read", isRead)}
        onbatchstar={(_ids, starred) => executeBatchAction("star", starred)}
        onbatchsnooze={() => { batchSnoozeOpen = true; }}
        onbatchunsnooze={(ids) => executeBatchAction("unsnooze")}
        onbatchmovetolabel={() => { labelPickerOpen = true; }}
        isSnoozedView={$selectedLabelId === "SNOOZED" || $selectedLabelId === "UNIFIED_SNOOZED"}
        isTrashView={$selectedLabelId === "TRASH"}
        hasSuperstars={capabilities.has_superstars}
        hasImportant={capabilities.has_important}
        accountProviderTypes={accountProviderMap}
      />
      {/if}

      <MessageDetail
        {isMacOS}
        isTrashView={$selectedLabelId === "TRASH"}
        isSnoozedView={$selectedLabelId === "SNOOZED" || $selectedLabelId === "UNIFIED_SNOOZED"}
        bind:showSnoozePopover={snoozePopoverOpen}
        onaction={executeAction}
        onreply={handleReply}
        onreplyall={handleReplyAll}
        onforward={handleForward}
        oneditdraft={handleEditDraft}
        oniframeload={handleIframeLoad}
        onsmartreply={handleSmartReply}
      />

      {#if batchSnoozeOpen}
        <div class="batch-popover-overlay">
          <SnoozePopover
            onsnooze={(until) => { batchSnoozeOpen = false; executeBatchAction("snooze", until); }}
            onclose={() => { batchSnoozeOpen = false; }}
          />
        </div>
      {/if}

      {#if labelPickerOpen}
        <div class="batch-popover-overlay">
          <LabelPicker
            labels={userLabels}
            onselect={(labelId) => { labelPickerOpen = false; executeBatchAction("movetolabel", labelId); }}
            onclose={() => { labelPickerOpen = false; }}
          />
        </div>
      {/if}
    {:else if viewMode === "calendar"}
      <FullCalendar {isMacOS} />
    {:else if viewMode === "subscriptions"}
      <Subscriptions accountId={activeAccount?.id ?? ""} {isMacOS} onselectsubscription={handleSelectSubscription} />
    {:else if viewMode === "contacts"}
      <Contacts />
    {/if}
  </div>

  <Settings
    bind:show={showSettings}
    initialAddAccount={showAddAccount}
    accounts={allAccounts}
    onclose={async () => {
      showSettings = false;
      showAddAccount = false;
      checkAndSetupSync();
      await refreshAccountState();
      await performSync(true);
    }}
    onAccountSwitch={switchAccount}
    onAccountAdd={(source, id, secret) => addAccount(source, id, secret)}
    onAccountRemove={removeAccount}
    onThemeChange={(mode) => applyTheme(mode as ThemeMode, false)}
    onDensityChange={(d) => density = d}
    onReadingPaneChange={(p) => readingPane = p}
    onThreadsPerPageChange={(n) => { threadsPerPage = n; currentPage = 0; loadThreads(true); }}
  />
{/if}

{#key composeKey}
  {#if showCompose}
    <Compose
      onClose={async () => {
        showCompose = false;
        const tid = $selectedThreadId;
        if (!tid) return;

        async function refreshThread() {
          try {
            await invoke("sync_thread_messages", { threadId: tid });
          } catch (_) {}
          try {
            const freshMsgs: LocalMessage[] = await invoke("get_messages", { threadId: tid });
            if ($selectedThreadId === tid) {
              currentMessages.set(freshMsgs);
              if (freshMsgs.length === 0) {
                selectedThreadId.set(null);
                currentMessages.set([]);
              }
            }
          } catch (_) {}
          await loadThreads(true, true);
        }

        // Immediate refresh (picks up draft changes)
        await refreshThread();
        // Delayed retry (picks up sent messages after Gmail processes them)
        setTimeout(() => refreshThread(), 2000);
      }}
      {...composeProps}
      onDraftSaved={(id) => (composeProps.initialDraftId = id)}
      hasDriveUpload={capabilities.has_drive_upload}
      providerType={activeAccount?.provider_type ?? 'gmail'}
    />
  {/if}
{/key}

<LinkSafetyDialog
  url={pendingLinkUrl}
  analysis={pendingLinkAnalysis}
  onconfirm={confirmOpenLink}
  ondismiss={dismissLinkDialog}
/>

<CommandPalette
  bind:show={showCommandPalette}
  accounts={allAccounts}
  hasThread={!!$selectedThreadId}
  onAction={handleCommandAction}
  onClose={() => showCommandPalette = false}
/>

{#if $pendingUpdate}
  <UpdateModal
    currentVersion={$pendingUpdate.currentVersion}
    newVersion={$pendingUpdate.newVersion}
    releaseDate={$pendingUpdate.releaseDate}
    releaseNotes={$pendingUpdate.releaseNotes}
    onClose={() => pendingUpdate.set(null)}
    onInstall={$pendingUpdate.onInstall}
  />
{/if}

<Toasts />

<style>
  .loading-container {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    width: 100vw;
    background: var(--bg-view);
  }

  .onboarding {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    width: 100vw;
    background: var(--bg-sidebar, #f5f5f7);
    font-family: var(--font-family, -apple-system, BlinkMacSystemFont, system-ui, sans-serif);
  }
  .onboard-content {
    text-align: center;
    max-width: 320px;
  }
  .slide-in {
    animation: slideIn 0.4s ease forwards;
  }
  @keyframes slideIn {
    from { opacity: 0; transform: translateY(12px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .onboard-icon {
    width: 80px;
    height: 80px;
    margin-bottom: 20px;
    border-radius: 18px;
  }
  .onboard-title {
    font-size: 26px;
    font-weight: 600;
    color: var(--text-primary, #1c1c1e);
    margin-bottom: 6px;
    letter-spacing: -0.3px;
  }
  .onboard-subtitle {
    font-size: 14px;
    line-height: 18px;
    color: var(--text-secondary, #8e8e93);
    margin-bottom: 32px;
  }
  .provider-buttons {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 100%;
    max-width: 300px;
    margin-top: 20px;
  }
  .btn-provider {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 11px 16px;
    border: 1px solid var(--border, rgba(0,0,0,0.1));
    border-radius: 8px;
    background: var(--bg-primary, #fff);
    color: var(--text-primary, #1c1c1e);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
    font-family: inherit;
  }
  .btn-provider:hover { background: var(--bg-secondary, #f0f0f0); border-color: var(--text-tertiary, #aaa); }
  .btn-provider:active { transform: scale(0.98); }
  .btn-provider:disabled { opacity: 0.5; cursor: not-allowed; }
  .provider-divider {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 6px 0;
    color: var(--text-tertiary, #aaa);
    font-size: 12px;
  }
  .provider-divider::before, .provider-divider::after {
    content: '';
    flex: 1;
    border-top: 1px dashed var(--border, rgba(0,0,0,0.1));
  }
  .btn-provider-link {
    background: none;
    border: none;
    color: var(--accent, #0a84ff);
    font-size: 12px;
    cursor: pointer;
    padding: 4px 0;
    font-family: inherit;
  }
  .btn-provider-link:hover { text-decoration: underline; }
  .onboard-credential-input {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--border, rgba(0,0,0,0.1));
    border-radius: 6px;
    background: var(--bg-primary, #fff);
    color: var(--text-primary, #1c1c1e);
    font-size: 12px;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  }
  .onboard-credential-input::placeholder { color: var(--text-secondary, #8e8e93); opacity: 0.5; }
  .onboard-credential-input:focus { outline: none; border-color: var(--accent, #0a84ff); }
  .onboard-credential-input.invalid { border-color: #FF453A; }
  .onboard-field-error { font-size: 11px; color: #FF453A; padding-left: 2px; }
  .onboard-footer {
    margin-top: 16px;
    font-size: 11px;
    line-height: 14px;
    color: var(--text-secondary, #8e8e93);
    padding: 8px;
    font-family: inherit;
    letter-spacing: normal;
    transition: color 0.2s;
  }
  .loading-spinner {
    width: 20px;
    height: 20px;
    border: 2px solid var(--border-color);
    border-top-color: var(--accent-blue);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }
  .loading-spinner.large {
    width: 32px;
    height: 32px;
    border-width: 3px;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .batch-popover-overlay {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    z-index: 200;
  }
</style>
