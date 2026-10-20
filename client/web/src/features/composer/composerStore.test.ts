// SPDX-License-Identifier: GPL-3.0-only
// The composer store's quote default: a request's quoted original opens
// below the reply or above it per the settings block, the reply typing in
// the space the setting leaves, and the document the composer then holds is
// the per-message truth the setting only seeds.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const settingsState = vi.hoisted(() => ({
  settings: { quote_below: false },
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
import { documentFromPlainText, editorToDocument } from './documentWire';
import type { Block, ComposeRequestSnapshot } from '@/services/ipc';

/** One plain paragraph block with the given words. */
function paragraph(text: string): Block {
  return {
    Paragraph: {
      content: [
        {
          Text: {
            text,
            bold: false,
            italic: false,
            underline: false,
            font_size: null,
            color: null,
            highlight: null,
            link: null,
          },
        },
      ],
    },
  };
}

/** The quoted original a reply request carries. */
function quoteBlock(): Block {
  return {
    Quote: {
      style: 'Indented',
      attribution: { line: 'Ada wrote:', headers: [] },
      body_html: '<div>The numbers are in.</div>',
      body_plain: 'The numbers are in.',
    },
  };
}

/** A reply request whose document holds a drafted line above the quote. */
function request(): ComposeRequestSnapshot {
  return {
    kind: 'reply',
    from: 'work',
    to: 'ada@example.test',
    cc: '',
    subject: 'Re: Quarterly report',
    in_reply_to: 'orig-1@server.test',
    document: { blocks: [paragraph('Thanks for the numbers.'), quoteBlock()], attachments: [] },
  };
}

beforeEach(() => {
  settingsState.settings.quote_below = false;
  useComposerStore.getState().closePane();
});

describe('the composer quote default', () => {
  it('opens with the quote above the reply when the setting says so', () => {
    useComposerStore.getState().openRequest(request(), 'Work');

    const editor = useComposerStore.getState().editor;
    expect(editor.retained).toHaveLength(1);
    expect(editor.retained[0]?.index).toBe(0);
    expect('Quote' in (editor.retained[0]?.block ?? {})).toBe(true);
  });

  it('keeps the quote below the reply on the default setting', () => {
    settingsState.settings.quote_below = true;
    useComposerStore.getState().openRequest(request(), 'Work');

    const editor = useComposerStore.getState().editor;
    expect(editor.retained[0]?.index).toBe(1);
  });
});

describe('the caller-minted composition', () => {
  it('opens a new message under the handed id, the pop-out window discipline', () => {
    useComposerStore.getState().newMessage({ id: 'work', label: 'Work' }, 'compose-77');

    const state = useComposerStore.getState();
    expect(state.open).toBe(true);
    expect(state.composition).toBe('compose-77');
    expect(state.from).toBe('work');
  });

  it('opens a request under the handed id, minting nothing new', () => {
    useComposerStore.getState().openRequest(request(), 'Work', 'compose-78');

    const state = useComposerStore.getState();
    expect(state.composition).toBe('compose-78');
    expect(state.subject).toBe('Re: Quarterly report');
    expect(state.request).toEqual({ kind: 'reply', inReplyTo: 'orig-1@server.test' });
  });
});

// A mailto link's prefill: the five fields the core's parser honours, opened
// as a plain new message from the account the caller resolved, request-free
// so the send rides the plain submit.
describe('a prefilled new message', () => {
  it('seeds the fields, the body document, and nothing pending', () => {
    useComposerStore.getState().newMessage({ id: 'work', label: 'Work' }, undefined, {
      to: 'ada@example.test, grace@example.test',
      cc: 'tim@example.test',
      bcc: '',
      subject: 'Lunch',
      body: 'Shall we say noon?\nAt the usual place.',
    });

    const state = useComposerStore.getState();
    expect(state.open).toBe(true);
    expect(state.from).toBe('work');
    expect(state.fromLabel).toBe('Work');
    expect(state.to).toBe('ada@example.test, grace@example.test');
    expect(state.cc).toBe('tim@example.test');
    expect(state.bcc).toBe('');
    expect(state.subject).toBe('Lunch');
    expect(state.request).toBeNull();
    expect(JSON.stringify(editorToDocument(state.editor))).toBe(
      JSON.stringify(documentFromPlainText('Shall we say noon?\nAt the usual place.')),
    );
  });

  it('opens a blank composition when no prefill crosses', () => {
    useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });

    const state = useComposerStore.getState();
    expect(state.to).toBe('');
    expect(state.subject).toBe('');
    expect(JSON.stringify(editorToDocument(state.editor))).toBe(
      JSON.stringify(documentFromPlainText('')),
    );
  });
});

