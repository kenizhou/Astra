// SPDX-License-Identifier: GPL-3.0-only
// The entry point's app branch: a URL naming no composition mounts the app
// shell as before. One module evaluation per test file, so the two branches
// have their own files.
import { waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@/App', () => ({ default: () => <div data-testid="app-root" /> }));
vi.mock('@/features/composer/ComposerWindow', () => ({
  ComposerWindow: ({ composition }: { composition: string }) => (
    <div data-testid="composer-root">{composition}</div>
  ),
}));

describe('the entry point on a plain URL', () => {
  it('mounts the app shell', async () => {
    window.history.replaceState(null, '', '/');
    document.body.innerHTML = '<div id="root"></div>';

    await import('./main');

    // createRoot renders on React's schedule, not the import's.
    await waitFor(() => {
      expect(document.querySelector('[data-testid="app-root"]')).not.toBeNull();
    });
    expect(document.querySelector('[data-testid="composer-root"]')).toBeNull();
  });
});
