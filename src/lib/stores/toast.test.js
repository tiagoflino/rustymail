import { describe, it, expect, vi, beforeEach } from 'vitest';
import { toasts, addToast, removeToast } from './toast';
import { get } from 'svelte/store';

describe('toast store', () => {
    beforeEach(() => {
        toasts.set([]);
        vi.useFakeTimers();
    });

    it('adds a toast message', () => {
        addToast('Test message', 'success', 5000);
        const current = get(toasts);
        expect(current).toHaveLength(1);
        expect(current[0].message).toBe('Test message');
        expect(current[0].type).toBe('success');
    });

    it('removes a toast message after duration', () => {
        addToast('Temporary', 'info', 1000);
        expect(get(toasts)).toHaveLength(1);

        vi.advanceTimersByTime(1000);
        expect(get(toasts)).toHaveLength(0);
    });

    it('removes toast manually', () => {
        addToast('Manual', 'info', 0);
        const id = get(toasts)[0].id;
        removeToast(id);
        expect(get(toasts)).toHaveLength(0);
    });
});
