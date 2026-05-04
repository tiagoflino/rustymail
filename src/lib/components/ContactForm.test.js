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
