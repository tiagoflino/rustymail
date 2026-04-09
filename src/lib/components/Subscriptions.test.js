import { render, screen, waitFor, fireEvent } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import Subscriptions from './Subscriptions.svelte';
import { invoke } from '@tauri-apps/api/core';

vi.mock('svelte/transition', () => ({
    fly: vi.fn(() => ({ duration: 0 }))
}));

describe('Subscriptions.svelte', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    const mockSubscriptions = [
        {
            id: 1,
            sender_name: "Newsletter Co",
            sender_email: "news@example.com",
            message_count: 42,
            avg_frequency_days: 7.0,
            last_seen: 1774828800000,
            status: "active",
            detection_method: "list-unsubscribe",
            unsubscribe_url: "https://example.com/unsub",
            unsubscribe_mailto: "unsubscribe@example.com",
            supports_one_click: false
        },
        {
            id: 2,
            sender_name: "Daily Digest",
            sender_email: "digest@daily.com",
            message_count: 100,
            avg_frequency_days: 1.0,
            last_seen: 1775347200000,
            status: "unsubscribed",
            detection_method: "manual",
            unsubscribe_url: null,
            unsubscribe_mailto: null,
            supports_one_click: false
        },
        {
            id: 3,
            sender_name: "Promo Alerts",
            sender_email: "promo@shop.com",
            message_count: 15,
            avg_frequency_days: null,
            last_seen: 1773638400000,
            status: "ignored",
            detection_method: "list-unsubscribe",
            unsubscribe_url: null,
            unsubscribe_mailto: null,
            supports_one_click: false
        }
    ];

    it('renders loading state initially', () => {
        vi.mocked(invoke).mockReturnValue(new Promise(() => {}));
        render(Subscriptions, { accountId: "test-account" });
        expect(document.querySelector('.loading-spinner')).toBeInTheDocument();
        expect(screen.getByText('Loading subscriptions...')).toBeInTheDocument();
    });

    it('renders subscriptions table when loaded', async () => {
        vi.mocked(invoke).mockResolvedValue(mockSubscriptions);
        render(Subscriptions, { accountId: "test-account" });

        await waitFor(() => {
            expect(screen.getByText('Newsletter Co')).toBeInTheDocument();
            expect(screen.getByText('news@example.com')).toBeInTheDocument();
            expect(screen.getByText('42')).toBeInTheDocument();
        });
    });

    it('renders empty state when no subscriptions', async () => {
        vi.mocked(invoke).mockResolvedValue([]);
        render(Subscriptions, { accountId: "test-account" });

        await waitFor(() => {
            expect(screen.getByText('No subscriptions found')).toBeInTheDocument();
        });
    });

    it('handles error state and shows retry button', async () => {
        vi.mocked(invoke).mockRejectedValue(new Error('API Error'));
        render(Subscriptions, { accountId: "test-account" });

        await waitFor(() => {
            expect(screen.getByText(/API Error/)).toBeInTheDocument();
            expect(screen.getByText('Retry')).toBeInTheDocument();
        });

        vi.mocked(invoke).mockResolvedValueOnce(mockSubscriptions);
        await fireEvent.click(screen.getByText('Retry'));

        await waitFor(() => {
            expect(screen.getByText('Newsletter Co')).toBeInTheDocument();
        });
    });

    it('scan button calls scan_subscriptions command and reloads list', async () => {
        vi.mocked(invoke).mockResolvedValueOnce([]).mockResolvedValueOnce({ messages_scanned: 100, subscriptions_found: 5, enriched: 3 });
        
        render(Subscriptions, { accountId: "test-account" });

        await waitFor(() => {
            expect(screen.getByText('Scan')).toBeInTheDocument();
        });

        const scanBtn = screen.getByText('Scan');
        await fireEvent.click(scanBtn);

        await waitFor(() => {
            expect(vi.mocked(invoke)).toHaveBeenCalledWith('scan_subscriptions', { accountId: "test-account" });
        });

        await waitFor(() => {
            expect(vi.mocked(invoke)).toHaveBeenCalledWith('get_subscriptions', { accountId: "test-account" });
        });
    });

    it('filter tabs filter by status', async () => {
        vi.mocked(invoke).mockResolvedValue(mockSubscriptions);
        render(Subscriptions, { accountId: "test-account" });

        await waitFor(() => {
            expect(screen.getByText('Active')).toBeInTheDocument();
        });

        const activeTab = screen.getByText('Active');
        await fireEvent.click(activeTab);

        const filtered = mockSubscriptions.filter(s => s.status === 'active');
        await waitFor(() => {
            expect(screen.getByText('Newsletter Co')).toBeInTheDocument();
        });
        expect(screen.queryByText('Daily Digest')).not.toBeInTheDocument();
    });

    it('search input filters by sender name/email', async () => {
        vi.mocked(invoke).mockResolvedValue(mockSubscriptions);
        render(Subscriptions, { accountId: "test-account" });

        await waitFor(() => {
            expect(screen.getByPlaceholderText('Filter by sender...')).toBeInTheDocument();
        });

        const searchInput = screen.getByPlaceholderText('Filter by sender...');
        await fireEvent.input(searchInput, { target: { value: 'digest' } });

        await waitFor(() => {
            expect(screen.getByText('Daily Digest')).toBeInTheDocument();
        });
        expect(screen.queryByText('Newsletter Co')).not.toBeInTheDocument();
    });

    it('unsubscribe button opens dialog, confirms via link, and marks done', async () => {
        vi.mocked(invoke)
            .mockResolvedValueOnce(mockSubscriptions)
            .mockResolvedValueOnce({ method: "https", success: true, message: "Opened", opened_browser: true })
            .mockResolvedValueOnce(undefined)
            .mockResolvedValueOnce(mockSubscriptions);

        render(Subscriptions, { accountId: "test-account" });

        await waitFor(() => {
            expect(screen.getByTitle('Unsubscribe')).toBeInTheDocument();
        });

        await fireEvent.click(screen.getByTitle('Unsubscribe'));

        await waitFor(() => {
            expect(screen.getByText('Open Link')).toBeInTheDocument();
        });

        await fireEvent.click(screen.getByText('Open Link'));

        await waitFor(() => {
            expect(vi.mocked(invoke)).toHaveBeenCalledWith('unsubscribe', { subscriptionId: 1 });
        });

        await waitFor(() => {
            expect(screen.getByText('Did you unsubscribe?')).toBeInTheDocument();
        });

        await fireEvent.click(screen.getByText('Yes'));

        await waitFor(() => {
            expect(vi.mocked(invoke)).toHaveBeenCalledWith('mark_unsubscribed', { subscriptionId: 1 });
        });
    });

    it('delete button calls delete_subscription command', async () => {
        vi.mocked(invoke)
            .mockResolvedValueOnce(mockSubscriptions)  // initial load
            .mockResolvedValueOnce(undefined)          // delete result
            .mockResolvedValueOnce(mockSubscriptions); // reload after delete
        
        render(Subscriptions, { accountId: "test-account" });

        await waitFor(() => {
            expect(screen.getAllByTitle('Delete')[0]).toBeInTheDocument();
        });

        await fireEvent.click(screen.getAllByTitle('Delete')[0]);

        await waitFor(() => {
            expect(vi.mocked(invoke)).toHaveBeenCalledWith('delete_subscription', { subscriptionId: 2 });
        });
    });

    it('Not a subscription button calls correct_subscription command', async () => {
        vi.mocked(invoke)
            .mockResolvedValueOnce(mockSubscriptions)  // initial load
            .mockResolvedValueOnce(undefined)          // correct result
            .mockResolvedValueOnce(mockSubscriptions); // reload after correct
        
        render(Subscriptions, { accountId: "test-account" });

        await waitFor(() => {
            expect(screen.getAllByTitle('Not a subscription')[0]).toBeInTheDocument();
        });

        await fireEvent.click(screen.getAllByTitle('Not a subscription')[0]);

        await waitFor(() => {
            expect(vi.mocked(invoke)).toHaveBeenCalledWith('correct_subscription', { subscriptionId: 2, isSubscription: false });
        });
    });
});