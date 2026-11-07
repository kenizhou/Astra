// SPDX-License-Identifier: GPL-3.0-only
// The draft row's opening path: the by-key read the row's Open verb drives,
// the standing composition's handover, and the rows the resume carries landing
// on the composer's document as pre-attached attachments.

import { beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => {
  // The by-key read's staged answer; unstaged, every host command refuses,
  // the plain-browser shape the rest of the tree expects.
  const byKey = { staged: false, view: null as unknown };
  return {
    byKey,
    invoke: vi.fn((cmd: string, _args: unknown) => {
      if (cmd === 'draft_resume_by_key' && byKey.staged) {
        return Promise.resolve(byKey.view);
      }
      return Promise.reject(new Error('the test host serves no commands'));
    }),
    dispatch: vi.fn(),
  };
});

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: ipc.invoke }));

import { draftResumeByKey, openDraftRow } from './draftOpen';
import { useComposerStore } from './composerStore';
import type { ResumeView } from '@/services/ipc';

/** A stored draft with one carried file, the answer the by-key read gives. */
function view(): ResumeView {
  return {
    account: 'work',
    to: 'you@test.local',
    cc: '',
    bcc: '',
    subject: 'Later',
    body_text: 'Half written',
    attachments: [
      {
        id: 'carry-0',
        blob: 'carry:0',
        file_name: 'report.pdf',
        media_type: 'application/pdf',
        size: 3,
        disposition: 'Attachment',
        data_url: null,
      },
    ],
  };
}

beforeEach(() => {
  ipc.byKey.staged = false;
  ipc.byKey.view = null;
  ipc.invoke.mockClear();
  ipc.dispatch.mockReset();
  useComposerStore.getState().closePane();
});

describe('draftResumeByKey', () => {
  it('invokes the by-key command with the row reference and the composition', async () => {
    ipc.byKey.staged = true;
    ipc.byKey.view = null;
    await expect(draftResumeByKey({ account: 'work', key: 'd-1' }, 'compose-9')).resolves.toBeNull();
    expect(ipc.invoke).toHaveBeenCalledWith('draft_resume_by_key', {
      message: { account: 'work', key: 'd-1' },
      composition: 'compose-9',
    });
  });
});

describe('openDraftRow', () => {
  it('hands a standing composition over and hydrates the resumed draft with its rows', async () => {
    useComposerStore.getState().newMessage({ id: 'work', label: 'Work' }, 'compose-1');
    ipc.byKey.staged = true;
    ipc.byKey.view = view();

    await openDraftRow({ account: 'work', key: 'd-1' });

    const save = ipc.dispatch.mock.calls[0]?.[0] as {
      drafts: { save: { composition: string; then_close: boolean } };
    };
    expect(save.drafts.save.composition).toBe('compose-1');
    expect(save.drafts.save.then_close).toBe(true);

    const call = ipc.invoke.mock.calls.find(([name]) => name === 'draft_resume_by_key');
    expect(call?.[1]).toMatchObject({ message: { account: 'work', key: 'd-1' } });
    const composition = (call?.[1] as { composition: string }).composition;
    expect(composition).not.toBe('compose-1');

    const store = useComposerStore.getState();
    expect(store.open).toBe(true);
    expect(store.composition).toBe(composition);
    expect(store.subject).toBe('Later');
    expect(store.to).toBe('you@test.local');
    expect(store.editor.attachments).toEqual(view().attachments);
  });

  it('leaves the pane shut when the core refuses the key', async () => {
    ipc.byKey.staged = true;
    ipc.byKey.view = null;
    await openDraftRow({ account: 'work', key: 'gone-1' });
    expect(useComposerStore.getState().open).toBe(false);
    expect(useComposerStore.getState().composition).toBeNull();
  });
});
