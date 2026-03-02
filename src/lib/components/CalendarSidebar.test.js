import { render, screen, waitFor, fireEvent } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import CalendarSidebar from './CalendarSidebar.svelte';
import { invoke } from '@tauri-apps/api/core';

// Mock svelte/transition
vi.mock('svelte/transition', () => ({
    fly: vi.fn(() => ({ duration: 0 }))
}));

describe('CalendarSidebar.svelte', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders loading state initially', () => {
        vi.mocked(invoke).mockReturnValue(new Promise(() => { })); // Never resolves
        render(CalendarSidebar, { onClose: vi.fn() });
        expect(document.querySelector('.spinner')).toBeInTheDocument();
    });

    it('renders events when loaded', async () => {
        const mockEvents = [
            {
                id: '1',
                summary: 'Test Event',
                start: { dateTime: '2026-03-02T10:00:00Z' },
                end: { dateTime: '2026-03-02T11:00:00Z' }
            }
        ];
        vi.mocked(invoke).mockResolvedValue(mockEvents);

        render(CalendarSidebar, { onClose: vi.fn() });

        await waitFor(() => {
            expect(screen.getByText('Test Event')).toBeInTheDocument();
        });
    });

    it('renders empty state when no events', async () => {
        vi.mocked(invoke).mockResolvedValue([]);
        render(CalendarSidebar, { onClose: vi.fn() });

        await waitFor(() => {
            expect(screen.getByText('No upcoming events.')).toBeInTheDocument();
        });
    });

    it('handles error state', async () => {
        vi.mocked(invoke).mockRejectedValue(new Error('API Error'));
        render(CalendarSidebar, { onClose: vi.fn() });

        await waitFor(() => {
            expect(screen.getByText('Could not load events.')).toBeInTheDocument();
            expect(screen.getByText('Error: API Error')).toBeInTheDocument();
        });
    });

    it('calls onClose when close button is clicked', async () => {
        vi.mocked(invoke).mockResolvedValue([]);
        const onClose = vi.fn();
        render(CalendarSidebar, { onClose });

        const closeBtn = await screen.findByRole('button', { name: '' }); // The button has no text/label in the snippet
        // Wait for loading to finish so header is stable
        await waitFor(() => expect(document.querySelector('.close-btn')).toBeInTheDocument());

        const btn = document.querySelector('.close-btn');
        if (btn) await fireEvent.click(btn);

        expect(onClose).toHaveBeenCalled();
    });
});
