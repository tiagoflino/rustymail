import { render, screen, waitFor, fireEvent } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import FeedView from './FeedView.svelte';
import { invoke } from '@tauri-apps/api/core';

vi.mock('svelte/transition', () => ({
    fly: vi.fn(() => ({ duration: 0 }))
}));

describe('FeedView.svelte', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    const mockFeedThreads = [
        {
            id: 't1',
            snippet: 'This week in tech news',
            sender: 'newsletter@techweekly.com',
            subject: 'Tech Weekly Digest',
            internal_date: Date.now(),
            unread: 2,
            starred: false,
            star_type: null,
            has_attachments: false,
            important: false,
            account_id: 'acc1',
            history_id: 'h1',
        },
        {
            id: 't2',
            snippet: 'Your daily deals await',
            sender: 'newsletter@techweekly.com',
            subject: 'Special Offer Inside',
            internal_date: Date.now() - 86400000,
            unread: 1,
            starred: false,
            star_type: null,
            has_attachments: false,
            important: false,
            account_id: 'acc1',
            history_id: 'h2',
        },
        {
            id: 't3',
            snippet: 'Top stories for today',
            sender: 'updates@dailynews.com',
            subject: 'Daily News Roundup',
            internal_date: Date.now() - 3600000,
            unread: 0,
            starred: false,
            star_type: null,
            has_attachments: false,
            important: false,
            account_id: 'acc1',
            history_id: 'h3',
        },
    ];

    it('renders loading state initially', () => {
        vi.mocked(invoke).mockReturnValue(new Promise(() => {}));
        render(FeedView);
        expect(document.querySelector('.loading-spinner')).toBeInTheDocument();
    });

    it('renders empty state when no feed threads', async () => {
        vi.mocked(invoke).mockResolvedValue([]);
        render(FeedView);

        await waitFor(() => {
            expect(screen.getByText('No newsletters yet')).toBeInTheDocument();
        });
        expect(screen.getByText('When you receive newsletters they will appear here')).toBeInTheDocument();
    });

    it('renders grouped feed threads when loaded', async () => {
        vi.mocked(invoke).mockResolvedValue(mockFeedThreads);
        render(FeedView);

        await waitFor(() => {
            expect(screen.getByText('newsletter@techweekly.com')).toBeInTheDocument();
            expect(screen.getByText('updates@dailynews.com')).toBeInTheDocument();
        });

        expect(screen.getByText('Tech Weekly Digest')).toBeInTheDocument();
        expect(screen.getByText('Special Offer Inside')).toBeInTheDocument();
        expect(screen.getByText('Daily News Roundup')).toBeInTheDocument();
    });

    it('handles error state and shows retry button', async () => {
        vi.mocked(invoke).mockRejectedValue(new Error('Failed to load'));
        render(FeedView);

        await waitFor(() => {
            expect(screen.getByText(/Failed to load/)).toBeInTheDocument();
            expect(screen.getByText('Retry')).toBeInTheDocument();
        });

        vi.mocked(invoke).mockResolvedValueOnce(mockFeedThreads);
        await fireEvent.click(screen.getByText('Retry'));

        await waitFor(() => {
            expect(screen.getByText('Tech Weekly Digest')).toBeInTheDocument();
        });
    });

    it('mark all read calls batch_mark_read_status for sender threads', async () => {
        vi.mocked(invoke).mockResolvedValue(mockFeedThreads);
        render(FeedView);

        await waitFor(() => {
            expect(screen.getByText('newsletter@techweekly.com')).toBeInTheDocument();
        });

        const markReadButtons = screen.getAllByTitle('Mark all as read');
        await fireEvent.click(markReadButtons[0]);

        await waitFor(() => {
            expect(vi.mocked(invoke)).toHaveBeenCalledWith('batch_mark_read_status', { threadIds: ['t1', 't2'], isRead: true });
        });
    });

    it('unsubscribe button looks up subscription and calls unsubscribe', async () => {
        const mockSubs = [
            { id: 42, sender_email: 'newsletter@techweekly.com' },
            { id: 99, sender_email: 'updates@dailynews.com' },
        ];

        vi.mocked(invoke)
            .mockResolvedValueOnce(mockFeedThreads)
            .mockResolvedValueOnce(mockSubs)
            .mockResolvedValueOnce(undefined);

        render(FeedView);

        await waitFor(() => {
            expect(screen.getByText('newsletter@techweekly.com')).toBeInTheDocument();
        });

        const unsubscribeButtons = screen.getAllByTitle('Unsubscribe');
        await fireEvent.click(unsubscribeButtons[0]);

        await waitFor(() => {
            expect(vi.mocked(invoke)).toHaveBeenCalledWith('get_subscriptions', { accountId: null, status: 'active' });
            expect(vi.mocked(invoke)).toHaveBeenCalledWith('unsubscribe', { subscriptionId: 42 });
        });
    });
});
