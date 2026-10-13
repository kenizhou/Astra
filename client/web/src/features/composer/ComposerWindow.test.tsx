// SPDX-License-Identifier: GPL-3.0-only
// The pop-out composer window's root: it hydrates its own session from the
// host under the URL's composition id (the pending request's prefill through
// the consuming take, or a blank new message on the first account), the
// hydration is StrictMode safe, a failed pull leaves the placeholder, and
// the composition leaving the store closes the OS window.
import { StrictMode } from 'react';
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  mailboxList: vi.fn(),
  settings: vi.fn(),
  draftStatus: vi.fn(),
  takeComposeRequest: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  mailboxList: ipc.mailboxList,
  settings: ipc.settings,
  draftStatus: ipc.draftStatus,
  takeComposeRequest: ipc.takeComposeRequest,
}));

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: true,
}));

const currentWindow = vi.hoisted(() => ({ close: vi.fn() }));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    close: currentWindow.close,
    minimize: vi.fn(async () => undefined),
    toggleMaximize: vi.fn(async () => undefined),
    isMaximized: vi.fn(async () => false),
    onResized: vi.fn(async () => () => undefined),
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => undefined),
}));

import type { ComposeRequestSnapshot } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useComposeRequestStore } from '@/stores/composeRequestStore';
import { useDraftStore } from '@/stores/draftStore';

import { useComposerStore } from './composerStore';
import { ComposerWindow } from './ComposerWindow';

/** The settings pull's answer: the shipped defaults plus an about block. */
const settingsSnapshot = {
  settings: {
    theme: 'light',
    twenty_four_clock: false,
    zone: 'Etc/UTC',
    first_day_of_week: 'mo',
    horizon_days: 42,
    quote_below: true,
    view_mode: 'flat',
  },
  accounts: [],
  about: { version: 'test' },
};

/** A reply request the take answers with. */
function replyRequest(): ComposeRequestSnapshot {
  return {
    kind: 'reply',
    from: 'work',
    to: 'ada@example.test',
    cc: '',
    subject: 'Re: Quarterly report',
    in_reply_to: 'orig-1@server.test',
    document: { blocks: [], attachments: [] },
  };
}

function resetStores(): void {
  useAccountStore.setState({ accounts: [] });
  useComposeRequestStore.setState({ snapshot: null });
  useDraftStore.setState({ snapshot: null });
  useComposerStore.getState().closePane();
  useComposerStore.setState({ resumeAttempted: true });
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.mailboxList.mockReset();
  ipc.mailboxList.mockResolvedValue({
    account_folders: [{ account_id: 'work', display_name: 'Work', folders: [] }],
  });
  ipc.settings.mockReset();
  ipc.settings.mockResolvedValue(settingsSnapshot);
  ipc.draftStatus.mockReset();
  ipc.draftStatus.mockResolvedValue(null);
  ipc.takeComposeRequest.mockReset();
  ipc.takeComposeRequest.mockResolvedValue(null);
  currentWindow.close.mockReset();
  currentWindow.close.mockResolvedValue(undefined);
  resetStores();
});

afterEach(cleanup);

/** Flushes the hydration's pull chain. */
async function settle(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe('the window hydration', () => {
  it('hydrates the pending request under the URL composition id', async () => {
    ipc.takeComposeRequest.mockResolvedValue(replyRequest());
    render(<ComposerWindow composition="compose-41" />);

    await settle();

    expect(ipc.takeComposeRequest).toHaveBeenCalledTimes(1);
    const state = useComposerStore.getState();
    expect(state.open).toBe(true);
    expect(state.composition).toBe('compose-41');
    expect(state.from).toBe('work');
    expect(state.fromLabel).toBe('Work');
    expect(state.to).toBe('ada@example.test');
    expect(state.request).toEqual({ kind: 'reply', inReplyTo: 'orig-1@server.test' });
    // The window's copy of the request is spent on arrival.
    expect(useComposeRequestStore.getState().snapshot).toBeNull();
    // The title bar carries the subject.
    expect(screen.getByText('Re: Quarterly report')).toBeTruthy();
    // The theme the settings block carries applied to the document.
    expect(document.documentElement.classList.contains('light')).toBe(true);
  });

  it('opens a blank new message on the first account when nothing is pending', async () => {
    render(<ComposerWindow composition="compose-42" />);

    await settle();

    const state = useComposerStore.getState();
    expect(state.open).toBe(true);
    expect(state.composition).toBe('compose-42');
    expect(state.from).toBe('work');
    expect(state.fromLabel).toBe('Work');
    expect(state.to).toBe('');
    expect(state.request).toBeNull();
    expect(screen.getByText('New message')).toBeTruthy();
  });

  it('hydrates once under a StrictMode double mount', async () => {
    ipc.takeComposeRequest.mockResolvedValue(replyRequest());
    render(
      <StrictMode>
        <ComposerWindow composition="compose-43" />
      </StrictMode>,
    );

    await settle();

    expect(ipc.takeComposeRequest).toHaveBeenCalledTimes(1);
    expect(useComposerStore.getState().composition).toBe('compose-43');
  });

  it('leaves the placeholder and logs when the pulls fail', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    ipc.mailboxList.mockRejectedValue(new Error('the host is unreachable'));
    render(<ComposerWindow composition="compose-44" />);

    await settle();

    expect(useComposerStore.getState().open).toBe(false);
    expect(screen.getByRole('status')).toBeTruthy();
    expect(error).toHaveBeenCalled();
    error.mockRestore();
  });
});

describe('the window lifetime', () => {
  it('closes the OS window when the composition leaves the store', async () => {
    render(<ComposerWindow composition="compose-45" />);
    await settle();
    expect(useComposerStore.getState().open).toBe(true);

    act(() => {
      useComposerStore.getState().closePane();
    });

    expect(currentWindow.close).toHaveBeenCalledTimes(1);
  });

  it('shows the composer chrome once hydrated', async () => {
    render(<ComposerWindow composition="compose-46" />);
    await settle();

    expect(screen.getByRole('button', { name: 'Send' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Close composer' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Bold' })).toBeTruthy();
    expect(screen.getByText('0 words · 0 characters')).toBeTruthy();
  });
});
