import { writable } from 'svelte/store';

export interface LocalThread {
    id: string;
    snippet: string;
    history_id: string;
    unread: number;
    sender: string;
    subject: string;
    internal_date: number;
    starred: boolean;
    star_type?: string | null;
    has_attachments?: boolean;
    important?: boolean;
    account_id: string;
}

/** Mirrors the Rust `SearchResult` returned by `semantic_search_query`. */
export interface SemanticSearchResult {
    message_id: string;
    thread_id: string;
    subject: string;
    sender: string;
    snippet: string;
    score: number;
    internal_date: number;
}

/**
 * Convert a semantic `SearchResult` into a fully-typed `LocalThread` suitable
 * for the shared `threads` store. Carries the real `internal_date` so date
 * sorting works, and appends the match score to the snippet for display.
 */
export function semanticResultToThread(
    r: SemanticSearchResult,
    accountId: string,
): LocalThread {
    return {
        id: r.thread_id,
        snippet: `${r.snippet}  [match: ${Math.round(r.score * 100)}%]`,
        history_id: "",
        unread: 0,
        sender: r.sender,
        subject: r.subject,
        internal_date: r.internal_date,
        starred: false,
        star_type: null,
        has_attachments: false,
        important: false,
        account_id: accountId,
    };
}

export const threads = writable<LocalThread[]>([]);
export const isSyncing = writable(false);
export const lastSyncError = writable<string | null>(null);
