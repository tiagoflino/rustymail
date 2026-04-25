<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { type Writable } from "svelte/store";
  import {
    iconSearch,
    iconClose,
    iconHistory,
    iconUser,
    iconTag,
    iconImportantArrow,
    iconImportantArrowFilled,
  } from "$lib/components/icons";
  import { getStarIcon, getStarColor } from "$lib/components/starIcons";
  import { threads, isSyncing } from "$lib/stores/threads";
  import { selectedThreadId, selectedThreadIds, lastSelectedIndex, toggleThreadSelection, clearSelection, selectAll } from "$lib/stores/messages";
  import { formatTime, decodeEntities } from "$lib/utils/formatters.js";
  import CategoryTabs from "./CategoryTabs.svelte";

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

  // Predefined account color palette (Apple HIG complementary)
  const ACCOUNT_COLORS = [
    '#007AFF', // blue
    '#FF9500', // orange
    '#34C759', // green
    '#AF52DE', // purple
    '#FF3B30', // red
    '#5AC8FA', // teal
    '#FF2D55', // pink
    '#FFCC00', // yellow
  ];

  function getAccountColor(accountId: string, accounts: AccountInfo[]): string {
    const idx = accounts.findIndex(a => a.id === accountId);
    return ACCOUNT_COLORS[idx >= 0 ? idx % ACCOUNT_COLORS.length : 0];
  }

  function getAccountInitial(accountId: string, accounts: AccountInfo[]): string {
    const acc = accounts.find(a => a.id === accountId);
    if (!acc) return '?';
    return (acc.display_name || acc.email || '?')[0].toUpperCase();
  }

  interface Props {
    isLoadingThreads: boolean;
    isLabelFetching: boolean;
    isMacOS: boolean;
    currentPage: number;
    threadsPerPage: number;
    totalCount: number;
    hasMoreRemote: boolean;
    gmailTotal: number | null;
    isBackgroundFilling: boolean;
    activeLabelName: string;
    searchQuery: Writable<string>;
    isSearching: Writable<boolean>;
    showCategoryTabs: boolean;
    selectedCategory: Writable<string>;
    unifiedIndicator: string;
    allAccounts: AccountInfo[];
    isUnifiedView: boolean;
    onselectthread: (threadId: string) => void;
    ontogglestar: (threadId: string, starType: string | null) => void;
    ontoggleimportant: (threadId: string, important: boolean) => void;
    onfirstpage: () => void;
    onprevpage: () => void;
    onnextpage: () => void;
    onsearch: (query: string) => void;
    onclearsearch: () => void;
    onselectcategory: (category: string) => void;
    onbatcharchive: (ids: string[]) => void;
    onbatchtrash: (ids: string[]) => void;
    onbatchrestore: (ids: string[]) => void;
    onbatchread: (ids: string[], isRead: boolean) => void;
    onbatchstar: (ids: string[], starred: boolean) => void;
    onbatchsnooze: (ids: string[], until: number) => void;
    onbatchunsnooze: (ids: string[]) => void;
    onbatchmovetolabel: (ids: string[], labelId: string) => void;
    isSnoozedView?: boolean;
    isTrashView?: boolean;
    hasSuperstars?: boolean;
    hasImportant?: boolean;
    accountProviderTypes?: Record<string, string>;
  }

  let {
    isLoadingThreads,
    isLabelFetching,
    isMacOS,
    currentPage,
    threadsPerPage,
    totalCount,
    hasMoreRemote,
    gmailTotal,
    isBackgroundFilling,
    activeLabelName,
    searchQuery,
    isSearching,
    showCategoryTabs,
    selectedCategory,
    unifiedIndicator = 'avatar',
    allAccounts = [],
    isUnifiedView = false,
    onselectthread,
    ontogglestar,
    ontoggleimportant,
    onfirstpage,
    onprevpage,
    onnextpage,
    onsearch,
    onclearsearch,
    onselectcategory,
    onbatcharchive,
    onbatchtrash,
    onbatchrestore,
    onbatchread,
    onbatchstar,
    onbatchsnooze,
    onbatchunsnooze,
    onbatchmovetolabel,
    isSnoozedView = false,
    isTrashView = false,
    hasSuperstars = false,
    hasImportant = false,
    accountProviderTypes = {},
  }: Props = $props();

  let searchInput = $state("");
  let searchInputEl = $state<HTMLInputElement>();
  let showSearchSuggestions = $state(false);
  let searchSuggestions = $state<SearchSuggestion[]>([]);
  let searchTimeout: ReturnType<typeof setTimeout> | null = null;
  let threadScrollArea = $state<HTMLDivElement>();

  export function focusSearch() {
    searchInputEl?.focus();
  }

  export function clearSearchInput() {
    searchInput = "";
    showSearchSuggestions = false;
    searchSuggestions = [];
  }

  export function getThreadScrollArea() {
    return threadScrollArea;
  }

  export function resetScroll() {
    if (threadScrollArea) threadScrollArea.scrollTop = 0;
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
      if (searchInput.trim().length >= 3) onsearch(searchInput.trim());
    }, 400);
  }

  function onSearchKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      onsearch(searchInput.trim());
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
      const beforeCursor = searchInput.slice(0, cursorPos);
      const lastSpace = beforeCursor.lastIndexOf(" ");
      const prefix = searchInput.slice(0, lastSpace + 1);
      const afterCursor = searchInput.slice(cursorPos);
      searchInput = `${prefix}${ctx.operator}:${text} ${afterCursor}`.replace(/  +/g, " ");
    } else {
      searchInput = text;
    }
    showSearchSuggestions = false;
    const trimmed = searchInput.trim();
    if (trimmed && !trimmed.endsWith(":")) {
      onsearch(trimmed);
    }
  }

  function clearSearch() {
    searchInput = "";
    showSearchSuggestions = false;
    searchSuggestions = [];
    onclearsearch();
  }
