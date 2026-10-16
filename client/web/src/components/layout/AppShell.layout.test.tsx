// SPDX-License-Identifier: GPL-3.0-only
// The shell's mail layout and the window stages: the reading pane stands on
// its empty state until a snapshot lands, the drag handles part the cards,
// and the panes leave from the left as the window narrows (the folder pane
// into the drawer at medium, the list and the reading pane trading the
// frame at narrow). The mock floor mirrors AppShell.test.tsx.

import { act, cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// jsdom has no ResizeObserver; the resizable panel row measures through one.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
vi.stubGlobal('ResizeObserver', ResizeObserverStub);

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  draftResume: vi.fn(),
  settings: vi.fn(),
  takeComposeRequest: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  draftResume: ipc.draftResume,
  settings: ipc.settings,
  takeComposeRequest: ipc.takeComposeRequest,
}));

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: true,
}));

vi.mock('@/features/composer/composeWindow', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openComposerWindow: vi.fn(async () => undefined),
  openNewMessageComposer: vi.fn(async () => undefined),
}));

const agent = vi.hoisted(() => ({
  agentStatus: vi.fn(),
  agentPrompt: vi.fn(),
  agentAbort: vi.fn(),
  permissionResponse: vi.fn(),
}));

vi.mock('@/services/agent', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...agent,
}));

import { AppShell } from './AppShell';
import { useAppearanceStore } from '@/services/appearance';
import { useAccountStore } from '@/stores/accountStore';
import { useFolderStore } from '@/stores/folderStore';
import { useReadingStore } from '@/stores/readingStore';
import { useSendingStore } from '@/stores/sendingStore';
import { useUiStore } from '@/stores/uiStore';
import { useAgentStore } from '@/features/agent/agentStore';
import { useComposerStore } from '@/features/composer/composerStore';
import { documentToEditor } from '@/features/composer/documentWire';
import { useSendResultStore } from '@/features/outbox/sendResult';

/** The reading snapshot the layout tests open. */
const SNAPSHOT = {
  message: { account: 'work', key: 'imap:v1:q3' },
  subject: 'Quarterly report',
  from_name: 'Ada Lovelace',
  from_address: 'ada@example.test',
  sent_at: '2026-10-09T08:30:00Z',
  plain: 'Hi',
  remote_image_count: 0,
};

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.draftResume.mockReset();
  ipc.draftResume.mockResolvedValue(null);
  ipc.settings.mockReset();
  ipc.settings.mockResolvedValue({
    settings: {
      theme: 'follow_host',
      twenty_four_clock: false,
      zone: 'Etc/UTC',
      first_day_of_week: 'mo',
      horizon_days: 42,
      quote_below: true,
      view_mode: 'flat',
    },
    accounts: [],
    about: { version: '0.1.0' },
  });
  ipc.takeComposeRequest.mockReset();
  ipc.takeComposeRequest.mockResolvedValue(null);
  agent.agentStatus.mockReset();
  agent.agentStatus.mockResolvedValue({ state: 'down', model: null });
  agent.agentPrompt.mockReset();
  agent.agentAbort.mockReset();
  agent.permissionResponse.mockReset();
  useAgentStore.getState().reset();
  // The pane stages read the window width at mount; the suite runs wide.
  window.innerWidth = 1280;
  useUiStore.getState().setShellView('mail');
  useUiStore.getState().setSettingsOpen(false);
  useUiStore.getState().setAgentOpen(false);
  useUiStore.getState().setFolderDrawerOpen(false);
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work' }] });
  useComposerStore.setState({
    open: false,
    inlineExpected: false,
    resumeAttempted: false,
    composition: null,
    from: '',
    fromLabel: '',
    to: '',
    cc: '',
    bcc: '',
    subject: '',
    editor: documentToEditor({ blocks: [{ Paragraph: { content: [] } }], attachments: [] }),
    savedFingerprint: null,
    request: null,
  });
  useSendingStore.getState().applySnapshot({ rows: [] });
  useFolderStore.setState({ accounts: [], selected: null, outboxAccount: null });
  useReadingStore.getState().applySnapshot(null);
  useAppearanceStore.setState({ readingPosition: 'right' });
  useSendResultStore.getState().dismiss();
  useSendResultStore.setState({ settled: [], answered: [] });
});

afterEach(cleanup);

