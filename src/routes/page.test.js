import { render, screen, waitFor, fireEvent } from '@testing-library/svelte';
import '@testing-library/jest-dom';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import Page from './+page.svelte';
import { invoke } from '@tauri-apps/api/core';
import { isAuthenticated } from '$lib/stores/auth';
import { get } from 'svelte/store';

// Mock child components to simplify testing
vi.mock('$lib/components/Settings.svelte', () => ({
    default: vi.fn(() => ({ type: 'Settings' }))
}));
vi.mock('$lib/components/Compose.svelte', () => ({
    default: vi.fn(() => ({ type: 'Compose' }))
}));
vi.mock('$lib/components/CalendarSidebar.svelte', () => ({
    default: vi.fn(() => ({ type: 'CalendarSidebar' }))
}));
vi.mock('$lib/components/Toasts.svelte', () => ({
    default: vi.fn(() => ({ type: 'Toasts' }))
}));
vi.mock('$lib/components/LinkSafetyDialog.svelte', () => ({
    default: vi.fn(() => ({ type: 'LinkSafetyDialog' }))
}));

describe('+page.svelte', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
        isAuthenticated.set(false);
        vi.mocked(invoke).mockImplementation(async (cmd) => {
            if (cmd === 'check_auth_status') {
                return { authenticated: false, active_account: null, accounts: [] };
            }
            return [];
        });
    });

    it('renders onboarding when not authenticated', async () => {
        render(Page);

        // Should show "Rustymail" title from onboarding
        await waitFor(() => {
            expect(screen.getByText('Rustymail')).toBeInTheDocument();
        });
        expect(screen.getByText('Sign in with Google')).toBeInTheDocument();
    });

    it('renders app container when authenticated', async () => {
        isAuthenticated.set(true);
        const account = { id: '1', email: 'test@example.com', display_name: 'Test', avatar_url: '', is_active: true };
        vi.mocked(invoke).mockImplementation(async (cmd) => {
            if (cmd === 'check_auth_status') {
                return { authenticated: true, active_account: account, accounts: [account] };
            }
            if (cmd === 'get_accounts') return [account];
            if (cmd === 'get_labels') return [];
            if (cmd === 'get_threads') return [];
            if (cmd === 'get_settings') return [];
            if (cmd === 'get_setting') return '';
            if (cmd === 'sync_gmail_data') return null;
            if (cmd === 'get_hydration_progress') return { total: 0, hydrated: 0 };
            if (cmd === 'get_search_suggestions') return [];
            return null;
        });

        render(Page);

        await waitFor(() => {
            expect(screen.getByPlaceholderText(/Search mail/)).toBeInTheDocument();
        }, { timeout: 3000 });
    });

    it('loads and displays threads when authenticated', async () => {
        isAuthenticated.set(true);
        const mockThreads = [
            { id: 't1', snippet: 'Hello from test', sender: 'Sender 1', subject: 'Subject 1', internal_date: Date.now(), unread: 1, starred: false },
            { id: 't2', snippet: 'Another one', sender: 'Sender 2', subject: 'Subject 2', internal_date: Date.now() - 1000, unread: 0, starred: true }
        ];

        vi.mocked(invoke).mockImplementation(async (cmd) => {
            if (cmd === 'check_auth_status') {
                return {
                    authenticated: true,
                    active_account: { id: '1', email: 'test@example.com', display_name: 'Test', avatar_url: '', is_active: true },
                    accounts: []
                };
            }
            if (cmd === 'get_labels') return [];
            if (cmd === 'get_threads') return mockThreads;
            if (cmd === 'get_setting') return '30';
            return [];
        });

        render(Page);

        await waitFor(() => {
            expect(screen.getByText('Hello from test')).toBeInTheDocument();
            expect(screen.getByText('Another one')).toBeInTheDocument();
            expect(screen.getByText('Sender 1')).toBeInTheDocument();
            expect(screen.getByText('Subject 1')).toBeInTheDocument();
        });
    });

    it('displays messages when a thread is selected', async () => {
        isAuthenticated.set(true);
        const mockThreads = [
            { id: 't1', snippet: 'Thread 1', sender: 'Sender 1', subject: 'Subject 1', internal_date: Date.now(), unread: 1, starred: false }
        ];
        const mockMessages = [
            { id: 'm1', thread_id: 't1', sender: 'Sender 1', recipients: 'me@test.com', subject: 'Subject 1', snippet: 'Hello World', body_plain: 'Hello World', body_html: '', internal_date: Date.now(), is_draft: false }
        ];

        const account = { id: '1', email: 'test@example.com', display_name: 'Test', avatar_url: '', is_active: true };
        vi.mocked(invoke).mockImplementation(async (cmd, args) => {
            if (cmd === 'check_auth_status') {
                return { authenticated: true, active_account: account, accounts: [account] };
            }
            if (cmd === 'get_accounts') return [account];
            if (cmd === 'get_labels') return [];
            if (cmd === 'get_threads') return mockThreads;
            if (cmd === 'get_settings') return [];
            if (cmd === 'get_setting') return '30';
            if (cmd === 'sync_gmail_data') return null;
            if (cmd === 'sync_thread_messages') return null;
            if (cmd === 'get_hydration_progress') return { total: 0, hydrated: 0 };
            if (cmd === 'get_messages' && args && typeof args === 'object' && 'threadId' in args && args.threadId === 't1') return mockMessages;
            if (cmd === 'mark_thread_read_status') return null;
            return null;
        });

        render(Page);

        // Wait for thread to appear
        const threadItem = await screen.findByText('Thread 1');

        // Click the thread
        await fireEvent.click(threadItem);

        // Verify message content appears
        await waitFor(() => {
            expect(screen.getByText('Hello World')).toBeInTheDocument();
        });
    });
});
