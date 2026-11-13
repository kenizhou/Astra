// SPDX-License-Identifier: GPL-3.0-only
// The attachment browser overlay: closed renders null, the dialog names itself
// a modal, the rows group by sender newest first with the count and total size,
// the mime chips filter without collapsing (derived from the unfiltered
// snapshot), the sender prefilter banner clears, the coverage footer states the
// window and the failed accounts, Refresh re-pulls, the empty state names the
// cache, Save As calls the service with the row's source, and opening the source
// message dispatches the intent and closes. Escape and the backdrop close. The
// attachment service is mocked whole; a mid-test store mutation rides act().

import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { AttachmentBrowserEntry, AttachmentBrowserView } from '@/services/attachments';

const attachments = vi.hoisted(() => ({
  browseAttachments: vi.fn(),
  saveAttachment: vi.fn(),
}));
const ipc = vi.hoisted(() => ({ dispatch: vi.fn(), intents: { openMessage: vi.fn() } }));

vi.mock('@/services/attachments', () => attachments);
vi.mock('@/services/ipc', () => ipc);

import { AttachmentBrowser } from './AttachmentBrowser';
import { openAttachmentBrowser, useAttachmentBrowserStore } from './attachmentBrowserStore';

const DIALOG = { name: 'Attachment browser' } as const;

/** One browsed row, overridable per test. */
function entry(over: Partial<AttachmentBrowserEntry> = {}): AttachmentBrowserEntry {
  return {
    id: 2,
    file_name: 'report.pdf',
    media_type: 'application/pdf',
    size: 1024,
    source: { account: 'work', key: 'm1' },
    subject: 'Launch',
    from_name: 'Ada Lovelace',
    from_address: 'ada@example.test',
    sent_at: '2026-10-09T08:00:00Z',
    ...over,
  };
}

/** A browse answer over the given rows. */
function view(entries: AttachmentBrowserEntry[], over: Partial<AttachmentBrowserView['coverage']> = {}): AttachmentBrowserView {
  return {
    entries,
    coverage: { scanned: entries.length, indexed: entries.length, skipped: 0, failed_accounts: [], scan_limit: 200, ...over },
  };
}

/** Mounts the overlay and opens it, awaiting the first pull. */
async function renderOpen(answer: AttachmentBrowserView = view([entry()])): Promise<void> {
  attachments.browseAttachments.mockResolvedValue(answer);
  render(<AttachmentBrowser />);
  act(() => {
    openAttachmentBrowser();
  });
  await waitFor(() => expect(attachments.browseAttachments).toHaveBeenCalled());
}

beforeEach(() => {
  attachments.browseAttachments.mockReset();
  attachments.saveAttachment.mockReset();
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.intents.openMessage.mockReset();
  ipc.intents.openMessage.mockImplementation((message) => ({ open_message: { message } }));
  useAttachmentBrowserStore.setState({ isOpen: false, sender: null });
});

afterEach(() => {
  useAttachmentBrowserStore.setState({ isOpen: false, sender: null });
  cleanup();
});

describe('the overlay gate', () => {
  it('renders nothing while the store holds it closed', () => {
    render(<AttachmentBrowser />);
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
  });

  it('names itself a modal at the modal tier', async () => {
    await renderOpen();
    const dialog = screen.getByRole('dialog', DIALOG);
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    expect(dialog.parentElement!.className).toContain('z-[var(--z-modal)]');
  });

  it('Escape closes the browser', async () => {
    await renderOpen();
    fireEvent.keyDown(screen.getByRole('dialog', DIALOG), { key: 'Escape' });
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
    expect(useAttachmentBrowserStore.getState().isOpen).toBe(false);
  });

  it('the backdrop closes but a click inside does not', async () => {
    await renderOpen();
    const dialog = screen.getByRole('dialog', DIALOG);
    fireEvent.mouseDown(dialog);
    expect(screen.getByRole('dialog', DIALOG)).toBeTruthy();
    fireEvent.mouseDown(dialog.parentElement!);
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
  });
});

describe('the grouped rows', () => {
  it('groups by sender with the count and total size', async () => {
    await renderOpen(
      view([
        entry({ source: { account: 'work', key: 'm2' }, from_address: 'grace@example.test', from_name: 'Grace Hopper', sent_at: '2026-10-09T09:30:00Z', file_name: 'notes.pdf' }),
        entry(),
        entry({ id: 3, file_name: 'logo.png', media_type: 'image/png', size: 2048 }),
      ]),
    );
    await screen.findByText('logo.png');
    const dialog = screen.getByRole('dialog', DIALOG);
    const groups = within(dialog).getAllByRole('region');
    expect(groups.map((group) => group.getAttribute('aria-label'))).toEqual([
      'Grace Hopper',
      'Ada Lovelace',
    ]);
    expect(groups[0]!.textContent).toContain('1 attachment');
    expect(groups[1]!.textContent).toContain('2 attachments');
    expect(groups[1]!.textContent).toContain('3.0 KB');
  });

  it('names the empty state telling the reader to open messages', async () => {
    await renderOpen(view([]));
    expect(screen.getByRole('dialog', DIALOG).textContent).toContain('Open messages');
  });
});