</script>

<section class="pane-list">
  <div class="titlebar-spacer" data-tauri-drag-region></div>
  <div class="search-container" data-tauri-drag-region>
    <div class="search-bar">
      <span class="search-icon">{@html iconSearch}</span>
      <input
        type="text"
        class="search-input"
        placeholder="Search mail... (/)"
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
          {@const allFilters = [["from:", "From sender"], ["to:", "To recipient"], ["subject:", "Subject contains"], ["has:attachment", "Has attachment"], ["is:unread", "Is unread"], ["is:read", "Is read"], ["before:", "Before date"], ["after:", "After date"]]}
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

  {#if showCategoryTabs && !$searchQuery}
    <CategoryTabs
      {selectedCategory}
      {onselectcategory}
    />
  {/if}

  <div class="list-header">
    {#if $selectedThreadIds.size === 0}
    <h3>
      {$searchQuery ? "Search Results" : activeLabelName}
      {#if isUnifiedView && !$searchQuery}
        <span class="unified-badge">Unified</span>
      {/if}
    </h3>
    {#if !$searchQuery && ($threads.length > 0 || isLoadingThreads || isBackgroundFilling)}
      <div class="pagination-controls">
        <span class="pagination-range">
          {#if isBackgroundFilling && $threads.length === 0}
            Loading...
          {:else}
            {currentPage * threadsPerPage + 1}–{currentPage * threadsPerPage + $threads.length}
            {#if gmailTotal !== null}
              of {gmailTotal.toLocaleString()}
            {:else if hasMoreRemote}
              of many
            {:else if totalCount > 0}
              of {totalCount.toLocaleString()}
            {/if}
          {/if}
        </span>
        <button
          class="pagination-btn"
          disabled={currentPage === 0 || isLoadingThreads}
          onclick={onfirstpage}
          title="Newest"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="17 18 11 12 17 6"/><line x1="7" y1="6" x2="7" y2="18"/></svg>
        </button>
        <button
          class="pagination-btn"
          disabled={currentPage === 0 || isLoadingThreads}
          onclick={onprevpage}
          title="Newer"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>
        </button>
        <button
          class="pagination-btn"
          disabled={isLoadingThreads || (!hasMoreRemote && (gmailTotal === null || (currentPage + 1) * threadsPerPage >= gmailTotal) && (currentPage + 1) * threadsPerPage >= totalCount)}
          onclick={onnextpage}
          title="Older"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>
        </button>
      </div>
    {/if}
    {:else}
      <div class="bulk-toolbar">
        <div class="bulk-toolbar-left">
          <div class="bulk-select-all" onclick={() => {
            const allIds = $threads.map(t => t.id);
            if ($selectedThreadIds.size === $threads.length) clearSelection();
            else selectAll(allIds);
          }} onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); const allIds = $threads.map(t => t.id); if ($selectedThreadIds.size === $threads.length) clearSelection(); else selectAll(allIds); }}} role="checkbox" aria-checked={$selectedThreadIds.size === $threads.length ? 'true' : $selectedThreadIds.size > 0 ? 'mixed' : 'false'} tabindex="0">
            {#if $selectedThreadIds.size === $threads.length}
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                <rect x="0.5" y="0.5" width="13" height="13" rx="3" fill="#0A84FF" stroke="#0A84FF"/>
                <path d="M3.5 7L6 9.5L10.5 4.5" stroke="white" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            {:else}
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                <rect x="0.5" y="0.5" width="13" height="13" rx="3" fill="#0A84FF" stroke="#0A84FF"/>
                <line x1="4" y1="7" x2="10" y2="7" stroke="white" stroke-width="1.5" stroke-linecap="round"/>
              </svg>
            {/if}
          </div>
          <span class="bulk-count">{$selectedThreadIds.size} selected</span>
          <div class="bulk-separator"></div>
          <button class="bulk-action" title="Archive" onclick={() => onbatcharchive([...$selectedThreadIds])}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="21 8 21 21 3 21 3 8"/><rect x="1" y="3" width="22" height="5"/><line x1="10" y1="12" x2="14" y2="12"/></svg>
          </button>
          {#if isTrashView}
            <button class="bulk-action" title="Restore from Trash" onclick={() => onbatchrestore([...$selectedThreadIds])}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="1 4 1 10 7 10"/><path d="M3.51 15a9 9 0 105.64-11.36L1 10"/></svg>
            </button>
          {:else}
            <button class="bulk-action" title="Move to Trash" onclick={() => onbatchtrash([...$selectedThreadIds])}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"/></svg>
            </button>
          {/if}
          <div class="bulk-separator"></div>
          <button class="bulk-action" title={(() => { const selected = $threads.filter(t => $selectedThreadIds.has(t.id)); return selected.some(t => t.unread > 0) ? 'Mark as Read' : 'Mark as Unread'; })()} onclick={() => { const selected = $threads.filter(t => $selectedThreadIds.has(t.id)); const hasUnread = selected.some(t => t.unread > 0); onbatchread([...$selectedThreadIds], hasUnread); }}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"/><polyline points="22,6 12,13 2,6"/></svg>
          </button>
          <button class="bulk-action" title={(() => { const selected = $threads.filter(t => $selectedThreadIds.has(t.id)); return selected.some(t => t.starred) ? 'Unstar' : 'Star'; })()} onclick={() => { const selected = $threads.filter(t => $selectedThreadIds.has(t.id)); const hasStarred = selected.some(t => t.starred); onbatchstar([...$selectedThreadIds], !hasStarred); }}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>
          </button>
          <div class="bulk-separator"></div>
          {#if isSnoozedView}
            <button class="bulk-action" title="Unsnooze" onclick={() => onbatchunsnooze([...$selectedThreadIds])}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="8" y1="12" x2="16" y2="12"/></svg>
            </button>
          {:else}
            <button class="bulk-action" title="Snooze" onclick={() => onbatchsnooze([...$selectedThreadIds], 0)}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
            </button>
          {/if}
          <button class="bulk-action" title="Move to Label" onclick={() => onbatchmovetolabel([...$selectedThreadIds], '')}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/><line x1="7" y1="7" x2="7.01" y2="7"/></svg>
          </button>
        </div>
        <button class="bulk-action bulk-close" title="Clear selection" onclick={() => clearSelection()}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
      </div>
    {/if}
  </div>

  <div class="thread-scroll-area" class:has-selection={$selectedThreadIds.size > 0} bind:this={threadScrollArea}>
    {#if $threads.length === 0 && ($isSyncing || isLabelFetching || isLoadingThreads || isBackgroundFilling)}
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
        {@const threadProvider = accountProviderTypes[thread.account_id]}
        {@const effectiveHasSuperstars = threadProvider ? threadProvider === 'gmail' : hasSuperstars}
        {@const effectiveHasImportant = threadProvider ? threadProvider === 'gmail' : hasImportant}
        <div
          class="thread-item {thread.unread > 0
            ? 'unread'
            : ''} {$selectedThreadId === thread.id ? 'selected' : ''} {$selectedThreadIds.has(thread.id) ? 'multi-selected' : ''}"
          class:unified={isUnifiedView}
          style={isUnifiedView ? `--account-color: ${getAccountColor(thread.account_id, allAccounts)}` : ''}
          role="button"
          tabindex="0"
          onclick={() => onselectthread(thread.id)}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") onselectthread(thread.id);
          }}
        >
          <div class="thread-item-leading">
            <div
              class="thread-checkbox {$selectedThreadIds.has(thread.id) ? 'checked' : ''}"
              onclick={(e) => {
                e.stopPropagation();
                const idx = $threads.indexOf(thread);
                if (e.shiftKey && $lastSelectedIndex !== null) {
                  const start = Math.min($lastSelectedIndex, idx);
                  const end = Math.max($lastSelectedIndex, idx);
                  const ids = $threads.slice(start, end + 1).map(t => t.id);
                  selectedThreadIds.update(set => {
                    const next = new Set(set);
                    ids.forEach(id => next.add(id));
                    return next;
                  });
                } else {
                  toggleThreadSelection(thread.id);
                }
                lastSelectedIndex.set(idx);
              }}
              onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.stopPropagation(); toggleThreadSelection(thread.id); }}}
              role="checkbox"
              aria-checked={$selectedThreadIds.has(thread.id)}
              tabindex="-1"
            >
              {#if $selectedThreadIds.has(thread.id)}
                <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                  <rect x="0.5" y="0.5" width="13" height="13" rx="3" fill="#0A84FF" stroke="#0A84FF"/>
                  <path d="M3.5 7L6 9.5L10.5 4.5" stroke="white" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
              {:else}
                <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                  <rect x="0.5" y="0.5" width="13" height="13" rx="3" stroke="var(--text-secondary)" stroke-opacity="0.5"/>
                </svg>
              {/if}
            </div>
            <button
              class="thread-star {thread.starred ? 'starred' : ''}"
              style={effectiveHasSuperstars && thread.star_type ? `color: ${getStarColor(thread.star_type)};` : ''}
              onclick={(e) => {
                e.stopPropagation();
                if (effectiveHasSuperstars) {
                  ontogglestar(thread.id, thread.star_type ?? null);
                } else {
                  ontogglestar(thread.id, thread.starred ? (thread.star_type || null) : null);
                }
              }}
            >
              {#if effectiveHasSuperstars}
                {@html getStarIcon(thread.star_type ?? null)}
              {:else}
                {@html getStarIcon(thread.starred ? "YELLOW_STAR" : null)}
              {/if}
            </button>
            {#if effectiveHasImportant}
            <button
              class="thread-important {thread.important ? 'active' : ''}"
              onclick={(e) => {
                e.stopPropagation();
                ontoggleimportant(thread.id, thread.important ?? false);
              }}
              title={thread.important ? "Remove importance" : "Mark as important"}
            >
              <span class="important-icon">{@html thread.important ? iconImportantArrowFilled : iconImportantArrow}</span>
            </button>
            {/if}
            <div class="thread-unread-dot"></div>
          </div>
          <div class="thread-content">
            <div class="thread-content-header">
              <span class="thread-sender">
                {#if isUnifiedView && unifiedIndicator !== 'none'}
                  {#if unifiedIndicator === 'avatar'}
                    {@const acc = allAccounts.find(a => a.id === thread.account_id)}
                    {#if acc?.avatar_url}
                      <img
                        src={acc.avatar_url}
                        alt=""
                        class="unified-avatar"
                        referrerpolicy="no-referrer"
                      />
                    {:else}
                      <span
                        class="unified-avatar-placeholder"
                        style="background: {getAccountColor(thread.account_id, allAccounts)}"
                      >{getAccountInitial(thread.account_id, allAccounts)}</span>
                    {/if}
                  {:else if unifiedIndicator === 'color'}
                    <span
                      class="unified-color-dot"
                      style="background: {getAccountColor(thread.account_id, allAccounts)}"
                      title={allAccounts.find(a => a.id === thread.account_id)?.email ?? ''}
                    ></span>
                  {/if}
                {/if}
                {thread.sender}
              </span>
              <span class="thread-meta">
                {#if thread.has_attachments}
                  <span class="thread-clip"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21.44 11.05l-9.19 9.19a6 6 0 01-8.49-8.49l9.19-9.19a4 4 0 015.66 5.66l-9.2 9.19a2 2 0 01-2.83-2.83l8.49-8.48"/></svg></span>
                {/if}
                <span class="thread-time">{formatTime(thread.internal_date)}</span>
              </span>
            </div>
            <div class="thread-subject">{thread.subject}</div>
            <div class="thread-snippet">
              {decodeEntities(thread.snippet)}
            </div>
          </div>
        </div>
      {/each}

      {#if isBackgroundFilling}
        <div class="loading-more">
          <div class="loading-spinner"></div>
        </div>
      {/if}
    {/if}
  </div>
</section>

<style>
  .titlebar-spacer {
    height: 28px;
    flex-shrink: 0;
    -webkit-app-region: drag;
    display: flex;
    align-items: center;
    justify-content: flex-end;
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
    border-radius: var(--radius-standard);
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
    font-size: var(--font-size-base);
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
    border-radius: var(--radius-standard);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.12);
    z-index: 50;
    max-height: 240px;
    overflow-y: auto;
  }
  .suggestion-section {
    padding: 6px 12px;
    font-size: var(--font-size-small);
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
    font-size: var(--font-size-toolbar);
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
    font-size: var(--font-size-small);
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
  .skeleton-line.w40 {
    width: 40%;
  }
  .skeleton-line.w60 {
    width: 60%;
  }
  .skeleton-line.w80 {
    width: 80%;
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
  .list-header {
    padding: 8px 16px 8px 16px;
    padding-right: 8px;
    border-bottom: 1px solid var(--border-color);
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 36px;
  }
  .list-header h3 {
    font-weight: 600;
    font-size: var(--font-size-base);
    line-height: 16px;
    letter-spacing: -0.08px;
    color: var(--text-primary);
    display: flex;
    align-items: center;
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
    font-size: var(--font-size-base);
    line-height: 16px;
    letter-spacing: -0.08px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  .loading-spinner {
    width: 20px;
    height: 20px;
    border: 2px solid var(--border-color);
    border-top-color: var(--accent-blue);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
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
  .pagination-controls {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .pagination-range {
    font-size: var(--font-size-small);
    line-height: 14px;
    color: var(--text-secondary);
    font-weight: 400;
    margin-right: 4px;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .pagination-btn {
    background: none;
    border: none;
    border-radius: var(--radius-standard);
    cursor: pointer;
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    transition: background 0.15s, color 0.15s;
  }
  .pagination-btn:hover:not(:disabled) {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }
  .thread-item {
    display: flex;
    padding: 12px 12px;
    border-bottom: 1px solid var(--border-color);
    cursor: pointer;
    background: var(--bg-view);
    position: relative;
    transition: background 0.15s ease;
  }
  .thread-item.unified::before {
    content: "";
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 3px;
    background: var(--account-color);
  }
  .thread-item:not(.unread) {
    background-color: var(--sidebar-hover);
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
    flex-direction: row;
    align-items: center;
    gap: 2px;
    margin-right: 4px;
    flex-shrink: 0;
  }
  .thread-star {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    color: var(--text-secondary);
    opacity: 0.4;
    transition: all 0.2s;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    width: 20px;
    height: 20px;
    overflow: hidden;
  }
  .thread-star:hover {
    opacity: 1;
    background: rgba(255, 255, 255, 0.05);
  }
  .thread-star.starred {
    opacity: 1;
  }
  .thread-important {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    color: var(--text-secondary);
    opacity: 0.4;
    transition: all 0.2s;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    width: 20px;
    height: 20px;
    overflow: hidden;
  }
  .thread-important:hover {
    opacity: 1;
    background: rgba(255, 255, 255, 0.05);
  }
  .thread-important.active {
    color: #f5a623;
    opacity: 1;
  }
  .important-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    margin-left: -2px;
    margin-top: 5px;
    pointer-events: none;
  }
  .thread-unread-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background-color: transparent;
    display: none;
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
    font-size: var(--font-size-base);
    line-height: 16px;
    letter-spacing: -0.08px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .thread-meta {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    margin-left: 8px;
  }
  .thread-clip {
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    flex-shrink: 0;
    opacity: 0.6;
  }
  .thread-time {
    font-size: var(--font-size-small);
    line-height: 14px;
    color: var(--text-secondary);
    white-space: nowrap;
    flex-shrink: 0;
    font-weight: 400;
  }
  .thread-subject {
    font-size: var(--font-size-base);
    line-height: 16px;
    letter-spacing: -0.08px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-primary);
    font-weight: 400;
  }
  .thread-snippet {
    font-size: var(--font-size-toolbar);
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
  .pane-list {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  /* Unified Inbox account indicators */
  .unified-avatar {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    object-fit: cover;
    vertical-align: middle;
    margin-right: 5px;
    flex-shrink: 0;
    border: 1px solid var(--border-color);
  }
  .unified-avatar-placeholder {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    font-size: var(--font-size-small);
    font-weight: 700;
    color: #fff;
    vertical-align: middle;
    margin-right: 5px;
    flex-shrink: 0;
    line-height: 1;
  }
  .unified-color-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    margin-right: 8px;
    display: inline-block;
    vertical-align: middle;
    flex-shrink: 0;
    border: 1.5px solid rgba(255, 255, 255, 0.4);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
  }
  :global([data-theme="dark"]) .unified-color-dot {
    border-color: rgba(255, 255, 255, 0.12);
  }
  .unified-badge {
    font-size: var(--font-size-small);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    background: rgba(10, 132, 255, 0.1);
    color: var(--accent-blue);
    padding: 2px 6px;
    border-radius: 4px;
    margin-left: 8px;
  }

  /* Multi-select checkboxes */
  .thread-checkbox {
    width: 0;
    overflow: hidden;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    opacity: 0;
    transition: width 0.12s ease, opacity 0.12s ease;
  }
  .thread-item:hover .thread-checkbox,
  .thread-checkbox.checked,
  .has-selection .thread-checkbox {
    width: 20px;
    opacity: 1;
  }
  .thread-item.multi-selected {
    background-color: rgba(10, 132, 255, 0.08);
  }
  :global([data-theme="dark"]) .thread-item.multi-selected {
    background-color: rgba(10, 132, 255, 0.12);
  }

  /* Bulk action toolbar */
  .bulk-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
  }
  .bulk-toolbar-left {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .bulk-select-all {
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 4px;
  }
  .bulk-select-all:hover { background: var(--sidebar-hover); }
  .bulk-count {
    font-size: var(--font-size-toolbar);
    font-weight: 600;
    color: var(--text-secondary);
    margin: 0 4px;
    white-space: nowrap;
  }
  .bulk-separator {
    width: 1px;
    height: 16px;
    background: var(--border-color);
    margin: 0 4px;
    flex-shrink: 0;
  }
  .bulk-action {
    background: none;
    border: none;
    cursor: pointer;
    padding: 4px;
    border-radius: 4px;
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    transition: color 0.1s, background 0.1s;
  }
  .bulk-action:hover {
    color: var(--accent-blue);
    background: var(--sidebar-hover);
  }
  .bulk-close { margin-left: auto; }
</style>
