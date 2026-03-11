<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
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
    type LocalMessage,
  } from "$lib/stores/messages";
  import { writable, get } from "svelte/store";
  import {
    getLabelIcon,
    formatLabelName,
    iconInbox,
    iconArchive,
    iconTrash,
    iconMail,
    iconSearch,
    iconRefresh,
    iconClose,
    iconSettings,
    iconUser,
    iconChevronDown,
    iconPlus,
    iconShield,
    iconZap,
    iconGlobe,
    iconCalendar,
    iconTag,
    iconHistory,
    iconStar,
    iconStarFilled,
    iconReply,
    iconReplyAll,
    iconForward,
    iconDraft,
  } from "$lib/components/icons";
  import Settings from "$lib/components/Settings.svelte";
  import Compose from "$lib/components/Compose.svelte";
  import CalendarSidebar from "$lib/components/CalendarSidebar.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import { addToast } from "$lib/stores/toast";
  import {
    formatTime,
    decodeEntities,
    prepareQuotedHtml,
  } from "$lib/utils/formatters.js";

  interface LocalLabel {
    id: string;
    name: string;
    type: string;
    unread_count: number;
  }
  interface AccountInfo {
    id: string;
    email: string;
    display_name: string;
    avatar_url: string;
    is_active: boolean;
  }
  interface SearchSuggestion {
    kind: string;
    text: string;
    detail: string;
  }

  const labels = writable<LocalLabel[]>([]);
  const selectedLabelId = writable<string>("INBOX");
  const searchQuery = writable<string>("");
  const isSearching = writable<boolean>(false);

  let activeAccount = $state<AccountInfo | null>(null);
  let allAccounts = $state<AccountInfo[]>([]);
  let showAccountDropdown = $state(false);
  let showSettings = $state(false);
  let isLoading = $state(false);
  let isLoadingThreads = $state(false);
  let showCompose = $state(false);
  let showCalendar = $state(false);

  let isMacOS = $state(false);
  let sidebarCollapsed = $state(false);
  let linkBehavior = $state("ask");
  let pendingLinkUrl = $state<string | null>(null);
  let pendingLinkAnalysis = $state<LinkAnalysis | null>(null);
  const iframeWindows = new Map<Window, HTMLIFrameElement>();

  function handleIframeMessage(event: MessageEvent) {
    const iframe = iframeWindows.get(event.source as Window);
    if (!iframe) return;
    const data = event.data;
    if (!data || typeof data !== 'object') return;

    if (data.type === 'rustymail-resize' && typeof data.height === 'number') {
      iframe.style.height = data.height + 'px';
      iframe.style.opacity = '1';
    } else if (data.type === 'rustymail-link' && typeof data.url === 'string') {
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
  let searchInput = $state("");
  let searchTimeout: ReturnType<typeof setTimeout> | null = null;
  let showSearchSuggestions = $state(false);
  let searchSuggestions = $state<SearchSuggestion[]>([]);
  let appState = $state<"loading" | "onboarding" | "authenticated">("loading");
  let searchInputEl = $state<HTMLInputElement>();
  let threadScrollArea = $state<HTMLDivElement>();
  let loadingSentinel = $state<HTMLDivElement>();

  let threadOffset = $state(0);
  const THREAD_PAGE_SIZE = 50;
  let hasMore = $state(true);
  let isLoadingMore = $state(false);
  let bgSyncDone = $state(false);
  let globalSyncInterval: ReturnType<typeof setInterval> | null = null;
  let currentBgInterval: ReturnType<typeof setInterval> | null = null;
  const labelLastSyncMap: Record<string, number> = {};
  let syncLock = false;

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
  });

  function openCompose(props: Partial<typeof composeProps> = {}) {
    composeProps = {
      initialTo: "",
      initialCc: "",
      initialSubject: "",
      initialBodyHTML: "",
      threadId: null,
      inReplyTo: null,
      references: null,
      initialDraftId: null,
      ...props,
    };
    composeKey++;
    showCompose = true;
  }

  type ThemeMode = "system" | "light" | "dark";
  let themeMode: ThemeMode = $state("system");
  const iconSun =
    '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>';
  const iconMoon =
    '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>';
  const iconMonitor =
    '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>';

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
  const iconSidebarCollapse = '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/><polyline points="14 9 11 12 14 15"/></svg>';
  const iconSidebarExpand = '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/><polyline points="13 9 16 12 13 15"/></svg>';
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
        appState = "authenticated";
      } else {
        appState = allAccounts.length > 0 ? "authenticated" : "onboarding";
      }
    } catch (e) {
      appState = "onboarding";
    }
  }

  async function performSync(isManual = false) {
    // Synchronous lock to prevent re-entrant calls from stacked intervals
    if (syncLock) return;
    syncLock = true;

    // Capture the label at invocation time — if it changes mid-sync, abort
    const syncLabelId = get(selectedLabelId) || "INBOX";

    try {
      isSyncing.set(true);
      lastSyncError.set(null);

      await invoke("sync_gmail_data", { labelId: syncLabelId });

      // GUARD: Abort if user switched folders while we were syncing
      if (get(selectedLabelId) !== syncLabelId) return;

      if (!get(searchQuery)) {
        if (isManual) {
          threadOffset = 0;
          hasMore = true;
          await loadThreads(true);
        } else {
          // Background sync: update silently without reset
          await loadThreads(true, true);
        }
      }

      await loadLabels();

      // Only start background hydration polling on manual syncs
      // to avoid cascading intervals from auto-sync
      if (isManual) {
        pollBackgroundSync();
      }
    } catch (e) {
      lastSyncError.set(String(e));
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
        // Enforce minimum 30 seconds to prevent accidental rapid polling
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

  async function loadThreads(reset = false, silent = false) {
    if (isLoadingThreads && !silent) return;
    if (!silent) isLoadingThreads = true;

    // Capture the label at invocation — all operations must match this
    const invocationLabelId = get(selectedLabelId) || null;

    if (
      reset &&
      get(threads).length > 0 &&
      !isLabelFetching &&
      !get(searchQuery) &&
      !silent
    ) {
    } else if (reset && !silent) {
      if (get(searchQuery) && !isLabelFetching) return;
      threadOffset = 0;
      hasMore = true;
      threads.set([]);
    }

    try {
      const fetched = (await invoke("get_threads", {
        labelId: invocationLabelId,
        offset: reset ? 0 : threadOffset,
        limit: THREAD_PAGE_SIZE,
      })) as LocalThread[];

      // GUARD: Abort if user switched folders during the await
      if ((get(selectedLabelId) || null) !== invocationLabelId) return;

      if (reset && fetched.length === 0 && invocationLabelId && !silent) {
        isLabelFetching = true;
        try {
          await invoke("fetch_label_threads", { labelId: invocationLabelId });

          // GUARD: Abort if user switched folders
          if ((get(selectedLabelId) || null) !== invocationLabelId) return;

          if (invocationLabelId)
            labelLastSyncMap[invocationLabelId] = Date.now();
          const retried = (await invoke("get_threads", {
            labelId: invocationLabelId,
            offset: 0,
            limit: THREAD_PAGE_SIZE,
          })) as LocalThread[];

          // GUARD: Abort if user switched folders
          if ((get(selectedLabelId) || null) !== invocationLabelId) return;

          threads.set(retried);
          hasMore = retried.length >= THREAD_PAGE_SIZE;
          threadOffset = retried.length;
        } catch (_) {
        } finally {
          isLabelFetching = false;
        }
        return;
      }

      if (silent) {
        threads.update((current) => {
          const map = new Map(current.map((t) => [t.id, t]));
          let hasNew = false;
          const updated = [...current];
          const newOnes: LocalThread[] = [];

          for (const f of fetched) {
            const existing = map.get(f.id);
            if (existing) {
              // Only update if something changed to avoid unnecessary re-renders
              if (
                existing.unread !== f.unread ||
                existing.starred !== f.starred ||
                existing.snippet !== f.snippet
              ) {
                Object.assign(existing, f);
              }
            } else {
              newOnes.push(f);
              hasNew = true;
            }
          }

          if (!hasNew) return updated;
          return [...newOnes, ...updated];
        });
      } else if (reset) {
        threads.set(fetched);
        threadOffset = fetched.length;
      } else {
        threads.update((t) => [...t, ...fetched]);
        threadOffset += fetched.length;
      }
      hasMore = fetched.length >= THREAD_PAGE_SIZE;
    } catch (e) {
      console.error("Failed to load threads", e);
    } finally {
      if (!silent) isLoadingThreads = false;
    }
    observeSentinel();
  }

  async function loadMoreThreads() {
    if (isLoadingMore || !hasMore) return;
    isLoadingMore = true;

    // Capture label at invocation — abort if it changes
    const invocationLabelId = get(selectedLabelId) || null;

    try {
      const fetched = (await invoke("get_threads", {
        labelId: invocationLabelId,
        offset: threadOffset,
        limit: THREAD_PAGE_SIZE,
      }).catch(() => [])) as LocalThread[];

      // GUARD: Abort if user switched folders
      if ((get(selectedLabelId) || null) !== invocationLabelId) return;

      if (fetched.length > 0) {
        threads.update((t) => [...t, ...fetched]);
        hasMore = fetched.length >= THREAD_PAGE_SIZE;
        threadOffset += fetched.length;
      } else if (invocationLabelId) {
        try {
          await invoke("fetch_label_threads", { labelId: invocationLabelId });

          // GUARD: Abort if user switched folders
          if ((get(selectedLabelId) || null) !== invocationLabelId) return;

          const retried = (await invoke("get_threads", {
            labelId: invocationLabelId,
            offset: threadOffset,
            limit: THREAD_PAGE_SIZE,
          }).catch(() => [])) as LocalThread[];

          // GUARD: Abort if user switched folders
          if ((get(selectedLabelId) || null) !== invocationLabelId) return;

          if (retried.length > 0) {
            threads.update((t) => [...t, ...retried]);
            threadOffset += retried.length;
          }
          hasMore = retried.length >= THREAD_PAGE_SIZE;
        } catch (_) {
          hasMore = false;
        }
      } else {
        hasMore = false;
      }
    } finally {
      isLoadingMore = false;
      observeSentinel();
    }
  }

  async function loadLabels() {
    try {
      const fetched: LocalLabel[] = await invoke("get_labels");
      labels.set(fetched);
    } catch (e) {
      console.error("Failed to load labels", e);
    }
  }

  async function selectLabel(labelId: string) {
    const prev = $selectedLabelId;
    const isReselect = prev === labelId;
    selectedLabelId.set(labelId);
    selectedThreadId.set(null);
    currentMessages.set([]);
    searchInput = "";
    searchQuery.set("");
    showSearchSuggestions = false;
    threadOffset = 0;
    hasMore = true;

    if (!isReselect) {
      threads.set([]);
    }

    // On-demand refresh for ALL labels (including INBOX)
    // Always refresh when re-clicking the same label (user expects fresh data)
    const lastSync = labelLastSyncMap[labelId] || 0;
    if (isReselect || Date.now() - lastSync > 300000) {
      isSyncing.set(true);
      try {
        await invoke("fetch_label_threads", { labelId: labelId });
        labelLastSyncMap[labelId] = Date.now();
      } catch (e) {
        console.error("On-demand sync failed", e);
      } finally {
        isSyncing.set(false);
      }
    }

    await loadThreads(true);
  }

  let scrollObserver: IntersectionObserver | null = null;

  function setupIntersectionObserver() {
    if (scrollObserver) scrollObserver.disconnect();
    scrollObserver = new IntersectionObserver(
      (entries) => {
        if (
          entries &&
          entries[0]?.isIntersecting &&
          hasMore &&
          !isLoadingMore
        ) {
          loadMoreThreads();
        }
      },
      { root: threadScrollArea, rootMargin: "300px", threshold: 0.01 },
    );
    observeSentinel();
  }

  function observeSentinel() {
    setTimeout(() => {
      if (loadingSentinel && scrollObserver) {
        scrollObserver.observe(loadingSentinel);
      }
    }, 50);
  }

  async function handleSearch() {
    const query = searchInput.trim();
    if (!query) {
      searchQuery.set("");
      await loadThreads(true);
      return;
    }
    searchQuery.set(query);
    isSearching.set(true);
    showSearchSuggestions = false;
    hasMore = false;
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

  async function fetchSuggestions() {
    const cursorPos = searchInputEl?.selectionStart ?? searchInput.length;
    const ctx = parseSearchContext(searchInput, cursorPos);

    if (ctx.operator === "has") {
      searchSuggestions = ["attachment", "link"]
        .filter((v) => v.startsWith(ctx.value))
        .map((v) => ({ kind: "filter", text: v, detail: "" }));
      return;
    }
    if (ctx.operator === "is") {
      searchSuggestions = ["unread", "read", "starred", "draft", "sent"]
        .filter((v) => v.startsWith(ctx.value))
        .map((v) => ({ kind: "filter", text: v, detail: "" }));
      return;
    }
    if (ctx.operator === "before" || ctx.operator === "after") {
      searchSuggestions = [{ kind: "hint", text: "YYYY/MM/DD", detail: `${ctx.operator}: date format` }];
      return;
    }

    try {
      searchSuggestions = await invoke("get_search_suggestions", {
        operator: ctx.operator,
        value: ctx.value,
        fullQuery: searchInput,
      });
    } catch (_) {
      searchSuggestions = [];
    }
  }

  function onSearchInput() {
    if (searchTimeout) clearTimeout(searchTimeout);
    fetchSuggestions();
    showSearchSuggestions = true;
    searchTimeout = setTimeout(() => {
      if (searchInput.trim().length >= 3) handleSearch();
    }, 400);
  }

  function onSearchKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      handleSearch();
      showSearchSuggestions = false;
    } else if (event.key === "Escape") {
      showSearchSuggestions = false;
      searchInputEl?.blur();
    }
  }

  function applySuggestion(text: string) {
    const cursorPos = searchInputEl?.selectionStart ?? searchInput.length;
    const ctx = parseSearchContext(searchInput, cursorPos);

    if (ctx.operator) {
      // Replace only the current token's value portion
      const beforeCursor = searchInput.slice(0, cursorPos);
      const lastSpace = beforeCursor.lastIndexOf(" ");
      const prefix = searchInput.slice(0, lastSpace + 1);
      const afterCursor = searchInput.slice(cursorPos);
      searchInput = `${prefix}${ctx.operator}:${text} ${afterCursor}`.replace(/  +/g, " ");
    } else {
      searchInput = text;
    }
    showSearchSuggestions = false;
    // Only auto-search if the query looks complete (no trailing operator)
    const trimmed = searchInput.trim();
    if (trimmed && !trimmed.endsWith(":")) {
      handleSearch();
    }
  }

  function clearSearch() {
    searchInput = "";
    searchQuery.set("");
    showSearchSuggestions = false;
    searchSuggestions = [];
    loadThreads(true);
  }

  function parseSearchContext(input: string, cursorPos: number): { operator: string | null; value: string } {
    const beforeCursor = input.slice(0, cursorPos);
    const lastSpace = beforeCursor.lastIndexOf(" ");
    const currentToken = beforeCursor.slice(lastSpace + 1);
    const colonIdx = currentToken.indexOf(":");
    if (colonIdx > 0) {
      return { operator: currentToken.slice(0, colonIdx), value: currentToken.slice(colonIdx + 1) };
    }
    return { operator: null, value: currentToken };
  }

  async function executeAction(action: "archive" | "trash" | "unread" | "untrash") {
    const threadId = $selectedThreadId;
    if (!threadId) return;

    // If the compose window is open for this thread and user clicks "Trash",
    // discard the draft instead of trashing the whole thread.
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

  async function toggleStar(threadId: string, currentStarred: boolean) {
    const newState = !currentStarred;
    threads.update((list) =>
      list.map((t) => (t.id === threadId ? { ...t, starred: newState } : t)),
    );
    try {
      await invoke("toggle_thread_star", {
        threadId: threadId,
        starred: newState,
      });
    } catch (e) {
      console.error("Failed to toggle star", e);
      threads.update((list) =>
        list.map((t) =>
          t.id === threadId ? { ...t, starred: currentStarred } : t,
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
      // Show cached messages from SQLite immediately
      const cachedMsgs: LocalMessage[] = await invoke("get_messages", {
        threadId: threadId,
      });
      if (cachedMsgs.length > 0) {
        currentMessages.set(cachedMsgs);
        isMessagesLoading.set(false);
      }
      // Sync from Gmail in background, then refresh if new data arrived
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
    showAccountDropdown = false;
    try {
      await invoke("switch_account", { accountId: accountId });
      await refreshAccountState();
      await performSync(true);
    } catch (e) {
      console.error("Switch account failed", e);
    }
  }

  async function addAccount() {
    showSettings = false;
    showAccountDropdown = false;
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
    if (
      event.target instanceof HTMLInputElement ||
      event.target instanceof HTMLTextAreaElement ||
      (event.target instanceof HTMLElement && event.target.isContentEditable)
    )
      return;
    if (showSettings) {
      if (event.key === "Escape") showSettings = false;
      return;
    }
    if (event.key === "[") {
      toggleSidebar();
      return;
    }
    if (event.key === "/") {
      event.preventDefault();
      searchInputEl?.focus();
      return;
    }
    if (event.key === "Escape") {
      if ($selectedThreadId) {
        selectedThreadId.set(null);
        currentMessages.set([]);
      }
      return;
    }
    if (!$selectedThreadId) return;
    if (event.key === "e") executeAction("archive");
    else if (event.key === "#") executeAction("trash");
    else if (event.key === "I" && event.shiftKey) executeAction("unread");
    else if (event.key === "r") {
      const msgs = $currentMessages;
      if (msgs.length > 0) handleReply(msgs[msgs.length - 1]);
    }
  }

  function extractAddress(str: string): string {
    const match = str.match(/<([^>]+)>/);
    return match ? match[1] : str;
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
      threadId: msg.thread_id,
      inReplyTo: msg.id,
      references: msg.id,
    });
  }

  async function handleEditDraft(msg: LocalMessage) {
    try {
      // First try to fetch the draft ID from Gmail API associated with this message
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

    await refreshAccountState();
    if (appState === "authenticated") {
      await loadLabels();
      await loadThreads(true);
      await checkAndSetupSync();
    }

    setTimeout(() => setupIntersectionObserver(), 100);
    setTimeout(() => checkForUpdates(true), 5000);
  });

  let stopUpdateCheck: (() => void) | null = null;
  $effect(() => {
    if ($isAuthenticated && !stopUpdateCheck) {
      stopUpdateCheck = setupPeriodicUpdateCheck();
    }
  });

  // Clean up all intervals on destroy (critical for HMR to prevent stacking)
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
  });

  function getActiveLabelName(): string {
    const label = $labels.find((l) => l.id === $selectedLabelId);
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
      <img src="/app-icon.png" alt="Rustymail" class="onboard-icon" />
      <h1 class="onboard-title">Rustymail</h1>
      <p class="onboard-subtitle">Fast, private email</p>
      <button class="btn-google" onclick={login} disabled={isLoading}>
        <svg width="18" height="18" viewBox="0 0 48 48"><path fill="#EA4335" d="M24 9.5c3.54 0 6.71 1.22 9.21 3.6l6.85-6.85C35.9 2.38 30.47 0 24 0 14.62 0 6.51 5.38 2.56 13.22l7.98 6.19C12.43 13.72 17.74 9.5 24 9.5z"/><path fill="#4285F4" d="M46.98 24.55c0-1.57-.15-3.09-.38-4.55H24v9.02h12.94c-.58 2.96-2.26 5.48-4.78 7.18l7.73 6c4.51-4.18 7.09-10.36 7.09-17.65z"/><path fill="#FBBC05" d="M10.53 28.59c-.48-1.45-.76-2.99-.76-4.59s.27-3.14.76-4.59l-7.98-6.19C.92 16.46 0 20.12 0 24c0 3.88.92 7.54 2.56 10.78l7.97-6.19z"/><path fill="#34A853" d="M24 48c6.48 0 11.93-2.13 15.89-5.81l-7.73-6c-2.15 1.45-4.92 2.3-8.16 2.3-6.26 0-11.57-4.22-13.47-9.91l-7.98 6.19C6.51 42.62 14.62 48 24 48z"/></svg>
        {isLoading ? "Connecting..." : "Sign in with Google"}
      </button>
      <p class="onboard-footer">Your data stays on your device.</p>
    </div>
  </main>
{:else}
  <div class="app-container">
    <aside class="pane-sidebar" class:collapsed={sidebarCollapsed}>
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
                onclick={() => switchAccount(account.id)}
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
            <button class="dropdown-item add-item" onclick={addAccount}
              >{@html iconPlus} Add Account</button
            >
          </div>
        {/if}
      </div>

      <div class="sidebar-compose">
        <button
          class="btn-sidebar flex-grow sidebar-compose-btn"
          onclick={() => openCompose()}
        >
          <span class="icon">{@html iconPlus}</span><span class="sidebar-text"> Compose</span>
        </button>
        <button
          class="btn-sidebar sidebar-calendar-btn"
          onclick={() => (showCalendar = !showCalendar)}
          title="Toggle Calendar"
        >
          {@html iconCalendar}
        </button>
      </div>

      <div class="sidebar-content">
        <h2 class="sidebar-heading">Mailboxes</h2>
        <ul class="sidebar-menu">
          {#each $labels.filter((l) => l.type === "system" && !l.name.startsWith("CATEGORY_") && l.name !== "UNREAD") as label}
            <li>
              <div
                class="sidebar-item {$selectedLabelId === label.id
                  ? 'active'
                  : ''}"
                role="button"
                tabindex="0"
                onclick={() => selectLabel(label.id)}
                onkeydown={(e) => {
                  if (e.key === "Enter" || e.key === " ") selectLabel(label.id);
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
        </ul>

        {#if !sidebarCollapsed && $labels.filter((l) => l.type === "user").length > 0}
          <h2 class="sidebar-heading">Labels</h2>
          <ul class="sidebar-menu">
            {#each $labels.filter((l) => l.type === "user") as label}
              <li>
                <div
                  class="sidebar-item {$selectedLabelId === label.id
                    ? 'active'
                    : ''}"
                  role="button"
                  tabindex="0"
                  onclick={() => selectLabel(label.id)}
                  onkeydown={(e) => {
                    if (e.key === "Enter" || e.key === " ")
                      selectLabel(label.id);
                  }}
                >
                  <span class="icon">{@html getLabelIcon("FOLDER")}</span>
                  <span class="label-text">{label.name}</span>
                  {#if label.unread_count > 0}<span class="badge"
                      >{label.unread_count}</span
                    >{/if}
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </div>

      <div class="sidebar-bottom">
        <div class="sidebar-bottom-row">
          <button
            onclick={() => performSync(true)}
            disabled={$isSyncing}
            class="btn-sidebar flex-grow"
          >
            <span class="icon {$isSyncing ? 'spin' : ''}"
              >{@html iconRefresh}</span
            >
            <span class="sidebar-text">{$isSyncing ? "Syncing…" : "Refresh"}</span>
          </button>
          <button
            onclick={cycleTheme}
            class="btn-sidebar btn-theme"
            title="{themeLabel}"
          >
            <span class="icon">{@html themeIcon}</span>
          </button>
        </div>
        <div class="sidebar-bottom-row">
          <button onclick={() => (showSettings = true)} class="btn-sidebar flex-grow">
            <span class="icon">{@html iconSettings}</span><span class="sidebar-text">Settings</span>
          </button>
          <button
            onclick={toggleSidebar}
            class="btn-sidebar btn-theme"
            title={sidebarCollapsed ? "Expand sidebar ([)" : "Collapse sidebar ([)"}
          >
            <span class="icon">{@html sidebarCollapsed ? iconSidebarExpand : iconSidebarCollapse}</span>
          </button>
        </div>
        {#if $lastSyncError}<div class="error sidebar-error">
            {$lastSyncError}
          </div>{/if}
      </div>
    </aside>

    <section class="pane-list">
      <div class="titlebar-spacer" data-tauri-drag-region></div>
      <div class="search-container" data-tauri-drag-region>
        <div class="search-bar">
          <span class="search-icon">{@html iconSearch}</span>
          <input
            type="text"
            class="search-input"
            placeholder="Search mail… (/)"
            bind:value={searchInput}
            bind:this={searchInputEl}
            oninput={onSearchInput}
            onkeydown={onSearchKeydown}
            onfocus={() => {
              fetchSuggestions();
              showSearchSuggestions = true;
            }}
            onblur={() =>
              setTimeout(() => (showSearchSuggestions = false), 200)}
          />
          {#if searchInput}<button class="search-clear" onclick={clearSearch}
              >{@html iconClose}</button
            >{/if}
          {#if $isSearching}<span class="search-spinner"></span>{/if}
        </div>

        {#if showSearchSuggestions && (searchSuggestions.length > 0 || !searchInput)}
          <div class="suggestions-dropdown">
            {#each searchSuggestions as s}
              <button
                class="suggestion-item"
                onmousedown={() => applySuggestion(s.text)}
              >
                <span class="suggestion-icon">
                  {#if s.kind === "recent"}
                    {@html iconHistory}
                  {:else if s.kind === "contact"}
                    {@html iconUser}
                  {:else if s.kind === "filter" || s.kind === "hint"}
                    {@html iconSearch}
                  {:else}
                    {@html iconTag}
                  {/if}
                </span>
                <span class="suggestion-text">{s.text}</span>
                {#if s.detail}<span class="suggestion-detail">{s.detail}</span>{/if}
              </button>
            {/each}
            {#if !searchInput || (parseSearchContext(searchInput, searchInputEl?.selectionStart ?? searchInput.length).operator === null)}
              {@const usedOperators = searchInput.match(/\b(from|to|subject|has|is|before|after):/g)?.map((o: string) => o) ?? []}
              {@const allFilters = [["from:", "From sender"], ["to:", "To recipient"], ["subject:", "Subject contains"], ["has:", "Has…"], ["is:", "Is…"], ["before:", "Before date"], ["after:", "After date"]]}
              {@const available = allFilters.filter(([val]) => !usedOperators.includes(val))}
              {#if available.length > 0}
                <div class="suggestion-section">Quick Filters</div>
                {#each available as [val, label]}
                  <button
                    class="suggestion-item filter"
                    onmousedown={() => {
                      const cursorPos = searchInputEl?.selectionStart ?? searchInput.length;
                      const before = searchInput.slice(0, cursorPos);
                      const after = searchInput.slice(cursorPos);
                      const needsSpace = before.length > 0 && !before.endsWith(" ");
                      searchInput = `${before}${needsSpace ? " " : ""}${val}${after}`;
                      onSearchInput();
                      // Move cursor after the inserted operator
                      setTimeout(() => {
                        const newPos = before.length + (needsSpace ? 1 : 0) + val.length;
                        searchInputEl?.setSelectionRange(newPos, newPos);
                        searchInputEl?.focus();
                      }, 0);
                    }}
                  >
                    <span class="suggestion-icon">{@html iconSearch}</span>
                    <span class="suggestion-text">{label}</span>
                    <span class="suggestion-detail">{val}</span>
                  </button>
                {/each}
              {/if}
            {/if}
          </div>
        {/if}
      </div>

      <div class="list-header">
        <h3>{$searchQuery ? "Search Results" : getActiveLabelName()}</h3>
        <span class="thread-count">{$threads.length}{hasMore ? "+" : ""}</span>
      </div>

      <div class="thread-scroll-area" bind:this={threadScrollArea}>
        {#if $threads.length === 0 && ($isSyncing || isLabelFetching || isLoadingThreads)}
          {#each Array(8) as _}
            <div class="skeleton-thread">
              <div class="skeleton-dot"></div>
              <div class="skeleton-content">
                <div class="skeleton-line w60"></div>
                <div class="skeleton-line w80"></div>
                <div class="skeleton-line w40"></div>
              </div>
            </div>
          {/each}
        {:else if $threads.length === 0}
          <div class="empty-state">
            {#if $searchQuery}No results for "{$searchQuery}"
            {:else}No messages here.{/if}
          </div>
        {:else}
          {#each $threads as thread (thread.id)}
            <div
              class="thread-item {thread.unread > 0
                ? 'unread'
                : ''} {$selectedThreadId === thread.id ? 'selected' : ''}"
              role="button"
              tabindex="0"
              onclick={() => selectThread(thread.id)}
              onkeydown={(e) => {
                if (e.key === "Enter" || e.key === " ") selectThread(thread.id);
              }}
            >
              <div class="thread-item-leading">
                <button
                  class="thread-star {thread.starred ? 'starred' : ''}"
                  onclick={(e) => {
                    e.stopPropagation();
                    toggleStar(thread.id, thread.starred);
                  }}
                >
                  {@html thread.starred ? iconStarFilled : iconStar}
                </button>
                <div class="thread-unread-dot"></div>
              </div>
              <div class="thread-content">
                <div class="thread-content-header">
                  <span class="thread-sender">{thread.sender}</span>
                  <span class="thread-time"
                    >{formatTime(thread.internal_date)}</span
                  >
                </div>
                <div class="thread-subject">{thread.subject}</div>
                <div class="thread-snippet">
                  {decodeEntities(thread.snippet)}
                </div>
              </div>
            </div>
          {/each}

          {#if hasMore}
            <div class="load-more-sentinel" bind:this={loadingSentinel}>
              {#if isLoadingMore}
                <div class="loading-more">
                  <div class="loading-spinner"></div>
                </div>
              {/if}
            </div>
          {/if}
        {/if}
      </div>
    </section>

    <main class="pane-view">
      <div class="titlebar-spacer" data-tauri-drag-region>
        {#if !isMacOS}
          <div class="window-controls">
            <button class="win-ctrl win-minimize" onclick={() => getCurrentWindow().minimize()} title="Minimize">
              <svg width="10" height="1" viewBox="0 0 10 1"><rect width="10" height="1" fill="currentColor"/></svg>
            </button>
            <button class="win-ctrl win-maximize" onclick={() => getCurrentWindow().toggleMaximize()} title="Maximize">
              <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1"/></svg>
            </button>
            <button class="win-ctrl win-close" onclick={() => getCurrentWindow().close()} title="Close">
              <svg width="10" height="10" viewBox="0 0 10 10"><line x1="0" y1="0" x2="10" y2="10" stroke="currentColor" stroke-width="1.2"/><line x1="10" y1="0" x2="0" y2="10" stroke="currentColor" stroke-width="1.2"/></svg>
            </button>
          </div>
        {/if}
      </div>
      {#if $selectedThreadId}
        <div class="message-toolbar" data-tauri-drag-region>
          <button
            onclick={() => executeAction("archive")}
            class="toolbar-btn"
            title="Archive (E)"
          >
            <span class="toolbar-icon">{@html iconArchive}</span><span
              >Archive</span
            >
          </button>
          {#if $selectedLabelId === "TRASH"}
            <button
              onclick={() => executeAction("untrash")}
              class="toolbar-btn"
              title="Restore from Trash"
            >
              <span class="toolbar-icon">{@html iconInbox}</span><span>Restore</span>
            </button>
          {:else}
            <button
              onclick={() => executeAction("trash")}
              class="toolbar-btn"
              title="Delete (#)"
            >
              <span class="toolbar-icon">{@html iconTrash}</span><span>Trash</span>
            </button>
          {/if}
          <button
            onclick={() => executeAction("unread")}
            class="toolbar-btn"
            title="Mark Unread (Shift + I)"
          >
            <span class="toolbar-icon">{@html iconMail}</span><span>Unread</span
            >
          </button>
        </div>
        {#if $isMessagesLoading}
          <div class="message-scroll-area">
            {#each Array(2) as _}
              <div class="skeleton-message">
                <div class="skeleton-msg-header">
                  <div class="skeleton-line w40"></div>
                  <div class="skeleton-line w20"></div>
                </div>
                <div
                  class="skeleton-line w60"
                  style="height:18px;margin-bottom:12px"
                ></div>
                <div class="skeleton-line w100"></div>
                <div class="skeleton-line w90"></div>
                <div class="skeleton-line w70"></div>
              </div>
            {/each}
          </div>
        {:else if $messagesError}
          <div class="error-state">{$messagesError}</div>
        {:else if $currentMessages.length > 0}
          <div class="message-scroll-area">
            {#each $currentMessages as msg}
              <div class="message-card animate-in">
                <div class="message-header">
                  <div class="msg-sender">{msg.sender || "Unknown Sender"}</div>
                  <div
                    class="message-header-right"
                    style="display: flex; align-items: center; gap: 12px;"
                  >
                    <div class="msg-time">{formatTime(msg.internal_date)}</div>
                    <div
                      class="message-actions"
                      style="display: flex; gap: 2px;"
                    >
                      {#if msg.is_draft}
                        <button
                          class="msg-action-btn"
                          onclick={() => handleEditDraft(msg)}
                          data-tooltip="Edit Draft"
                          style="width: auto; padding: 0 12px; font-size: 13px; font-weight: 500;"
                        >
                          <span
                            style="display: flex; align-items: center; gap: 6px;"
                          >
                            <span class="icon">{@html iconDraft}</span>
                            <span>Edit Draft</span>
                          </span>
                        </button>
                      {:else}
                        <button
                          class="msg-action-btn"
                          onclick={() => handleReply(msg)}
                          title="Reply (R)"
                          data-tooltip="Reply (R)"
                        >
                          {@html iconReply}
                        </button>
                        <button
                          class="msg-action-btn"
                          onclick={() => handleReplyAll(msg)}
                          data-tooltip="Reply All"
                        >
                          {@html iconReplyAll}
                        </button>
                        <button
                          class="msg-action-btn"
                          onclick={() => handleForward(msg)}
                          data-tooltip="Forward"
                        >
                          {@html iconForward}
                        </button>
                      {/if}
                    </div>
                  </div>
                </div>
                <h2 class="msg-subject">{msg.subject}</h2>
                <div class="message-body">
                  {#if msg.body_html}
                    <iframe
                      title="Email Body"
                      sandbox="allow-scripts"
                      style="width:100%;height:0;border:none;overflow:hidden;background:#f5f5f5;border-radius:6px;opacity:0;transition:opacity .15s;"
                      srcdoc={`<html><head><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src https: http: data: cid:;"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light only"></head><body style="margin:0;padding:0;background:#f5f5f5;overflow:hidden;"><div style="max-width:680px;margin:0 auto;padding:12px;">${msg.body_html}</div><script>(function(){var b=document.body;function post(type,data){parent.postMessage(Object.assign({type:type},data),'*');}function resize(){post('rustymail-resize',{height:b.scrollHeight});}resize();new ResizeObserver(resize).observe(b);b.querySelectorAll('img').forEach(function(img){if(!img.complete)img.addEventListener('load',resize,{once:true});});document.addEventListener('click',function(e){var t=e.target;while(t&&t.tagName!=='A')t=t.parentElement;if(!t||!t.href)return;e.preventDefault();post('rustymail-link',{url:t.href});},true);})();<\/script></body></html>`}
                      onload={(e) => {
                        const iframe = e.currentTarget as HTMLIFrameElement;
                        if (iframe.contentWindow) {
                          iframeWindows.set(iframe.contentWindow, iframe);
                        }
                      }}
                    ></iframe>
                  {:else if msg.body_plain}
                    <pre class="plain-body">{msg.body_plain}</pre>
                  {:else}
                    <p class="no-body">
                      Message body not available. Try refreshing.
                    </p>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <div class="empty-state">No messages loaded for this thread.</div>
        {/if}
      {:else}
        <div class="empty-state centered-empty">
          <div class="empty-icon">{@html iconInbox}</div>
          <p>Select a conversation to read</p>
          <span class="empty-hint">Press <kbd>/</kbd> to search</span>
        </div>
      {/if}
    </main>
  </div>

  <Settings
    bind:show={showSettings}
    accounts={allAccounts}
    onclose={() => {
      showSettings = false;
      checkAndSetupSync();
    }}
    onAccountSwitch={switchAccount}
    onAccountAdd={addAccount}
    onAccountRemove={removeAccount}
    onThemeChange={(mode) => applyTheme(mode as ThemeMode, false)}
  />
{/if}

{#key composeKey}
  {#if showCompose}
    <Compose
      onClose={() => (showCompose = false)}
      {...composeProps}
      onDraftSaved={(id) => (composeProps.initialDraftId = id)}
    />
  {/if}
{/key}

{#if showCalendar}
  <CalendarSidebar onClose={() => (showCalendar = false)} />
{/if}

{#if pendingLinkUrl}
  <div class="link-overlay" role="button" tabindex="-1" onclick={dismissLinkDialog} onkeydown={(e) => { if (e.key === 'Escape') dismissLinkDialog(); }}>
    <div class="link-dialog" role="dialog" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <div class="link-dialog-body">
        {#if pendingLinkAnalysis}
          <div class="link-shield link-shield-{pendingLinkAnalysis.risk}">
            <svg width="28" height="28" viewBox="0 0 24 24" fill="none">
              {#if pendingLinkAnalysis.risk === 'safe'}
                <path d="M12 2L3 7v5c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V7l-9-5zm-1 14.5l-3.5-3.5 1.41-1.41L11 13.67l5.09-5.09L17.5 10 11 16.5z" fill="currentColor"/>
              {:else if pendingLinkAnalysis.risk === 'caution'}
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
          <span class="link-url-text">{pendingLinkUrl}</span>
        </div>
        {#if pendingLinkAnalysis && pendingLinkAnalysis.risk !== 'safe'}
          <div class="link-warning link-warning-{pendingLinkAnalysis.risk}">
            {#each pendingLinkAnalysis.reasons as reason}
              <p class="link-warning-line">{reason}</p>
            {/each}
          </div>
        {/if}
      </div>
      <div class="link-dialog-actions">
        <button class="link-action link-action-cancel" onclick={dismissLinkDialog}>Cancel</button>
        <button class="link-action link-action-open link-action-{pendingLinkAnalysis?.risk ?? 'safe'}" onclick={confirmOpenLink}>
          {pendingLinkAnalysis?.risk === 'danger' ? 'Open Anyway' : 'Open Link'}
        </button>
      </div>
    </div>
  </div>
{/if}

<Toasts />

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
  .window-controls {
    display: flex;
    align-items: center;
    height: 100%;
    -webkit-app-region: no-drag;
  }
  .win-ctrl {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 46px;
    height: 100%;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.1s;
  }
  .win-ctrl:hover {
    background: var(--bg-hover, rgba(128, 128, 128, 0.2));
  }
  .win-close:hover {
    background: #e81123;
    color: #fff;
  }
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
  .btn-google {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    width: 100%;
    background: var(--text-primary, #1c1c1e);
    color: var(--bg-sidebar, #ffffff);
    border: none;
    padding: 12px 24px;
    font-size: 14px;
    font-weight: 500;
    border-radius: 10px;
    cursor: pointer;
    transition: opacity 0.15s;
    font-family: inherit;
  }
  .btn-google:hover {
    opacity: 0.85;
  }
  .btn-google:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
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
  .error {
    color: #ff453a;
    font-size: 12px;
    line-height: 15px;
    font-weight: 300;
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
    font-size: 13px;
    line-height: 16px;
    letter-spacing: -0.08px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .brand-email {
    display: block;
    font-size: 10px;
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
    border-radius: 10px;
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
    font-size: 12px;
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
    font-size: 10px;
    color: var(--text-secondary);
  }
  .dropdown-text {
    flex: 1;
    overflow: hidden;
  }
  .dropdown-name {
    display: block;
    font-weight: 500;
    font-size: 12px;
    line-height: 15px;
  }
  .dropdown-email {
    display: block;
    font-size: 10px;
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
  .sidebar-content {
    flex: 1;
    overflow-y: auto;
    padding: 12px;
  }
  .sidebar-heading {
    font-size: 11px;
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
  .sidebar-item {
    display: flex;
    align-items: center;
    padding: 6px 12px;
    margin: 2px 0;
    border-radius: 8px;
    font-size: 13px;
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
    font-size: 11px;
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
    border-radius: 6px;
    color: var(--text-primary);
    cursor: pointer;
    font-size: 12px;
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
    font-size: 13px;
    font-weight: 500;
    padding: 7px 14px;
    background: var(--accent-blue);
    color: white;
    border: none;
    border-radius: 8px;
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
  .sidebar-error {
    margin-top: 8px;
    font-size: 11px;
    line-height: 14px;
    padding: 0 4px;
  }
  .spin {
    animation: spin 0.8s linear infinite;
  }

  .search-container {
    padding: 10px 12px 0;
    position: relative;
  }
  .search-bar {
    display: flex;
    align-items: center;
    background: var(--bg-sidebar);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    padding: 0 10px;
    height: 34px;
    transition:
      border-color 0.15s ease,
      box-shadow 0.15s ease;
  }
  .search-bar:focus-within {
    border-color: var(--accent-blue);
    box-shadow: 0 0 0 3px rgba(10, 132, 255, 0.15);
  }
  .search-icon {
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    flex-shrink: 0;
    margin-right: 8px;
  }
  .search-input {
    flex: 1;
    border: none;
    background: transparent;
    outline: none;
    font-size: 13px;
    line-height: 16px;
    letter-spacing: -0.08px;
    color: var(--text-primary);
    font-family: var(--font-family);
  }
  .search-input::placeholder {
    color: var(--text-secondary);
  }
  .search-clear {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    padding: 2px;
    border-radius: 50%;
  }
  .search-clear:hover {
    color: var(--text-primary);
  }
  .search-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid var(--border-color);
    border-top-color: var(--accent-blue);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
    margin-left: 8px;
    flex-shrink: 0;
  }

  .suggestions-dropdown {
    position: absolute;
    left: 12px;
    right: 12px;
    top: 48px;
    background: var(--bg-view);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.12);
    z-index: 50;
    max-height: 240px;
    overflow-y: auto;
  }
  .suggestion-section {
    padding: 6px 12px;
    font-size: 10px;
    line-height: 13px;
    text-transform: uppercase;
    color: var(--text-secondary);
    letter-spacing: 0.5px;
    font-weight: 600;
  }
  .suggestion-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    width: 100%;
    background: none;
    border: none;
    cursor: pointer;
    font-size: 12px;
    line-height: 15px;
    color: var(--text-primary);
    font-family: var(--font-family);
    text-align: left;
    transition: background 0.1s;
  }
  .suggestion-item:hover {
    background: var(--sidebar-hover);
  }
  .suggestion-icon {
    font-size: 12px;
    width: 18px;
    text-align: center;
    flex-shrink: 0;
  }
  .suggestion-text {
    flex: 1;
    font-weight: 500;
  }
  .suggestion-detail {
    font-size: 11px;
    line-height: 14px;
    color: var(--text-secondary);
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skeleton-thread {
    display: flex;
    padding: 12px 14px;
    gap: 8px;
    border-bottom: 1px solid var(--border-color);
  }
  .skeleton-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--border-color);
    margin-top: 5px;
    flex-shrink: 0;
    animation: shimmer 1.5s infinite;
  }
  .skeleton-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .skeleton-line {
    height: 10px;
    border-radius: 4px;
    background: var(--border-color);
    animation: shimmer 1.5s infinite;
  }
  .skeleton-line.w20 {
    width: 20%;
  }
  .skeleton-line.w40 {
    width: 40%;
  }
  .skeleton-line.w60 {
    width: 60%;
  }
  .skeleton-line.w70 {
    width: 70%;
  }
  .skeleton-line.w80 {
    width: 80%;
  }
  .skeleton-line.w90 {
    width: 90%;
  }
  .skeleton-line.w100 {
    width: 100%;
  }
  @keyframes shimmer {
    0%,
    100% {
      opacity: 0.4;
    }
    50% {
      opacity: 0.8;
    }
  }
  .skeleton-message {
    padding: 20px;
    margin: 16px 20px;
    border: 1px solid var(--border-color);
    border-radius: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .skeleton-msg-header {
    display: flex;
    justify-content: space-between;
    margin-bottom: 4px;
    gap: 20px;
  }

  .list-header {
    padding: 8px 16px;
    border-bottom: 1px solid var(--border-color);
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 36px;
  }
  .list-header h3 {
    font-weight: 600;
    font-size: 13px;
    line-height: 16px;
    letter-spacing: -0.08px;
    color: var(--text-primary);
  }
  .thread-count {
    font-size: 11px;
    line-height: 14px;
    color: var(--text-secondary);
    font-weight: 500;
  }
  .thread-scroll-area {
    flex: 1;
    overflow-y: auto;
    overflow-anchor: auto;
  }

  .empty-state {
    padding: 2rem;
    text-align: center;
    color: var(--text-secondary);
    font-size: 13px;
    line-height: 16px;
    letter-spacing: -0.08px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  .centered-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 8px;
    opacity: 0.5;
  }
  .empty-icon {
    width: 48px;
    height: 48px;
    color: var(--text-secondary);
    opacity: 0.25;
    margin-bottom: 8px;
  }
  .empty-icon :global(svg) {
    width: 48px;
    height: 48px;
  }
  .empty-hint {
    font-size: 11px;
    line-height: 14px;
    color: var(--text-secondary);
    opacity: 0.5;
  }
  .empty-hint kbd {
    background: var(--sidebar-hover);
    padding: 1px 6px;
    border-radius: 3px;
    font-size: 11px;
    font-family: monospace;
    border: 1px solid var(--border-color);
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
  .loading-more {
    display: flex;
    justify-content: center;
    padding: 12px;
  }
  .load-more-sentinel {
    min-height: 1px;
  }

  .thread-item {
    display: flex;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border-color);
    cursor: pointer;
    align-items: flex-start;
    transition: background 0.1s ease;
    width: 100%;
    text-align: left;
    font-family: var(--font-family);
    color: var(--text-primary);
    outline: none;
  }
  .thread-item:hover {
    background-color: var(--sidebar-hover);
    border-radius: 6px;
  }
  .thread-item.selected {
    background-color: rgba(10, 132, 255, 0.08);
  }
  .thread-item-leading {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    margin-right: 10px;
    flex-shrink: 0;
    width: 24px;
  }
  .thread-star {
    background: none;
    border: none;
    padding: 4px;
    cursor: pointer;
    color: var(--text-secondary);
    opacity: 0.4;
    transition: all 0.2s;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
  }
  .thread-star:hover {
    opacity: 1;
    background: rgba(255, 255, 255, 0.05);
  }
  .thread-star.starred {
    color: #f2a600;
    opacity: 1;
  }
  .thread-unread-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background-color: transparent;
    transition: background 0.2s;
  }
  .thread-item.unread .thread-unread-dot {
    background-color: var(--accent-blue);
  }
  .thread-content {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .thread-content-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .thread-sender {
    font-weight: 500;
    font-size: 13px;
    line-height: 16px;
    letter-spacing: -0.08px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .thread-time {
    font-size: 11px;
    line-height: 14px;
    color: var(--text-secondary);
    white-space: nowrap;
    margin-left: 8px;
    flex-shrink: 0;
    font-weight: 400;
  }
  .thread-subject {
    font-size: 13px;
    line-height: 16px;
    letter-spacing: -0.08px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-primary);
    font-weight: 400;
  }
  .thread-snippet {
    font-size: 12px;
    line-height: 15px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 400;
  }
  .thread-item.unread .thread-sender {
    font-weight: 700;
    color: var(--text-primary);
    -webkit-font-smoothing: auto;
  }
  .thread-item.unread .thread-subject {
    font-weight: 600;
    color: var(--text-primary);
    -webkit-font-smoothing: auto;
  }
  .thread-item.unread .thread-time {
    color: var(--text-primary);
    font-weight: 500;
  }
  .thread-item.unread .thread-snippet {
    color: var(--text-secondary);
    font-weight: 400;
  }

  .pane-view {
    display: flex;
    flex-direction: column;
    background: var(--bg-view);
    height: 100%;
  }
  .message-toolbar {
    height: 44px;
    display: flex;
    align-items: center;
    padding: 0 16px;
    border-bottom: 1px solid var(--border-color);
    gap: 4px;
    flex-shrink: 0;
  }
  .toolbar-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 12px;
    line-height: 15px;
    transition: background 0.1s;
    font-family: var(--font-family);
  }
  .toolbar-icon {
    display: flex;
    align-items: center;
    width: 16px;
    height: 16px;
  }
  .toolbar-btn:hover {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }

  /* Custom icon-only button style for message actions */
  .msg-action-btn {
    background: transparent;
    border: none;
    border-radius: 50%;
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary);
    cursor: pointer;
    transition: all 0.2s ease;
    position: relative;
  }
  .msg-action-btn:hover {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }
  .msg-action-btn :global(svg) {
    width: 14px;
    height: 14px;
  }

  /* Simple CSS Tooltip */
  .msg-action-btn::after {
    content: attr(data-tooltip);
    position: absolute;
    bottom: -30px;
    left: 50%;
    transform: translateX(-50%) translateY(5px);
    background: #333;
    color: #fff;
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 11px;
    white-space: nowrap;
    opacity: 0;
    visibility: hidden;
    transition: all 0.2s ease;
    z-index: 100;
    pointer-events: none;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
  }
  :global([data-theme="light"]) .msg-action-btn::after {
    background: #333;
    color: #fff;
  }
  :global([data-theme="dark"]) .msg-action-btn::after {
    background: #f0f0f0;
    color: #1c1c1e;
  }
  .msg-action-btn:hover::after {
    opacity: 1;
    visibility: visible;
    transform: translateX(-50%) translateY(0);
  }

  .message-scroll-area {
    flex: 1;
    overflow-y: auto;
    padding: 20px;
  }
  .error-state {
    padding: 2rem;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    color: #ff3b30;
  }
  .message-card {
    background: var(--bg-view);
    border: 1px solid var(--border-color);
    border-radius: 10px;
    padding: 20px;
    margin-bottom: 16px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04);
  }
  .animate-in {
    animation: fadeSlideIn 0.25s ease-out;
  }
  @keyframes fadeSlideIn {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  .message-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
    font-size: 13px;
    line-height: 16px;
    letter-spacing: -0.08px;
  }
  .msg-sender {
    font-weight: 600;
    color: var(--text-primary);
    font-size: 14px;
    line-height: 18px;
    letter-spacing: -0.08px;
  }
  .msg-time {
    color: var(--text-secondary);
    font-size: 12px;
    line-height: 15px;
    flex-shrink: 0;
  }
  .msg-subject {
    font-size: 17px;
    line-height: 22px;
    font-weight: 600;
    margin: 0 0 14px 0;
    letter-spacing: -0.1px;
    color: var(--text-primary);
  }
  .message-body {
    font-size: 14px;
    line-height: 1.6;
    color: var(--text-primary);
    overflow-x: hidden;
  }
  .plain-body {
    white-space: pre-wrap;
    font-family: inherit;
    font-size: 13px;
    line-height: 16px;
    letter-spacing: -0.08px;
    margin: 0;
    background: var(--bg-view);
    color: var(--text-primary);
    padding: 12px;
    border-radius: 6px;
  }
  .no-body {
    color: var(--text-secondary);
    font-style: italic;
    font-size: 13px;
    line-height: 16px;
    letter-spacing: -0.08px;
  }
</style>