describe('the mime chips', () => {
  it('filters the rows by the chosen bucket without collapsing the chips', async () => {
    await renderOpen(
      view([entry(), entry({ id: 3, file_name: 'logo.png', media_type: 'image/png' })]),
    );
    await screen.findByText('logo.png');
    const dialog = screen.getByRole('dialog', DIALOG);
    fireEvent.click(within(dialog).getByRole('button', { name: /Images/ }));

    // The image row stands, the pdf row goes; the chips keep both buckets.
    expect(within(dialog).getByText('logo.png')).toBeTruthy();
    expect(within(dialog).queryByText('report.pdf')).toBeNull();
    expect(within(dialog).getByRole('button', { name: /Images/ })).toBeTruthy();
    expect(within(dialog).getByRole('button', { name: /PDF/ })).toBeTruthy();
    expect(within(dialog).getByRole('button', { name: /All/ })).toBeTruthy();
  });

  it('All restores every row', async () => {
    await renderOpen(
      view([entry(), entry({ id: 3, file_name: 'logo.png', media_type: 'image/png' })]),
    );
    await screen.findByText('logo.png');
    const dialog = screen.getByRole('dialog', DIALOG);
    fireEvent.click(within(dialog).getByRole('button', { name: /Images/ }));
    fireEvent.click(within(dialog).getByRole('button', { name: /All/ }));
    expect(within(dialog).getByText('report.pdf')).toBeTruthy();
    expect(within(dialog).getByText('logo.png')).toBeTruthy();
  });
});

describe('the sender prefilter', () => {
  it('shows the banner and clears it', async () => {
    attachments.browseAttachments.mockResolvedValue(view([entry()]));
    render(<AttachmentBrowser />);
    act(() => {
      openAttachmentBrowser('ada@example.test');
    });
    await waitFor(() => expect(attachments.browseAttachments).toHaveBeenCalled());

    const dialog = screen.getByRole('dialog', DIALOG);
    expect(dialog.textContent).toContain('ada@example.test');
    expect(attachments.browseAttachments).toHaveBeenCalledWith({ sender: 'ada@example.test', mime_prefix: null });

    fireEvent.click(within(dialog).getByRole('button', { name: 'Clear' }));
    await waitFor(() =>
      expect(attachments.browseAttachments).toHaveBeenCalledWith({ sender: null, mime_prefix: null }),
    );
    expect(useAttachmentBrowserStore.getState().sender).toBeNull();
  });
});

describe('the coverage footer', () => {
  it('states the indexed count, the skips, the window, and the failed accounts', async () => {
    await renderOpen(
      view([entry()], { scanned: 5, indexed: 4, skipped: 1, failed_accounts: ['personal'] }),
    );
    const footer = screen.getByRole('dialog', DIALOG).textContent ?? '';
    expect(footer).toContain('4');
    expect(footer).toContain('1');
    expect(footer).toContain('200');
    expect(footer).toContain('personal');
  });

  it('Refresh re-pulls the browse', async () => {
    await renderOpen();
    const dialog = screen.getByRole('dialog', DIALOG);
    fireEvent.click(within(dialog).getByRole('button', { name: 'Refresh' }));
    await waitFor(() => expect(attachments.browseAttachments).toHaveBeenCalledTimes(2));
  });
});

describe('the row verbs', () => {
  it('Save As calls the service with the row source and id', async () => {
    attachments.saveAttachment.mockResolvedValue({ saved: { path: 'C:\\saved\\report.pdf' } });
    await renderOpen();
    const dialog = screen.getByRole('dialog', DIALOG);
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save As report.pdf' }));
    await waitFor(() =>
      expect(attachments.saveAttachment).toHaveBeenCalledWith({ account: 'work', key: 'm1' }, 2),
    );
    expect((await screen.findByRole('status')).textContent).toContain('C:\\saved\\report.pdf');
  });

  it('opens the source message and closes the overlay', async () => {
    await renderOpen();
    const dialog = screen.getByRole('dialog', DIALOG);
    fireEvent.click(within(dialog).getByRole('button', { name: 'Open message Launch' }));
    expect(ipc.intents.openMessage).toHaveBeenCalledWith({ account: 'work', key: 'm1' });
    expect(ipc.dispatch).toHaveBeenCalledWith({ open_message: { message: { account: 'work', key: 'm1' } } });
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
  });

  it('Preview stands as a placeholder', async () => {
    await renderOpen();
    const dialog = screen.getByRole('dialog', DIALOG);
    expect(within(dialog).getByRole('button', { name: 'Preview report.pdf' })).toBeTruthy();
  });
});
