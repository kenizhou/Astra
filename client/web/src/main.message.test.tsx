// SPDX-License-Identifier: GPL-3.0-only
// The entry point's message branch: a URL naming a message's account and key
// mounts the pop-out reading window's root in place of the app shell. One
// module evaluation per test file, so each branch has its own file.
import { waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@/App', () => ({ default: () => <div data-testid="app-root" /> }));
vi.mock('@/features/composer/ComposerWindow', () => ({
  ComposerWindow: () => <div data-testid="composer-root" />,
}));
vi.mock('@/features/reader/MessageWindow', () => ({
  MessageWindow: ({ message, folderKey }: { message: { account: string; key: string }; folderKey: string | null }) => (
    <div data-testid="message-root">
      {message.account}:{message.key}:{folderKey ?? 'none'}
    </div>
  ),
}));
vi.mock('@/features/reader/PrintWindow', () => ({
  PrintWindow: () => <div data-testid="print-root" />,
}));

describe('the entry point on a message URL', () => {
  it('mounts the reading window root over the named message', async () => {
    window.history.replaceState(null, '', '/?message-account=work&message-key=m1&folder=Junk');
    document.body.innerHTML = '<div id="root"></div>';

    await import('./main');

    // createRoot renders on React's schedule, not the import's.
    await waitFor(() => {
      expect(document.querySelector('[data-testid="message-root"]')?.textContent).toBe(
        'work:m1:Junk',
      );
    });
    expect(document.querySelector('[data-testid="app-root"]')).toBeNull();
    expect(document.querySelector('[data-testid="composer-root"]')).toBeNull();
  });
});
