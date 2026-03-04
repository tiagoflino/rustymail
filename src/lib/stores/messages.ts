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
}

export const selectedThreadId = writable<string | null>(null);
export const currentMessages = writable<LocalMessage[]>([]);
export const isMessagesLoading = writable(false);
export const messagesError = writable<string | null>(null);
