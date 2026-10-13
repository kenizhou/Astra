// SPDX-License-Identifier: GPL-3.0-only
// The entry point's composer branch: a URL naming a composition mounts the
// pop-out composer window's root in place of the app shell. One module
// evaluation per test file, so the two branches have their own files.
import { waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@/App', () => ({ default: () => <div data-testid="app-root" /> }));
vi.mock('@/features/composer/ComposerWindow', () => ({
  ComposerWindow: ({ composition }: { composition: string }) => (
    <div data-testid="composer-root">{composition}</div>
  ),
}));

describe('the entry point on a composer URL', () => {
  it('mounts the composer root over the named composition', async () => {
    window.history.replaceState(null, '', '/?composer=compose-7');
    document.body.innerHTML = '<div id="root"></div>';

    await import('./main');

    // createRoot renders on React's schedule, not the import's.
    await waitFor(() => {
      expect(document.querySelector('[data-testid="composer-root"]')?.textContent).toBe(
        'compose-7',
      );
    });
    expect(document.querySelector('[data-testid="app-root"]')).toBeNull();
  });
});