describe('the inline expectation', () => {
  it('flags the next request for the quick-reply strip until the opening spends it', () => {
    expect(useComposerStore.getState().inlineExpected).toBe(false);

    useComposerStore.getState().expectInlineReply();
    expect(useComposerStore.getState().inlineExpected).toBe(true);

    useComposerStore.getState().openRequest(request(), 'Work');
    expect(useComposerStore.getState().inlineExpected).toBe(false);
  });

  it('a close spends a standing expectation too', () => {
    useComposerStore.getState().expectInlineReply();
    useComposerStore.getState().closePane();
    expect(useComposerStore.getState().inlineExpected).toBe(false);
  });
});

// The composer's attachments: picked files stand as document rows with their
// bytes staged client-side, a removed row drops both, and the send reads the
// standing rows' bytes into blobs. A forward's pre-attached originals ride
// the document the request carried, removable one by one.
describe('the composer attachments', () => {
  /** One picked file with real bytes the send can read. */
  function file(name: string, bytes: Uint8Array<ArrayBuffer>, type = 'application/pdf'): File {
    return new File([bytes], name, { type });
  }

  beforeEach(() => {
    ipc.dispatch.mockReset();
    // The send settles its dispatch, so the seam always answers.
    ipc.dispatch.mockResolvedValue(undefined);
  });

  it('stands each picked file as a removable document row', () => {
    useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });
    useComposerStore.getState().attachFiles([file('report.pdf', new Uint8Array([80, 68, 70]))]);

    const state = useComposerStore.getState();
    expect(state.editor.attachments).toHaveLength(1);
    const row = state.editor.attachments[0]!;
    expect(row.file_name).toBe('report.pdf');
    expect(row.media_type).toBe('application/pdf');
    expect(row.size).toBe(3);
    expect(row.blob).not.toBeNull();
    expect(row.disposition).toBe('Attachment');
  });

  it('drops a removed row and its staged bytes', () => {
    useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });
    useComposerStore.getState().attachFiles([file('report.pdf', new Uint8Array([1]))]);
    const id = useComposerStore.getState().editor.attachments[0]!.id;

    useComposerStore.getState().removeAttachment(id);

    const state = useComposerStore.getState();
    expect(state.editor.attachments).toHaveLength(0);
    expect(Object.keys(state.files)).toHaveLength(0);
  });

  it('carries the standing rows bytes as blobs at send', async () => {
    useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });
    useComposerStore.getState().attachFiles([
      file('report.pdf', new Uint8Array([80, 68, 70])),
      file('notes.txt', new Uint8Array([97])),
    ]);
    const state = useComposerStore.getState();
    useComposerStore.getState().setField('to', 'ada@example.test');
    const handleToKeep = state.editor.attachments[0]!.blob!;
    useComposerStore.getState().removeAttachment(state.editor.attachments[1]!.id);

    useComposerStore.getState().submit();
    await vi.waitFor(() => expect(ipc.dispatch).toHaveBeenCalled());
    const sent = ipc.dispatch.mock.calls.find(
      ([intent]) => JSON.stringify(intent).includes('submit_rich_mail'),
    )?.[0] as { submit_rich_mail: { blobs: { handle: string; bytes: number[] }[] } };

    expect(sent.submit_rich_mail.blobs).toEqual([
      { handle: handleToKeep, bytes: [80, 68, 70] },
    ]);
  });

  it('fails the send with a sentence when a staged file will not read', async () => {
    class UnreadableFile extends File {
      override arrayBuffer(): Promise<ArrayBuffer> {
        return Promise.reject(new Error('the file refused to read'));
      }
    }
    useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });
    useComposerStore
      .getState()
      .attachFiles([new UnreadableFile([new Uint8Array([65])], 'gone.pdf', {
        type: 'application/pdf',
      })]);
    useComposerStore.getState().setField('to', 'ada@example.test');

    useComposerStore.getState().submit();
    await vi.waitFor(() => {
      const state = useComposerStore.getState();
      expect(state.sending).toBe(false);
      expect(state.sendError).toBeTruthy();
    });

    const sent = ipc.dispatch.mock.calls.find(
      ([intent]) => JSON.stringify(intent).includes('submit_rich_mail'),
    );
    expect(sent).toBeUndefined();
  });

  it('opens a forward holding the original attachments, removable one by one', () => {
    const request: ComposeRequestSnapshot = {
      kind: 'forward',
      from: 'work',
      to: '',
      cc: '',
      subject: 'Fwd: Launch',
      document: {
        blocks: [paragraph('Look at this')],
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
      },
    };
    useComposerStore.getState().openRequest(request, 'Work');

    const state = useComposerStore.getState();
    expect(state.editor.attachments).toHaveLength(1);
    expect(state.files).toEqual({});

    useComposerStore.getState().removeAttachment('carry-0');
    expect(useComposerStore.getState().editor.attachments).toHaveLength(0);
  });
});
