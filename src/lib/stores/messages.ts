import { writable } from 'svelte/store';

export interface LocalMessage {
    id: string;
    thread_id: string;
    sender: string;
    recipients: string;
    subject: string;
    snippet: string;
    internal_date: number;
    body_html: string;
    body_plain: string;
    is_draft?: boolean;
    has_attachments?: boolean;
}

export const selectedThreadId = writable<string | null>(null);
export const currentMessages = writable<LocalMessage[]>([]);
export const isMessagesLoading = writable(false);
export const messagesError = writable<string | null>(null);

// Multi-select state
export const selectedThreadIds = writable<Set<string>>(new Set());
export const lastSelectedIndex = writable<number | null>(null);

export function toggleThreadSelection(id: string) {
	selectedThreadIds.update(set => {
		const next = new Set(set);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		return next;
	});
}

export function clearSelection() {
	selectedThreadIds.set(new Set());
	lastSelectedIndex.set(null);
}

export function selectAll(ids: string[]) {
	selectedThreadIds.set(new Set(ids));
}
