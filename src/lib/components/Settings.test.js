import { render, screen, fireEvent } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import Settings from './Settings.svelte';
import { invoke } from '@tauri-apps/api/core';

// Mock symbols/icons if necessary, though they are likely just strings
vi.mock('$lib/components/icons', () => ({
    iconClose: 'close-icon',
    iconUser: 'user-icon',
    iconPlus: 'plus-icon',
    iconCheck: 'check-icon'
}));

describe('Settings.svelte', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        // Default mock implementation
        vi.mocked(invoke).mockResolvedValue([]);
    });

    it('renders when show is true', async () => {
        render(Settings, { show: true, accounts: [] });
        // Tab "Accounts" should be visible
        expect(screen.getByText('Accounts')).toBeInTheDocument();
    });

    it('calls loadSettings on mount if show is true', async () => {
        vi.mocked(invoke).mockResolvedValue([
            { key: 'theme', value: 'dark' }
        ]);

        render(Settings, { show: true, accounts: [] });

        // Check if invoke was called for get_settings
        expect(invoke).toHaveBeenCalledWith('get_settings');
    });

    it('closes when close button is clicked', async () => {
        const onclose = vi.fn();
        render(Settings, { show: true, accounts: [], onclose });

        const closeBtn = screen.getByLabelText('Close Settings');
        await fireEvent.click(closeBtn);

        expect(onclose).toHaveBeenCalled();
    });
});
