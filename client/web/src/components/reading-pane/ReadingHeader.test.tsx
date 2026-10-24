// SPDX-License-Identifier: GPL-3.0-only
// The reading header: subject, the sender avatar beside the name, and the
// full UTC stamp the pane draws from the snapshot, with the bare address
// standing in for an empty sender name and the standard placeholder for an
// empty subject.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ peopleAvatar: vi.fn(), dispatch: vi.fn() }));
const saves = vi.hoisted(() => ({ saveEml: vi.fn() }));
const readerWindow = vi.hoisted(() => ({
  openMessageWindow: vi.fn(),
  openPrintWindow: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  peopleAvatar: ipc.peopleAvatar,
  dispatch: ipc.dispatch,
}));

vi.mock('@/services/attachments', () => ({ saveEml: saves.saveEml }));

vi.mock('@/features/reader/readerWindows', () => ({
  openMessageWindow: readerWindow.openMessageWindow,
  openPrintWindow: readerWindow.openPrintWindow,
}));

const settingsState = vi.hoisted(() => ({
  settings: { twenty_four_clock: true },
}));

vi.mock('@/features/settings/settingsStore', () => ({
  useSettingsStore: (selector: (state: unknown) => unknown) => selector(settingsState),
}));

import { ReadingHeader, formatReadingDate } from './ReadingHeader';
import { useAppearanceStore } from '@/services/appearance';

beforeEach(() => {
  ipc.peopleAvatar.mockReset();
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  // Every header with a sender resolves an avatar; a test that cares
  // overrides the answer.
  ipc.peopleAvatar.mockResolvedValue({ initials: '', colour: '' });
  saves.saveEml.mockReset();
  readerWindow.openMessageWindow.mockReset();
  readerWindow.openPrintWindow.mockReset();
});

afterEach(cleanup);

