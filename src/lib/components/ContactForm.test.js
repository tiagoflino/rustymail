import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import ContactForm from './ContactForm.svelte';

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

describe('ContactForm.svelte', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(invoke).mockResolvedValue({ id: '1', display_name: 'Test', emails: [], groups: [] });
    });

    it('renders create form with empty fields', () => {
        render(ContactForm, { onClose: vi.fn(), onSaved: vi.fn() });
        expect(screen.getByPlaceholderText('Display name')).toBeInTheDocument();
        expect(screen.getByPlaceholderText('Email')).toBeInTheDocument();
        expect(screen.getByText('Save')).toBeInTheDocument();
    });

    it('renders edit form with pre-filled fields', () => {
        const contact = {
            id: '1', display_name: 'Alice Smith', given_name: 'Alice', surname: 'Smith',
            account_id: 'acc1', nickname: null, department: null, notes: null,
            birthday: null, photo_uri: null, is_starred: false, source: 'local',
            created_at: 1000, updated_at: 1000, email_count_sent: 0,
            email_count_received: 0, first_seen_at: null, last_contacted_at: null,
            is_promoted: true,
            company: 'TechCo', job_title: 'Dev',
            emails: [{ id: 'e1', contact_id: '1', email: 'alice@tech.com', type: 'work', is_primary: true }],
            phones: '[]', addresses: '[]', social_profiles: '[]', urls: '[]', relations: '[]', groups: [],
        };
        render(ContactForm, { contact, onClose: vi.fn(), onSaved: vi.fn() });
        expect(screen.getByDisplayValue('Alice Smith')).toBeInTheDocument();
        expect(screen.getByDisplayValue('TechCo')).toBeInTheDocument();
        expect(screen.getByText('Edit Contact')).toBeInTheDocument();
    });

    it('shows cancel button', () => {
        render(ContactForm, { onClose: vi.fn(), onSaved: vi.fn() });
        expect(screen.getByText('Cancel')).toBeInTheDocument();
    });
});
