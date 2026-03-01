import { writable } from 'svelte/store';

export type ToastType = 'success' | 'error' | 'info';

export interface ToastMessage {
    id: string;
    type: ToastType;
    message: string;
    duration?: number;
}

export const toasts = writable<ToastMessage[]>([]);

export function addToast(message: string, type: ToastType = 'info', duration: number = 4000) {
    const id = Math.random().toString(36).substring(2, 9);
    toasts.update(all => [...all, { id, type, message, duration }]);

    if (duration > 0) {
        setTimeout(() => {
            removeToast(id);
        }, duration);
    }
}

export function removeToast(id: string) {
    toasts.update(all => all.filter(t => t.id !== id));
}
