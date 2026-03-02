import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import Compose from './Compose.svelte';
import { invoke } from '@tauri-apps/api/core';

// Mock symbols/icons
vi.mock('./icons', () => ({
    iconClose: 'close-icon',
    iconTrash: 'trash-icon',
    iconSent: 'sent-icon',
    iconCheck: 'check-icon'
}));

// Mock svelte/transition
vi.mock('svelte/transition', () => ({
    fly: vi.fn(() => ({ duration: 0 }))
}));

// Mock execCommand
if (typeof document !== 'undefined') {
    document.execCommand = vi.fn();
}

describe('Compose.svelte', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(invoke).mockResolvedValue([]);
        vi.useFakeTimers();
    });

    it('renders correctly', () => {
        render(Compose, { onClose: vi.fn() });
        expect(screen.getByPlaceholderText('Recipients')).toBeInTheDocument();
        expect(screen.getByPlaceholderText('Subject')).toBeInTheDocument();
    });

    it('handles "to" field input and suggestions', async () => {
        vi.mocked(invoke).mockResolvedValue([
            { email: 'test@example.com', name: 'Test User' }
        ]);

        render(Compose, { onClose: vi.fn() });
        const toInput = screen.getByPlaceholderText('Recipients');

        await fireEvent.input(toInput, { target: { value: 'te' } });

        vi.advanceTimersByTime(200);

        await waitFor(() => {
            expect(invoke).toHaveBeenCalledWith('search_contacts', { query: 'te' });
        });

        await waitFor(() => {
            expect(screen.getByText('Test User')).toBeInTheDocument();
        });
    });

    it('selects a suggestion', async () => {
        vi.mocked(invoke).mockResolvedValue([
            { email: 'test@example.com', name: 'Test User' }
        ]);

        render(Compose, { onClose: vi.fn() });
        const toInput = screen.getByPlaceholderText('Recipients');

        await fireEvent.input(toInput, { target: { value: 'te' } });
        vi.advanceTimersByTime(200);

        const suggestion = await screen.findByText('Test User');
        await fireEvent.click(suggestion);

        expect(/** @type {HTMLInputElement} */(toInput).value).toBe('Test User <test@example.com>, ');
    });
});
