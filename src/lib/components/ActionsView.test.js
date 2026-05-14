import { render, screen, waitFor, fireEvent } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import ActionsView from './ActionsView.svelte';
import { invoke } from '@tauri-apps/api/core';

vi.mock('svelte/transition', () => ({
    fly: vi.fn(() => ({ duration: 0 }))
}));

describe('ActionsView.svelte', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    const mockPendingItems = [
        {
            id: 1,
            account_id: "acc1",
            thread_id: "t1",
            message_id: "m1",
            description: "Review the quarterly report",
            assignee: "Alice",
            deadline: "2026-06-01",
            confidence: 0.95,
            status: "pending",
            created_at: 1000,
            completed_at: null,
            thread_subject: "Q2 Review",
            thread_sender: "bob@example.com",
        },
        {
            id: 2,
            account_id: "acc1",
            thread_id: "t1",
            message_id: "m2",
            description: "Schedule follow-up meeting",
            assignee: null,
            deadline: "2026-05-20",
            confidence: 0.6,
            status: "pending",
            created_at: 2000,
            completed_at: null,
            thread_subject: "Q2 Review",
            thread_sender: "bob@example.com",
        },
        {
            id: 3,
            account_id: "acc1",
            thread_id: "t2",
            message_id: "m3",
            description: "Update project timeline",
            assignee: "Charlie",
            deadline: null,
            confidence: 0.85,
            status: "pending",
            created_at: 3000,
            completed_at: null,
            thread_subject: "Project Alpha",
            thread_sender: "charlie@example.com",
        },
    ];

    const mockCompletedItems = [
        {
            id: 4,
            account_id: "acc1",
            thread_id: "t3",
            message_id: "m4",
            description: "Send invoice",
            assignee: null,
            deadline: null,
            confidence: 0.9,
            status: "completed",
            created_at: 500,
            completed_at: 6000,
            thread_subject: "Invoice",
            thread_sender: "dave@example.com",
        },
    ];

    function mockInvokeHandler(opts = {}) {
        const { pending = mockPendingItems, completed = mockCompletedItems } = opts;
        vi.mocked(invoke).mockImplementation(async (cmd, args) => {
            if (cmd === "mark_action_complete" || cmd === "dismiss_action_item") {
                return null;
            }
            if (args && args.status === "completed") return completed;
            return pending;
        });
    }

    it('renders loading state initially', () => {
        vi.mocked(invoke).mockReturnValue(new Promise(() => {}));
        render(ActionsView, { accountId: "acc1" });
        expect(document.querySelector('.loading-spinner')).toBeInTheDocument();
        expect(screen.getByText('Loading action items...')).toBeInTheDocument();
    });

    it('renders empty state when no action items', async () => {
        vi.mocked(invoke).mockResolvedValue([]);
        render(ActionsView, { accountId: "acc1" });

        await waitFor(() => {
            expect(screen.getByText('No pending action items')).toBeInTheDocument();
        });
    });

    it('renders pending action items grouped by thread', async () => {
        mockInvokeHandler();
        render(ActionsView, { accountId: "acc1" });

        await waitFor(() => {
            expect(screen.getByText('Q2 Review')).toBeInTheDocument();
            expect(screen.getByText('Project Alpha')).toBeInTheDocument();
            expect(screen.getByText('Review the quarterly report')).toBeInTheDocument();
            expect(screen.getByText('Schedule follow-up meeting')).toBeInTheDocument();
            expect(screen.getByText('Update project timeline')).toBeInTheDocument();
        });

        expect(screen.getByText('Alice')).toBeInTheDocument();
        expect(screen.getByText('Charlie')).toBeInTheDocument();
        expect(screen.getByText('95%')).toBeInTheDocument();
        expect(screen.getByText('60%')).toBeInTheDocument();
        expect(screen.getByText('85%')).toBeInTheDocument();
    });

    it('marks an action item as complete', async () => {
        mockInvokeHandler();
        render(ActionsView, { accountId: "acc1" });

        await waitFor(() => {
            expect(screen.getByText('Review the quarterly report')).toBeInTheDocument();
        });

        const checkButtons = document.querySelectorAll('.check-btn');
        expect(checkButtons.length).toBeGreaterThan(0);

        fireEvent.click(checkButtons[0]);

        await waitFor(() => {
            expect(vi.mocked(invoke)).toHaveBeenLastCalledWith("mark_action_complete", { actionItemId: 3 });
        });
    });

    it('dismisses an action item', async () => {
        mockInvokeHandler();
        render(ActionsView, { accountId: "acc1" });

        await waitFor(() => {
            expect(screen.getByText('Review the quarterly report')).toBeInTheDocument();
        });

        const dismissButtons = document.querySelectorAll('.dismiss-btn');
        expect(dismissButtons.length).toBeGreaterThan(0);

        fireEvent.click(dismissButtons[1]);

        await waitFor(() => {
            expect(vi.mocked(invoke)).toHaveBeenLastCalledWith("dismiss_action_item", { actionItemId: 1 });
        });
    });

    it('switches to completed tab', async () => {
        mockInvokeHandler({ pending: mockPendingItems, completed: mockCompletedItems });
        render(ActionsView, { accountId: "acc1" });

        await waitFor(() => {
            expect(screen.getByText('Review the quarterly report')).toBeInTheDocument();
        });

        const completedTab = screen.getByText('Completed');
        fireEvent.click(completedTab);

        await waitFor(() => {
            expect(screen.getByText('Send invoice')).toBeInTheDocument();
        });
    });

    it('displays completed tab content', async () => {
        mockInvokeHandler({ pending: mockPendingItems, completed: mockCompletedItems });
        render(ActionsView, { accountId: "acc1" });

        await waitFor(() => {
            expect(screen.getByText('Review the quarterly report')).toBeInTheDocument();
        });

        const completedTab = screen.getByText('Completed');
        fireEvent.click(completedTab);

        await waitFor(() => {
            expect(screen.getByText('Invoice')).toBeInTheDocument();
            expect(screen.getByText('Send invoice')).toBeInTheDocument();
        });
    });

    it('shows error state and retry', async () => {
        vi.mocked(invoke).mockRejectedValue(new Error("DB error"));
        render(ActionsView, { accountId: "acc1" });

        await waitFor(() => {
            expect(screen.getByText((content) => content.includes('DB error'))).toBeInTheDocument();
            expect(screen.getByText('Retry')).toBeInTheDocument();
        });

        vi.mocked(invoke).mockReset();
        vi.mocked(invoke).mockResolvedValue([]);

        fireEvent.click(screen.getByText('Retry'));

        await waitFor(() => {
            expect(screen.getByText('No pending action items')).toBeInTheDocument();
        });
    });
});
