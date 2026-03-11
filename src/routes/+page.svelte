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
    formatLabelName,
  } from "$lib/components/icons";
  import Settings from "$lib/components/Settings.svelte";
  import Compose from "$lib/components/Compose.svelte";
  import CalendarSidebar from "$lib/components/CalendarSidebar.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import ThreadList from "$lib/components/ThreadList.svelte";
  import MessageDetail from "$lib/components/MessageDetail.svelte";
  import LinkSafetyDialog from "$lib/components/LinkSafetyDialog.svelte";
  import { addToast } from "$lib/stores/toast";
  import {
    formatTime,
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

  const labels = writable<LocalLabel[]>([]);
  const selectedLabelId = writable<string>("INBOX");
  const searchQuery = writable<string>("");
  const isSearching = writable<boolean>(false);

  let activeAccount = $state<AccountInfo | null>(null);
  let allAccounts = $state<AccountInfo[]>([]);
  let showSettings = $state(false);
  let isLoading = $state(false);
  let isLoadingThreads = $state(false);
  let showCompose = $state(false);
  let showCalendar = $state(false);

  let isMacOS = $state(false);
  let sidebarCollapsed = $state(false);
  let density = $state("default");
  let readingPane = $state("right");
  let linkBehavior = $state("ask");
  let pendingLinkUrl = $state<string | null>(null);
  let pendingLinkAnalysis = $state<LinkAnalysis | null>(null);
  const iframeWindows = new Map<Window, HTMLIFrameElement>();

  let threadListRef = $state<ThreadList>();

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

    try {
      isSyncing.set(true);
      lastSyncError.set(null);

      await invoke("sync_gmail_data", { labelId: syncLabelId });

      if (get(selectedLabelId) !== syncLabelId) return;

      if (!get(searchQuery)) {
        if (isManual) {
          threadOffset = 0;
          hasMore = true;
          await loadThreads(true);
        } else {
          await loadThreads(true, true);
        }
      }

      await loadLabels();

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

      if ((get(selectedLabelId) || null) !== invocationLabelId) return;

      if (reset && fetched.length === 0 && invocationLabelId && !silent) {
        isLabelFetching = true;
        try {
          await invoke("fetch_label_threads", { labelId: invocationLabelId });

          if ((get(selectedLabelId) || null) !== invocationLabelId) return;

          if (invocationLabelId)
            labelLastSyncMap[invocationLabelId] = Date.now();
          const retried = (await invoke("get_threads", {
            labelId: invocationLabelId,
            offset: 0,
            limit: THREAD_PAGE_SIZE,
          })) as LocalThread[];

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
    threadListRef?.observeSentinel();
  }

  async function loadMoreThreads() {
    if (isLoadingMore || !hasMore) return;
    isLoadingMore = true;

    const invocationLabelId = get(selectedLabelId) || null;

    try {
      const fetched = (await invoke("get_threads", {
        labelId: invocationLabelId,
        offset: threadOffset,
        limit: THREAD_PAGE_SIZE,
      }).catch(() => [])) as LocalThread[];

      if ((get(selectedLabelId) || null) !== invocationLabelId) return;

      if (fetched.length > 0) {
        threads.update((t) => [...t, ...fetched]);
        hasMore = fetched.length >= THREAD_PAGE_SIZE;
        threadOffset += fetched.length;
      } else if (invocationLabelId) {
        try {
          await invoke("fetch_label_threads", { labelId: invocationLabelId });

          if ((get(selectedLabelId) || null) !== invocationLabelId) return;

          const retried = (await invoke("get_threads", {
            labelId: invocationLabelId,
            offset: threadOffset,
            limit: THREAD_PAGE_SIZE,
          }).catch(() => [])) as LocalThread[];

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
      threadListRef?.observeSentinel();
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
    threadListRef?.clearSearchInput();
    searchQuery.set("");
    threadOffset = 0;
    hasMore = true;

    if (!isReselect) {
      threads.set([]);
    }

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

  async function handleSearch(query: string) {
    if (!query) {
      searchQuery.set("");
      await loadThreads(true);
      return;
    }
    searchQuery.set(query);
    isSearching.set(true);
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

  function clearSearch() {
    searchQuery.set("");
    loadThreads(true);
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
    } catch (e) {
      console.error("Switch account failed", e);
    }
  }

  async function addAccount() {
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
      threadListRef?.focusSearch();
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

    await refreshAccountState();
    if (appState === "authenticated") {
      await loadLabels();
      await loadThreads(true);
      await checkAndSetupSync();
    }

    setTimeout(() => threadListRef?.initObserver(), 100);
    setTimeout(() => checkForUpdates(true), 5000);
  });

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
      {labels}
      {selectedLabelId}
      oncompose={() => openCompose()}
      onsync={() => performSync(true)}
      onthemecycle={cycleTheme}
      ontogglecalendar={() => (showCalendar = !showCalendar)}
      onsettings={() => (showSettings = true)}
      ontogglecollapse={toggleSidebar}
      onselectlabel={selectLabel}
      onswitchaccount={switchAccount}
      onaddaccount={addAccount}
    />

    <ThreadList
      bind:this={threadListRef}
      {isLoadingThreads}
      {isLabelFetching}
      {isMacOS}
      {hasMore}
      {isLoadingMore}
      activeLabelName={getActiveLabelName()}
      {searchQuery}
      {isSearching}
      onselectthread={selectThread}
      ontogglestar={toggleStar}
      onloadmore={loadMoreThreads}
      onsearch={handleSearch}
      onclearsearch={clearSearch}
    />

    <MessageDetail
      {isMacOS}
      isTrashView={$selectedLabelId === "TRASH"}
      onaction={executeAction}
      onreply={handleReply}
      onreplyall={handleReplyAll}
      onforward={handleForward}
      oneditdraft={handleEditDraft}
      oniframeload={handleIframeLoad}
    />
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
    onDensityChange={(d) => density = d}
    onReadingPaneChange={(p) => readingPane = p}
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
    />
  {/if}
{/key}

{#if showCalendar}
  <CalendarSidebar onClose={() => (showCalendar = false)} />
{/if}

<LinkSafetyDialog
  url={pendingLinkUrl}
  analysis={pendingLinkAnalysis}
  onconfirm={confirmOpenLink}
  ondismiss={dismissLinkDialog}
/>

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
</style>
