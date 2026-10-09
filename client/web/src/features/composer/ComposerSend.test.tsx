// SPDX-License-Identifier: GPL-3.0-only
// The composer's Send button: it stays disabled until a recipient stands in
// the To field and while a submit is in flight, it submits the CURRENT
// document and then clears the stored draft and closes, a refused submit
// keeps the pane open with its words and says why, and neither the error
// line nor an in-flight submit outlives its own composition. A composition a
// compose request opened submits through its own threaded intent: the reply
// under its original with in_reply_to, the forward answering no one.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { JSONContent } from '@tiptap/react';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: true,
}));

vi.mock('./Editor', () => ({
  Editor: (_props: { initialContent: JSONContent[]; onChange: (doc: JSONContent) => void }) => {
    return <div data-testid="editor-stub" />;
  },
}));

import type { Block, ComposeRequestSnapshot } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useComposeRequestStore } from '@/stores/composeRequestStore';
import { useDraftStore } from '@/stores/draftStore';

import { ComposerPane } from './Composer';
import { useComposerStore } from './composerStore';
import { documentToEditor } from './documentWire';

const UUID =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

const plainRun = (text: string): string =>
  `{"Text":{"text":"${text}","bold":false,"italic":false,"underline":false,"font_size":null,"color":null,"highlight":null,"link":null}}`;

/** The quote block a compose request carries, the pinned wire literal. */
function quoteBlock(): Block {
  return {
    Quote: {
      style: 'Indented',
      attribution: {
        line: 'On 9 Oct 2026 at 08:00, Ada Lovelace <ada@example.test> wrote:',
        headers: [
          { label: 'From', value: 'Ada Lovelace <ada@example.test>' },
          { label: 'Sent', value: '9 Oct 2026 at 08:00' },
          { label: 'To', value: 'me@work.local, Bob <bob@example.test>' },
          { label: 'Subject', value: 'Quarterly report' },
        ],
      },
      body_html: '<div style="white-space: pre-wrap;">The numbers are in.</div>',
      body_plain: 'The numbers are in.',
    },
  };
}

/** The quote as the wire spells it, for byte-exact payload assertions. */
const QUOTE_JSON =
  '{"Quote":{"style":"Indented","attribution":{"line":"On 9 Oct 2026 at 08:00, Ada Lovelace <ada@example.test> wrote:",' +
  '"headers":[{"label":"From","value":"Ada Lovelace <ada@example.test>"},{"label":"Sent","value":"9 Oct 2026 at 08:00"},' +
  '{"label":"To","value":"me@work.local, Bob <bob@example.test>"},{"label":"Subject","value":"Quarterly report"}]},' +
  '"body_html":"<div style=\\"white-space: pre-wrap;\\">The numbers are in.</div>","body_plain":"The numbers are in."}}';

/** A reply request, overridable per test. */
function replyRequest(over: Partial<ComposeRequestSnapshot> = {}): ComposeRequestSnapshot {
  return {
    kind: 'reply',
    from: 'work',
    to: 'ada@example.test',
    cc: '',
    subject: 'Re: Quarterly report',
    in_reply_to: 'orig-1@server.test',
    document: { blocks: [quoteBlock()], attachments: [] },
    ...over,
  };
}

