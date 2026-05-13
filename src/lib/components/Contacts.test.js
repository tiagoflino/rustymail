import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import Contacts from './Contacts.svelte';

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

describe('Contacts.svelte', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(invoke).mockResolvedValue([]);
    });

    it('renders contacts view with search', async () => {
        render(Contacts);
        await waitFor(() => {
            expect(screen.getByPlaceholderText('Search contacts...')).toBeInTheDocument();
        });
    });

    it('displays contact list', async () => {
        vi.mocked(invoke).mockImplementation((cmd) => {
            if (cmd === 'get_contacts') return Promise.resolve([
                { id: '1', display_name: 'Alice Anderson', company: 'TechCo', emails: [{ email: 'alice@tech.com', is_primary: true }], groups: [], phones: '[]', addresses: '[]', social_profiles: '[]', urls: '[]', relations: '[]' },
                { id: '2', display_name: 'Bob Builder', company: null, emails: [{ email: 'bob@build.io', is_primary: true }], groups: [], phones: '[]', addresses: '[]', social_profiles: '[]', urls: '[]', relations: '[]' },
            ]);
            if (cmd === 'get_contact_groups') return Promise.resolve([]);
            return Promise.resolve([]);
        });

        render(Contacts);
        await waitFor(() => {
            expect(screen.getByText('Alice Anderson')).toBeInTheDocument();
            expect(screen.getByText('Bob Builder')).toBeInTheDocument();
        });
    });

    it('shows new contact button', async () => {
        render(Contacts);
        await waitFor(() => {
            expect(screen.getByTitle('New contact')).toBeInTheDocument();
        });
    });
});
