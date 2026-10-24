// SPDX-License-Identifier: GPL-3.0-only
// The composer's pending share set: the OS hand-off's files stand beside
// the document with their names shown and their bytes unstaged (the read
// seam is the row's named remainder), and the send refuses honestly
// while any stands. Split from composerStore.test.ts for the line cap.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const settingsState = vi.hoisted(() => ({
  settings: { quote_below: true },
  accounts: [] as { account: string; signatures?: unknown }[],
}));

vi.mock('@/features/settings/settingsStore', () => ({
  useSettingsStore: { getState: () => settingsState },
}));

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { useComposerStore } from './composerStore';

/** One shared file, the way the OS hand-off names it. */
const NOTES = { name: 'notes.txt', path: 'C:\\Users\\ada\\notes.txt' };
const PHOTO = { name: 'photo.png', path: 'C:\\Users\\ada\\photo.png' };

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useComposerStore.getState().closePane();
});

describe('the pending share set', () => {
  it('seeds the shared files beside a plain composition and drops one by name', () => {
    useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });
    useComposerStore.getState().seedShares([NOTES, PHOTO]);

    let state = useComposerStore.getState();
    expect(state.pendingShares).toEqual([NOTES, PHOTO]);
    expect(state.editor.attachments).toHaveLength(0);

    useComposerStore.getState().removeShare(NOTES);
    state = useComposerStore.getState();
    expect(state.pendingShares).toEqual([PHOTO]);
  });

  it('drops only the named row among two same-named shares from different paths', () => {
    // A command-line hand-off can carry two files of one name from
    // different directories: one row's removal spends one row.
    const otherNotes = { name: 'notes.txt', path: 'C:\Elsewhere\notes.txt' };
    useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });
    useComposerStore.getState().seedShares([NOTES, otherNotes]);

    useComposerStore.getState().removeShare(NOTES);

    const state = useComposerStore.getState();
    expect(state.pendingShares).toEqual([otherNotes]);
  });

  it('refuses the send while a pending share stands, with the sentence', () => {
    useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });
    useComposerStore.getState().setField('to', 'ada@example.test');
    useComposerStore.getState().seedShares([NOTES]);

    useComposerStore.getState().submit();

    const state = useComposerStore.getState();
    expect(state.sending).toBe(false);
    expect(state.sendError).toBe(
      'Shared files cannot be attached yet. Remove them to send this message.',
    );
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('sends once the last pending share is removed', async () => {
    useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });
    useComposerStore.getState().setField('to', 'ada@example.test');
    useComposerStore.getState().seedShares([NOTES]);

    useComposerStore.getState().removeShare(NOTES);
    useComposerStore.getState().submit();
    await vi.waitFor(() =>
      expect(
        ipc.dispatch.mock.calls.some(([intent]) =>
          JSON.stringify(intent).includes('submit_rich_mail'),
        ),
      ).toBe(true),
    );
  });

  it('clears the pending set when a composition closes', () => {
    useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });
    useComposerStore.getState().seedShares([NOTES]);

    useComposerStore.getState().closePane();

    expect(useComposerStore.getState().pendingShares).toEqual([]);
  });
});
