import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { sveltekit } from '@sveltejs/kit/vite';
import path from 'path';

export default defineConfig({
    plugins: [sveltekit()],
    resolve: {
        alias: {
            $lib: path.resolve('./src/lib'),
        },
        conditions: ['browser'],
    },
    test: {
        include: ['src/**/*.{test,spec}.{js,ts}'],
        environment: 'jsdom',
        globals: true,
        setupFiles: ['./vitest-setup.js'],
        coverage: {
            provider: 'v8',
            reporter: ['text', 'html'],
            include: ['src/**/*.{js,ts,svelte}'],
            exclude: ['src/**/*.test.{js,ts}', 'src/lib/components/icons.js'],
        },
    },
});
