import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import ContactHoverCard from './ContactHoverCard.svelte';

describe('ContactHoverCard.svelte', () => {
    it('displays contact info when shown', () => {
        const contact = {
            display_name: 'Alice Anderson',
            company: 'TechCo',
            job_title: 'Engineer',
            emails: [{ email: 'alice@tech.com', type: 'work', is_primary: true }],
        };
        render(ContactHoverCard, { contact, x: 100, y: 200 });
        expect(screen.getByText('Alice Anderson')).toBeInTheDocument();
        expect(screen.getByText('Engineer at TechCo')).toBeInTheDocument();
        expect(screen.getByText('alice@tech.com')).toBeInTheDocument();
    });

    it('shows initials as avatar', () => {
        const contact = {
            display_name: 'Bob Builder',
            emails: [{ email: 'bob@build.io', type: 'work', is_primary: true }],
        };
        render(ContactHoverCard, { contact, x: 0, y: 0 });
        expect(screen.getByText('BB')).toBeInTheDocument();
    });

    it('handles missing optional fields', () => {
        const contact = {
            display_name: 'Simple Contact',
            emails: [{ email: 'simple@test.com', type: 'work', is_primary: true }],
        };
        render(ContactHoverCard, { contact, x: 50, y: 50 });
        expect(screen.getByText('Simple Contact')).toBeInTheDocument();
        expect(screen.getByText('simple@test.com')).toBeInTheDocument();
    });
});
