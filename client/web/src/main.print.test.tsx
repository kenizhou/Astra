// SPDX-License-Identifier: GPL-3.0-only
// The entry point's print branch: a message URL carrying the print marker
// mounts the print window's root in place of every other. One module
// evaluation per test file, so each branch has its own file.
import { waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@/App', () => ({ default: () => <div data-testid="app-root" /> }));
vi.mock('@/features/composer/ComposerWindow', () => ({
  ComposerWindow: () => <div data-testid="composer-root" />,
}));
vi.mock('@/features/reader/MessageWindow', () => ({
  MessageWindow: () => <div data-testid="message-root" />,
}));
vi.mock('@/features/reader/PrintWindow', () => ({
  PrintWindow: ({ message }: { message: { account: string; key: string } }) => (
    <div data-testid="print-root">
      {message.account}:{message.key}
    </div>
  ),
}));

describe('the entry point on a print URL', () => {
  it('mounts the print root over the named message', async () => {
    window.history.replaceState(null, '', '/?message-account=work&message-key=m1&print=1');
    document.body.innerHTML = '<div id="root"></div>';

    await import('./main');

    await waitFor(() => {
      expect(document.querySelector('[data-testid="print-root"]')?.textContent).toBe('work:m1');
    });
    expect(document.querySelector('[data-testid="app-root"]')).toBeNull();
    expect(document.querySelector('[data-testid="message-root"]')).toBeNull();
  });
});
