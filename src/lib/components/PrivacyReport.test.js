import { render, screen, waitFor } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import PrivacyReport from './PrivacyReport.svelte';
import { invoke } from '@tauri-apps/api/core';

vi.mock('svelte/transition', () => ({
    fade: vi.fn(() => ({ duration: 0 })),
}));

describe('PrivacyReport.svelte', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    const fullReport = {
        total_blocked: 1234,
        unique_senders_tracked: 12,
        blocked_this_week: 56,
        top_trackers: [
            { sender_email: 'spy@ads.com', tracker_count: 9, tracker_types: 'tracking_pixel,remote_image' },
        ],
        trend: [
            { day: '2026-06-21', count: 3 },
            { day: '2026-06-22', count: 7 },
        ],
    };

    it('renders loading state initially', () => {
        vi.mocked(invoke).mockReturnValue(new Promise(() => {}));
        render(PrivacyReport, { onclose: () => {} });
        expect(screen.getByText('Loading report…')).toBeInTheDocument();
    });

    it('renders stats and trend chart when data is present', async () => {
        vi.mocked(invoke).mockResolvedValue(fullReport);
        render(PrivacyReport, { onclose: () => {} });

        await waitFor(() => {
            expect(screen.getByText('1.2k')).toBeInTheDocument();
        });
        expect(screen.getByText('Trackers Blocked')).toBeInTheDocument();
        expect(screen.getByText('spy@ads.com')).toBeInTheDocument();
        expect(screen.getByText('9 blocked')).toBeInTheDocument();
        // Accessible trend chart with non-color text alternatives
        expect(
            screen.getByLabelText('Trackers blocked per day, last 30 days'),
        ).toBeInTheDocument();
        expect(screen.getByText('3 blocked on 2026-06-21')).toBeInTheDocument();
    });

    it('shows benign empty state for a genuine no-data report', async () => {
        vi.mocked(invoke).mockResolvedValue({
            total_blocked: 0,
            unique_senders_tracked: 0,
            blocked_this_week: 0,
            top_trackers: [],
            trend: [],
        });
        render(PrivacyReport, { onclose: () => {} });

        await waitFor(() => {
            expect(screen.getByText(/No tracking data yet/)).toBeInTheDocument();
        });
        expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    });

    it('shows benign empty state when there is no active account', async () => {
        vi.mocked(invoke).mockRejectedValue('No active account found: no rows');
        render(PrivacyReport, { onclose: () => {} });

        await waitFor(() => {
            expect(screen.getByText(/No tracking data yet/)).toBeInTheDocument();
        });
        expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    });

    it('surfaces a real error (e.g. missing table / DB failure) distinctly', async () => {
        vi.mocked(invoke).mockRejectedValue('no such table: tracking_events');
        render(PrivacyReport, { onclose: () => {} });

        await waitFor(() => {
            expect(screen.getByRole('alert')).toBeInTheDocument();
        });
        expect(screen.getByText("Couldn't load your privacy report")).toBeInTheDocument();
        expect(screen.getByText(/no such table/)).toBeInTheDocument();
        expect(screen.queryByText(/No tracking data yet/)).not.toBeInTheDocument();
    });
});
