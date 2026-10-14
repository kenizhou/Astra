// SPDX-License-Identifier: GPL-3.0-only
/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import path from 'node:path';

import { tauriBridgeAliases } from './fixture-alias';

// The fixtures mode (`npm run dev:fixtures`, vite's `--mode fixtures`) swaps
// the Tauri bridge for the recorded-snapshot harness, so the real UI runs in
// a plain browser for the design-review and QA skills. Test and production
// builds keep the real bridge: vitest mocks it per test, and the webview
// build always runs inside the host. The alias decision itself lives in
// fixture-alias.ts, pinned by harness.test.ts.
export default defineConfig(({ mode }) => ({
  plugins: [react(), tailwindcss()],
  test: {
    environment: 'jsdom',
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      ...tauriBridgeAliases(mode, __dirname),
    },
  },
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
}));
