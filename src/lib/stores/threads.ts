import { writable } from 'svelte/store';

export interface LocalThread {
    id: string;
    snippet: string;
    history_id: string;
    unread: number;
}

export const threads = writable<LocalThread[]>([]);
export const isSyncing = writable(false);
export const lastSyncError = writable<string | null>(null);
