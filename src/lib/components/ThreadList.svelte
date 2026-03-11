<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { writable, get, type Writable } from "svelte/store";
  import {
    iconSearch,
    iconClose,
    iconHistory,
    iconUser,
    iconTag,
    iconStar,
    iconStarFilled,
  } from "$lib/components/icons";
  import { threads, isSyncing } from "$lib/stores/threads";
  import { selectedThreadId } from "$lib/stores/messages";
  import { formatTime, decodeEntities } from "$lib/utils/formatters.js";

  interface SearchSuggestion {
    kind: string;
    text: string;
    detail: string;
  }

  interface Props {
    isLoadingThreads: boolean;
    isLabelFetching: boolean;
    isMacOS: boolean;
    hasMore: boolean;
    isLoadingMore: boolean;
    activeLabelName: string;
    searchQuery: Writable<string>;
    isSearching: Writable<boolean>;
    onselectthread: (threadId: string) => void;
    ontogglestar: (threadId: string, starred: boolean) => void;
    onloadmore: () => void;
    onsearch: (query: string) => void;
    onclearsearch: () => void;
  }

  let {
    isLoadingThreads,
    isLabelFetching,
    isMacOS,
    hasMore,
    isLoadingMore,
    activeLabelName,
    searchQuery,
    isSearching,
    onselectthread,
    ontogglestar,
    onloadmore,
    onsearch,
    onclearsearch,
  }: Props = $props();

  let searchInput = $state("");
  let searchInputEl = $state<HTMLInputElement>();
  let showSearchSuggestions = $state(false);
  let searchSuggestions = $state<SearchSuggestion[]>([]);
  let searchTimeout: ReturnType<typeof setTimeout> | null = null;
  let threadScrollArea = $state<HTMLDivElement>();
  let loadingSentinel = $state<HTMLDivElement>();
  let scrollObserver: IntersectionObserver | null = null;

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

  export function getLoadingSentinel() {
    return loadingSentinel;
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
          onloadmore();
        }
      },
      { root: threadScrollArea, rootMargin: "300px", threshold: 0.01 },
    );
    observeSentinel();
  }

  export function observeSentinel() {
    setTimeout(() => {
      if (loadingSentinel && scrollObserver) {
        scrollObserver.observe(loadingSentinel);
      }
    }, 50);
  }

  export function initObserver() {
    setupIntersectionObserver();
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
          {@const allFilters = [["from:", "From sender"], ["to:", "To recipient"], ["subject:", "Subject contains"], ["has:", "Has\u2026"], ["is:", "Is\u2026"], ["before:", "Before date"], ["after:", "After date"]]}
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

  <div class="list-header">
    <h3>{$searchQuery ? "Search Results" : activeLabelName}</h3>
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
          onclick={() => onselectthread(thread.id)}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") onselectthread(thread.id);
          }}
        >
          <div class="thread-item-leading">
            <button
              class="thread-star {thread.starred ? 'starred' : ''}"
              onclick={(e) => {
                e.stopPropagation();
                ontogglestar(thread.id, thread.starred);
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
  .pane-list {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
</style>