describe('the mail row layout', () => {
  it('keeps the reading pane standing on its empty state while nothing is open', () => {
    render(<AppShell />);

    const pane = screen.getByRole('complementary', { name: 'Reading pane' });
    expect(within(pane).getByText(/Select a message/)).toBeTruthy();
  });

  it('fills the message list pane to the full height of the row', () => {
    render(<AppShell />);

    const list = screen.getByRole('region', { name: 'Message list' });
    expect(list.className).toContain('h-full');
  });

  it('opens the message in the reading pane when a reading snapshot lands', () => {
    render(<AppShell />);
    act(() => {
      useReadingStore.getState().applySnapshot(SNAPSHOT);
    });

    const pane = screen.getByRole('complementary', { name: 'Reading pane' });
    expect(within(pane).getByText('Quarterly report')).toBeTruthy();
  });

  it('offers a drag handle between the folder pane and the list', () => {
    render(<AppShell />);

    // One separator per boundary: folder|right zone, then list|reading.
    expect(screen.getAllByRole('separator').length).toBeGreaterThanOrEqual(2);
  });
});

describe('the pane stages', () => {
  it('wide: the folder pane, the list, and the reading pane all stand', () => {
    window.innerWidth = 1280;
    render(<AppShell />);

    expect(screen.getByRole('tree', { name: 'Accounts' })).toBeTruthy();
    expect(screen.getByRole('region', { name: 'Message list' })).toBeTruthy();
    expect(screen.getByRole('complementary', { name: 'Reading pane' })).toBeTruthy();
  });

  it('medium: the folder pane leaves first, the list and reading pane stay', () => {
    window.innerWidth = 1000;
    render(<AppShell />);

    expect(screen.queryByRole('tree', { name: 'Accounts' })).toBeNull();
    expect(screen.getByRole('region', { name: 'Message list' })).toBeTruthy();
    expect(screen.getByRole('complementary', { name: 'Reading pane' })).toBeTruthy();
  });

  it('narrow with nothing open: the list fills the frame alone', () => {
    window.innerWidth = 700;
    render(<AppShell />);

    expect(screen.queryByRole('tree', { name: 'Accounts' })).toBeNull();
    expect(screen.getByRole('region', { name: 'Message list' })).toBeTruthy();
    expect(screen.queryByRole('complementary', { name: 'Reading pane' })).toBeNull();
  });

  it('narrow with a message open: the reading pane fills the frame alone', () => {
    window.innerWidth = 700;
    render(<AppShell />);
    act(() => {
      useReadingStore.getState().applySnapshot(SNAPSHOT);
    });

    expect(screen.queryByRole('region', { name: 'Message list' })).toBeNull();
    expect(screen.getByRole('complementary', { name: 'Reading pane' })).toBeTruthy();
  });
});

describe('the reading pane position', () => {
  it('bottom: the reading pane docks beneath the list in one column', () => {
    window.innerWidth = 1280;
    useAppearanceStore.setState({ readingPosition: 'bottom' });
    render(<AppShell />);

    expect(screen.getByRole('region', { name: 'Message list' })).toBeTruthy();
    expect(screen.getByRole('complementary', { name: 'Reading pane' })).toBeTruthy();
    // The vertical stack adds a row-resize handle beside the column ones.
    expect(document.querySelector('.cursor-row-resize')).not.toBeNull();
  });

  it('off: the list takes the whole row and the reading pane hides', () => {
    window.innerWidth = 1280;
    useAppearanceStore.setState({ readingPosition: 'off' });
    render(<AppShell />);

    expect(screen.getByRole('region', { name: 'Message list' })).toBeTruthy();
    expect(screen.queryByRole('complementary', { name: 'Reading pane' })).toBeNull();
  });

  it('off keeps hiding the pane even with a message open', () => {
    window.innerWidth = 1280;
    useAppearanceStore.setState({ readingPosition: 'off' });
    render(<AppShell />);
    act(() => {
      useReadingStore.getState().applySnapshot(SNAPSHOT);
    });

    expect(screen.getByRole('region', { name: 'Message list' })).toBeTruthy();
    expect(screen.queryByRole('complementary', { name: 'Reading pane' })).toBeNull();
  });

  it('narrow ignores the position: the open message still takes the frame', () => {
    window.innerWidth = 700;
    useAppearanceStore.setState({ readingPosition: 'bottom' });
    render(<AppShell />);
    act(() => {
      useReadingStore.getState().applySnapshot(SNAPSHOT);
    });

    expect(screen.queryByRole('region', { name: 'Message list' })).toBeNull();
    expect(screen.getByRole('complementary', { name: 'Reading pane' })).toBeTruthy();
  });
});