describe('the reading header', () => {
  it('draws subject, sender, and the full date', () => {
    render(
      <ReadingHeader
        subject="Launch"
        fromName="Ada Lovelace"
        fromAddress="ada@example.test"
        sentAt="2026-10-09T08:30:00Z"
      />,
    );

    expect(screen.getByText('Launch')).toBeTruthy();
    expect(screen.getByText('Ada Lovelace')).toBeTruthy();
    expect(screen.getByText('ada@example.test')).toBeTruthy();
    expect(screen.getByText('9 Oct 2026, 08:30')).toBeTruthy();
  });

  it('shows the bare address when the name is empty', () => {
    render(
      <ReadingHeader
        subject="Digest"
        fromName=""
        fromAddress="notifications@example.test"
        sentAt="2026-10-09T08:30:00Z"
      />,
    );

    expect(screen.getByText('notifications@example.test')).toBeTruthy();
  });

  it('draws the placeholder subject and drops an empty sender line', () => {
    render(
      <ReadingHeader subject="" fromName="" fromAddress="" sentAt="2026-10-09T08:30:00Z" />,
    );

    expect(screen.getByText('(no subject)')).toBeTruthy();
    expect(screen.queryByText('<>')).toBeNull();
  });

  it('mounts the sender avatar beside the name, resolved by the address', async () => {
    ipc.peopleAvatar.mockResolvedValue({ initials: 'AL', colour: '#4DB6AC' });
    render(
      <ReadingHeader
        subject="Launch"
        fromName="Ada Lovelace"
        fromAddress="ada@example.test"
        sentAt="2026-10-09T08:30:00Z"
      />,
    );

    expect(ipc.peopleAvatar).toHaveBeenCalledWith('ada@example.test');
    const avatar = await waitFor(() => {
      const badge = screen.getByRole('img', { name: 'Avatar' });
      expect(badge.textContent).toBe('AL');
      return badge;
    });
    const sender = screen.getByText('Ada Lovelace');
    expect(avatar.compareDocumentPosition(sender) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('draws no avatar when there is no sender', () => {
    render(
      <ReadingHeader subject="" fromName="" fromAddress="" sentAt="2026-10-09T08:30:00Z" />,
    );

    expect(screen.queryByRole('img', { name: 'Avatar' })).toBeNull();
    expect(ipc.peopleAvatar).not.toHaveBeenCalled();
  });

  it('formats a whole-year stamp and renders nothing for an unknown one', () => {
    expect(formatReadingDate('2026-10-09T08:30:00Z')).toBe('9 Oct 2026, 08:30');
    expect(formatReadingDate('2021-01-02T03:04:00Z')).toBe('2 Jan 2021, 03:04');
    expect(formatReadingDate('')).toBe('');
    expect(formatReadingDate('not a date')).toBe('');
  });

  it('renders the stamp in 12 hour form when the clock setting says so', () => {
    settingsState.settings.twenty_four_clock = false;
    render(
      <ReadingHeader
        subject="Launch"
        fromName="Ada Lovelace"
        fromAddress="ada@example.test"
        sentAt="2026-10-09T13:05:00Z"
      />,
    );

    expect(screen.getByText('9 Oct 2026, 1:05 PM')).toBeTruthy();
    expect(screen.queryByText('9 Oct 2026, 13:05')).toBeNull();
    settingsState.settings.twenty_four_clock = true;
  });

  it('formats noon and midnight on the 12 hour clock', () => {
    expect(formatReadingDate('2026-10-09T00:05:00Z', false)).toBe('9 Oct 2026, 12:05 AM');
    expect(formatReadingDate('2026-10-09T12:05:00Z', false)).toBe('9 Oct 2026, 12:05 PM');
  });
});

describe('the header actions', () => {
  const message = { account: 'work', key: 'm1' };

  it('offers the three compose verbs, each dispatching on the open message', () => {
    render(
      <ReadingHeader
        subject='Launch'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
        message={message}
      />,
    );

    for (const label of ['Reply', 'Reply all', 'Forward']) {
      expect(screen.getByRole('button', { name: label })).toBeTruthy();
    }
  });

  it('dispatches the verbs from the verb row', () => {
    render(
      <ReadingHeader
        subject='Launch'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
        message={message}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Reply' }));
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"compose":{"reply":{"message":{"account":"work","key":"m1"}}}}',
    );
    fireEvent.click(screen.getByRole('button', { name: 'Reply all' }));
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"compose":{"reply_all":{"message":{"account":"work","key":"m1"}}}}',
    );
    fireEvent.click(screen.getByRole('button', { name: 'Forward' }));
    expect(JSON.stringify(ipc.dispatch.mock.calls[2]?.[0])).toBe(
      '{"compose":{"forward":{"message":{"account":"work","key":"m1"}}}}',
    );
  });

  it('holds the remaining placeholders in the overflow menu beside the working verbs', () => {
    render(
      <ReadingHeader
        subject='Launch'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
        message={message}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'More actions' }));

    for (const label of ['Recipients', 'View source']) {
      const item = screen.getByRole('menuitem', { name: label });
      expect(item.getAttribute('aria-disabled')).toBe('true');
    }
    for (const label of ['Open in New Window', 'Print', 'Save as .eml']) {
      const item = screen.getByRole('menuitem', { name: label });
      expect(item.getAttribute('aria-disabled')).toBeNull();
    }
  });

  it('saves the open message as .eml and surfaces the outcome like the attachment save', async () => {
    saves.saveEml.mockResolvedValue({ saved: { path: 'C:\\mail\\Launch.eml' } });
    render(
      <ReadingHeader
        subject='Launch'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
        message={message}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'More actions' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Save as .eml' }));

    expect(saves.saveEml).toHaveBeenCalledWith({ account: 'work', key: 'm1' });
    expect(await screen.findByRole('status').then((node) => node.textContent)).toBe(
      'Saved to C:\\mail\\Launch.eml',
    );
  });

  it('surfaces the refusal sentence when the .eml save fails and nothing on cancel', async () => {
    saves.saveEml.mockResolvedValue({ failed: { reason: 'the disk refused' } });
    render(
      <ReadingHeader
        subject='Launch'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
        message={message}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'More actions' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Save as .eml' }));

    expect(
      await screen.findByRole('status').then((node) => node.textContent),
    ).toBe('the disk refused');

    cleanup();
    saves.saveEml.mockResolvedValue('cancelled');
    render(
      <ReadingHeader
        subject='Launch'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
        message={message}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'More actions' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Save as .eml' }));
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('opens the print window over the open message', () => {
    render(
      <ReadingHeader
        subject='Launch'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
        message={message}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'More actions' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Print' }));

    expect(readerWindow.openPrintWindow).toHaveBeenCalledTimes(1);
    expect(readerWindow.openPrintWindow).toHaveBeenCalledWith({ account: 'work', key: 'm1' });
  });

  it('opens the own window over the open message and the folder it was opened from', () => {
    render(
      <ReadingHeader
        subject='Launch'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
        message={message}
        openedFromFolder={{ account: 'work', key: 'junk' }}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'More actions' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Open in New Window' }));

    expect(readerWindow.openMessageWindow).toHaveBeenCalledWith(
      { account: 'work', key: 'm1' },
      'Launch',
      'junk',
    );
  });

  it('draws no verbs and no overflow without a message', () => {
    render(
      <ReadingHeader
        subject='Launch'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
      />,
    );

    expect(screen.queryByRole('button', { name: 'Reply' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'More actions' })).toBeNull();
  });
});

describe('the not-spam verb', () => {
  const message = { account: 'work', key: 'm1' };

  it('offers Not spam on the verb row only for a message opened from the junk folder', () => {
    const { rerender } = render(
      <ReadingHeader
        subject='Misfiled'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
        message={message}
        notJunk
      />,
    );
    expect(screen.getByRole('button', { name: 'Not spam' })).toBeTruthy();

    rerender(
      <ReadingHeader
        subject='Misfiled'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
        message={message}
      />,
    );
    expect(screen.queryByRole('button', { name: 'Not spam' })).toBeNull();
  });

  it('reports the message as legitimate through the mark_not_junk intent', () => {
    render(
      <ReadingHeader
        subject='Misfiled'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
        message={message}
        notJunk
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Not spam' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"mark_not_junk":{"row":{"message":{"account":"work","key":"m1"}}}}',
    );
  });
});

describe('the zoom menu', () => {
  beforeEach(() => {
    useAppearanceStore.setState({ readingZoom: 'default' });
  });

  it('offers the three steps with the standing one checked', () => {
    useAppearanceStore.setState({ readingZoom: 'large' });
    render(
      <ReadingHeader
        subject='Launch'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Reading zoom' }));

    const items = screen.getAllByRole('menuitem');
    expect(items.map((i) => i.textContent)).toEqual(['Smaller', 'Default', 'Larger']);
    expect(items[2]!.querySelector('svg')).not.toBeNull();
    expect(items[0]!.querySelector('svg')).toBeNull();
  });

  it('sets the step from the menu and persists it in the appearance store', () => {
    render(
      <ReadingHeader
        subject='Launch'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Reading zoom' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Smaller' }));

    expect(useAppearanceStore.getState().readingZoom).toBe('small');
    expect(localStorage.getItem('astra.readingZoom')).toBe('small');
  });
});

describe('the position menu', () => {
  beforeEach(() => {
    useAppearanceStore.setState({ readingPosition: 'right' });
  });

  it('offers the three positions with the current one checked', () => {
    useAppearanceStore.setState({ readingPosition: 'bottom' });
    render(
      <ReadingHeader
        subject='Launch'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Reading pane position' }));

    const items = screen.getAllByRole('menuitem');
    expect(items.map((i) => i.textContent)).toEqual(['Right', 'Bottom', 'Off']);
    expect(items[1]!.querySelector('svg')).not.toBeNull();
    expect(items[0]!.querySelector('svg')).toBeNull();
  });

  it('sets the position from the menu', () => {
    render(
      <ReadingHeader
        subject='Launch'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Reading pane position' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Off' }));

    expect(useAppearanceStore.getState().readingPosition).toBe('off');
    expect(localStorage.getItem('astra.readingPosition')).toBe('off');
  });

  it('shows no close button anywhere', () => {
    render(
      <ReadingHeader
        subject='Launch'
        fromName='Ada'
        fromAddress='ada@example.test'
        sentAt='2026-10-09T08:30:00Z'
        message={{ account: 'work', key: 'm1' }}
      />,
    );

    expect(screen.queryByRole('button', { name: 'Close reading pane' })).toBeNull();
  });
});
