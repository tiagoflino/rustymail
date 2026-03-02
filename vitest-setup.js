import '@testing-library/jest-dom';
import { vi } from 'vitest';

// Mock Tauri's invoke
vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

// Mock window.matchMedia
Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: vi.fn().mockImplementation(query => ({
        matches: false,
        media: query,
        onchange: null,
        addListener: vi.fn(), // deprecated
        removeListener: vi.fn(), // deprecated
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        dispatchEvent: vi.fn(),
    })),
});

// Mock getAnimations for Svelte transitions in jsdom
if (typeof window !== 'undefined') {
    Element.prototype.getAnimations = vi.fn().mockReturnValue([]);

    // Mock IntersectionObserver
    window.IntersectionObserver = vi.fn().mockImplementation((callback) => ({
        observe: vi.fn(),
        unobserve: vi.fn(),
        disconnect: vi.fn(),
        // Allow triggering the callback manually in tests
        __trigger: (entries) => callback(entries)
    }));

    // Mock localStorage
    const localStorageMock = (() => {
        let store = {};
        return {
            getItem: vi.fn((key) => store[key] || null),
            setItem: vi.fn((key, value) => { store[key] = value.toString(); }),
            clear: vi.fn(() => { store = {}; }),
            removeItem: vi.fn((key) => { delete store[key]; }),
        };
    })();
    Object.defineProperty(window, 'localStorage', { value: localStorageMock });
}
