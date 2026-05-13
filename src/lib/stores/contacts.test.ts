import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import { contacts, selectedContactId, contactSearchQuery, isContactsSyncing, loadContacts, searchContacts } from './contacts';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

describe('contacts store', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        contacts.set([]);
        selectedContactId.set(null);
        contactSearchQuery.set('');
    });

    it('initializes with empty state', () => {
        expect(get(contacts)).toEqual([]);
        expect(get(selectedContactId)).toBeNull();
        expect(get(contactSearchQuery)).toBe('');
        expect(get(isContactsSyncing)).toBe(false);
    });

    it('loadContacts fetches and populates store', async () => {
        const mockContacts = [
            { id: '1', display_name: 'Alice', emails: [{ email: 'alice@test.com' }], groups: [] },
            { id: '2', display_name: 'Bob', emails: [{ email: 'bob@test.com' }], groups: [] },
        ];
        vi.mocked(invoke).mockResolvedValue(mockContacts);

        await loadContacts();

        expect(invoke).toHaveBeenCalledWith('get_contacts', { search: null, groupId: null, offset: 0, limit: 50, accountId: null });
        expect(get(contacts)).toEqual(mockContacts);
    });

    it('searchContacts passes query to backend', async () => {
        vi.mocked(invoke).mockResolvedValue([]);

        await searchContacts('alice');

        expect(invoke).toHaveBeenCalledWith('get_contacts', { search: 'alice', groupId: null, offset: 0, limit: 50, accountId: null });
    });
});