function resetStores(): void {
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work' }] });
  useDraftStore.setState({ snapshot: null });
  useComposerStore.setState({
    open: false,
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
  useComposeRequestStore.setState({ snapshot: null });
}

/** Opens a composition the way the New message action does. */
function newMessage(): string {
  const account = useAccountStore.getState().accounts[0];
  if (account === undefined) {
    throw new Error('the send test needs an account');
  }
  act(() => {
    useComposerStore.getState().newMessage({ id: account.accountId, label: account.displayName });
  });
  const composition = useComposerStore.getState().composition;
  expect(composition).toMatch(UUID);
  return composition as string;
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  resetStores();
});

afterEach(cleanup);

describe('the send button', () => {
  it('stays disabled until a recipient stands in the to field', () => {
    newMessage();
    render(<ComposerPane />);

    const send = screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement;
    expect(send.disabled).toBe(true);

    act(() => {
      useComposerStore.getState().setField('to', '   ');
    });
    expect(send.disabled).toBe(true);

    act(() => {
      useComposerStore.getState().setField('to', 'you@test.local');
    });
    expect(send.disabled).toBe(false);
  });

  it('submits the current document, then clears the stored draft and closes', async () => {
    const composition = newMessage();
    render(<ComposerPane />);

    act(() => {
      const store = useComposerStore.getState();
      store.setField('to', 'you@test.local');
      store.setField('subject', 'Launch');
      store.setEditorContent([
        { type: 'paragraph', content: [{ type: 'text', text: 'Sent words' }] },
      ]);
    });
    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    });

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"submit_rich_mail":{"from":"work","to":"you@test.local","cc":"","bcc":"","subject":"Launch",' +
        `"document":{"blocks":[{"Paragraph":{"content":[${plainRun('Sent words')}]}}],"attachments":[]},"blobs":[]}}`,
    );
    // The submit carries no composition id, so the stored draft goes too.
    await act(async () => {
      await Promise.resolve();
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      `{"drafts":{"discard":{"composition":"${composition}"}}}`,
    );
    expect(useComposerStore.getState().open).toBe(false);
    expect(screen.queryByRole('region', { name: 'Composer' })).toBeNull();
  });

  it('keeps the pane open with its words and shows the error when the submit rejects', async () => {
    newMessage();
    render(<ComposerPane />);
    act(() => {
      const store = useComposerStore.getState();
      store.setField('to', 'you@test.local');
      store.setField('subject', 'Launch');
      store.setEditorContent([
        { type: 'paragraph', content: [{ type: 'text', text: 'Kept words' }] },
      ]);
    });

    ipc.dispatch.mockRejectedValueOnce(new Error('the host is unreachable'));
    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    });
    await act(async () => {
      await Promise.resolve();
    });

    expect(screen.getByRole('alert').textContent).toBe('the host is unreachable');
    expect(useComposerStore.getState().open).toBe(true);
    expect(useComposerStore.getState().to).toBe('you@test.local');
    expect(useComposerStore.getState().subject).toBe('Launch');
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    const send = screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement;
    expect(send.disabled).toBe(false);
  });

  it('never sends twice from one gesture while the submit is in flight', async () => {
    let settle: (() => void) | undefined;
    ipc.dispatch.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          settle = resolve;
        }),
    );
    newMessage();
    render(<ComposerPane />);
    act(() => {
      useComposerStore.getState().setField('to', 'you@test.local');
    });

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    });
    const send = screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement;
    expect(send.disabled).toBe(true);

    act(() => {
      fireEvent.click(send);
    });
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);

    await act(async () => {
      settle?.();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(ipc.dispatch).toHaveBeenCalledTimes(2);
    expect(useComposerStore.getState().open).toBe(false);
  });

  it('a failed send leaves no error under the next composition', async () => {
    newMessage();
    render(<ComposerPane />);
    act(() => {
      useComposerStore.getState().setField('to', 'you@test.local');
    });

    ipc.dispatch.mockRejectedValueOnce(new Error('the host is unreachable'));
    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    });
    await act(async () => {
      await Promise.resolve();
    });
    expect(screen.getByRole('alert')).toBeTruthy();

    act(() => {
      useComposerStore.getState().closePane();
      newMessage();
    });
    act(() => {
      useComposerStore.getState().setField('to', 'them@test.local');
    });

    expect(screen.queryByRole('alert')).toBeNull();
    const send = screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement;
    expect(send.disabled).toBe(false);
  });

  it('a send left in flight when the pane moved on settles against nothing', async () => {
    let settle: (() => void) | undefined;
    ipc.dispatch.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          settle = resolve;
        }),
    );
    newMessage();
    render(<ComposerPane />);
    act(() => {
      useComposerStore.getState().setField('to', 'you@test.local');
    });
    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    });

    // The pane closes mid-flight and a fresh composition opens.
    act(() => {
      useComposerStore.getState().closePane();
      newMessage();
    });
    act(() => {
      useComposerStore.getState().setField('to', 'them@test.local');
    });
    const send = screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement;
    expect(send.disabled).toBe(false);

    await act(async () => {
      settle?.();
      await Promise.resolve();
      await Promise.resolve();
    });

    // The old composition's settlement may neither discard nor close.
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(useComposerStore.getState().open).toBe(true);
    expect(screen.queryByRole('alert')).toBeNull();
    expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(
      false,
    );
  });
});

describe('the threaded sends', () => {
  /** Opens a composition the way a landed compose request does. */
  function openRequest(request: ComposeRequestSnapshot): void {
    act(() => {
      useComposerStore.getState().openRequest(request, 'Work');
    });
  }

  it('submits a reply under its original, then clears the draft, the request, and closes', async () => {
    const request = replyRequest();
    useComposeRequestStore.getState().applySnapshot(request);
    openRequest(request);
    const composition = useComposerStore.getState().composition as string;
    render(<ComposerPane />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    });

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"submit_rich_reply":{"from":"work","to":"ada@example.test","cc":"","in_reply_to":"orig-1@server.test","subject":"Re: Quarterly report",' +
        `"document":{"blocks":[${QUOTE_JSON}],"attachments":[]},"blobs":[]}}`,
    );
    await act(async () => {
      await Promise.resolve();
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      `{"drafts":{"discard":{"composition":"${composition}"}}}`,
    );
    expect(useComposerStore.getState().open).toBe(false);
    expect(useComposeRequestStore.getState().snapshot).toBeNull();
  });

  it('submits a forward answering no one once the user names a recipient', () => {
    openRequest(
      replyRequest({ kind: 'forward', to: '', subject: 'Fwd: Quarterly report', in_reply_to: undefined }),
    );
    render(<ComposerPane />);

    const send = screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement;
    expect(send.disabled).toBe(true);

    act(() => {
      useComposerStore.getState().setField('to', 'bob@example.test');
    });
    expect(send.disabled).toBe(false);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    });

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"submit_rich_forward":{"from":"work","to":"bob@example.test","subject":"Fwd: Quarterly report",' +
        `"document":{"blocks":[${QUOTE_JSON}],"attachments":[]},"blobs":[]}}`,
    );
  });

  it('keeps the pending request when a reply submit refuses, so a retry still threads', async () => {
    openRequest(replyRequest());
    render(<ComposerPane />);

    ipc.dispatch.mockRejectedValueOnce(new Error('the host is unreachable'));
    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    });
    await act(async () => {
      await Promise.resolve();
    });

    expect(useComposerStore.getState().open).toBe(true);
    expect(useComposerStore.getState().request).toEqual({
      kind: 'reply',
      inReplyTo: 'orig-1@server.test',
    });

    ipc.dispatch.mockClear();
    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toContain('"submit_rich_reply"');
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toContain('"in_reply_to":"orig-1@server.test"');
  });
});
