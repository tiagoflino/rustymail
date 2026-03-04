import { render, screen, fireEvent } from '@testing-library/svelte';
import '@testing-library/jest-dom';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import Toasts from './Toasts.svelte';
import { toasts, addToast, removeToast } from '$lib/stores/toast';
import { get } from 'svelte/store';

// Mock svelte/transition and animate
vi.mock('svelte/transition', () => ({
    fly: vi.fn(() => ({ duration: 0 })),
    fade: vi.fn(() => ({ duration: 0 }))
}));
vi.mock('svelte/animate', () => ({
    flip: vi.fn(() => ({ duration: 0 }))
}));

describe('Toasts.svelte', () => {
    beforeEach(() => {
        toasts.set([]);
    });

    it('renders toasts from the store', async () => {
        addToast('Hello World', 'success');
        render(Toasts);

        expect(screen.getByText('Hello World')).toBeInTheDocument();
        expect(document.querySelector('.toast-success')).toBeInTheDocument();
    });

    it('removes toast when close button clicked', async () => {
        addToast('Click me', 'info');
        render(Toasts);

        const closeBtn = document.querySelector('.toast-close');
        if (closeBtn) await fireEvent.click(closeBtn);

        expect(get(toasts)).toHaveLength(0);
    });
});
