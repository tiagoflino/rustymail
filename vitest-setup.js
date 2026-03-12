import '@testing-library/jest-dom';
import { vi } from 'vitest';

// Mock Tauri's invoke
vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

// Mock Tauri's window API
vi.mock('@tauri-apps/api/window', () => ({
    getCurrentWindow: vi.fn(() => ({
        setDecorations: vi.fn(() => Promise.resolve()),
        minimize: vi.fn(() => Promise.resolve()),
        maximize: vi.fn(() => Promise.resolve()),
        toggleMaximize: vi.fn(() => Promise.resolve()),
        close: vi.fn(() => Promise.resolve()),
    })),
}));

// Mock Tauri updater + process + app plugins
vi.mock('@tauri-apps/plugin-updater', () => ({
    check: vi.fn(() => Promise.resolve(null)),
}));
vi.mock('@tauri-apps/plugin-process', () => ({
    relaunch: vi.fn(() => Promise.resolve()),
}));
vi.mock('@tauri-apps/api/app', () => ({
    getVersion: vi.fn(() => Promise.resolve('0.1.0')),
}));

// Mock Tauri event API
vi.mock('@tauri-apps/api/event', () => ({
    listen: vi.fn(() => Promise.resolve(() => {})),
    emit: vi.fn(() => Promise.resolve()),
}));

// Mock Tauri dialog plugin
vi.mock('@tauri-apps/plugin-dialog', () => ({
    open: vi.fn(() => Promise.resolve(null)),
    save: vi.fn(() => Promise.resolve(null)),
    message: vi.fn(() => Promise.resolve()),
    ask: vi.fn(() => Promise.resolve(false)),
    confirm: vi.fn(() => Promise.resolve(false)),
}));

// Mock Tauri notification plugin
vi.mock('@tauri-apps/plugin-notification', () => ({
    sendNotification: vi.fn(),
    isPermissionGranted: vi.fn(() => Promise.resolve(true)),
    requestPermission: vi.fn(() => Promise.resolve('granted')),
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
