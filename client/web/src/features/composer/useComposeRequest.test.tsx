// SPDX-License-Identifier: GPL-3.0-only
// The compose request flow's routing: a landed request opens a pop-out
// composer window on a minted composition id and leaves the inline composer
// and a standing strip composition alone; the quick-reply strip's
// expectation opens it inline instead, prefilled and threaded, consuming the
// engine's slot behind it; and a null application opens nothing.
import { act, cleanup, fireEvent, render, renderHook, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { JSONContent } from '@tiptap/react';

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  draftResume: vi.fn(),
  takeComposeRequest: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  draftResume: ipc.draftResume,
  takeComposeRequest: ipc.takeComposeRequest,
}));

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: true,
}));

const windows = vi.hoisted(() => ({ openComposerWindow: vi.fn() }));

vi.mock('./composeWindow', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openComposerWindow: windows.openComposerWindow,
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
import { documentToEditor, editorToDocument } from './documentWire';
import { useComposeRequestHydration } from './useComposeRequest';

const UUID =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

const plainRun = (text: string): string =>
  `{"Text":{"text":"${text}","bold":false,"italic":false,"underline":false,"font_size":null,"color":null,"highlight":null,"link":null}}`;

const paragraphBlock = (text: string): string =>
  `{"Paragraph":{"content":[${plainRun(text)}]}}`;

/** The quote block the core's reply request carries, the pinned wire literal. */
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

/** A reply request for the fixture original, overridable per test. */
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

/** A forward request: no recipients and no threading header. */
function forwardRequest(): ComposeRequestSnapshot {
  return replyRequest({
    kind: 'forward',
    to: '',
    subject: 'Fwd: Quarterly report',
    in_reply_to: undefined,
  });
}

/** Lands a request the way the surface pull does. */
function land(request: ComposeRequestSnapshot): void {
  act(() => {
    useComposeRequestStore.getState().applySnapshot(request);
  });
}

function resetStores(): void {
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work' }] });
  useDraftStore.setState({ snapshot: null });
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
  useComposeRequestStore.setState({ snapshot: null });
}

/** Opens a composition the way the New message action does. */
function newMessage(): string {
  const account = useAccountStore.getState().accounts[0];
  if (account === undefined) {
    throw new Error('the request test needs an account');
  }
  act(() => {
    useComposerStore.getState().newMessage({ id: account.accountId, label: account.displayName });
  });
  const composition = useComposerStore.getState().composition;
  expect(composition).toMatch(UUID);
  return composition as string;
}

/** Lands a request routed to the inline composer, the strip's path. */
function landInline(request: ComposeRequestSnapshot): void {
  act(() => {
    useComposerStore.getState().expectInlineReply();
  });
  land(request);
}

beforeEach(() => {
  vi.useFakeTimers();
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.draftResume.mockReset();
  ipc.takeComposeRequest.mockReset();
  ipc.takeComposeRequest.mockResolvedValue(null);
  windows.openComposerWindow.mockReset();
  windows.openComposerWindow.mockResolvedValue(undefined);
  resetStores();
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe('the pop-out route', () => {
  it('opens a window on a minted composition, leaving the inline composer shut', () => {
    renderHook(() => useComposeRequestHydration());
    land(replyRequest());

    expect(windows.openComposerWindow).toHaveBeenCalledTimes(1);
    const [composition, title] = windows.openComposerWindow.mock.calls[0] ?? [];
    expect(composition).toMatch(UUID);
    expect(title).toBe('Re: Quarterly report');
    expect(useComposerStore.getState().open).toBe(false);
    // The request is the window's now; the main window holds no copy.
    expect(useComposeRequestStore.getState().snapshot).toBeNull();
  });

  it('routes each landed request to its own window', () => {
    renderHook(() => useComposeRequestHydration());
    land(replyRequest());
    land(forwardRequest());

    expect(windows.openComposerWindow).toHaveBeenCalledTimes(2);
    const [first] = windows.openComposerWindow.mock.calls[0] ?? [];
    const [second, secondTitle] = windows.openComposerWindow.mock.calls[1] ?? [];
    expect(second).not.toBe(first);
    expect(secondTitle).toBe('Fwd: Quarterly report');
  });

  it('leaves a standing inline composition untouched', () => {
    renderHook(() => useComposeRequestHydration());
    const standing = newMessage();
    act(() => {
      const store = useComposerStore.getState();
      store.setField('to', 'you@test.local');
      store.setField('subject', 'Later');
    });

    land(replyRequest());

    // The pop-out takes nothing from the strip: no final save crosses, and
    // the standing composition keeps its words.
    expect(ipc.dispatch).not.toHaveBeenCalled();
    const state = useComposerStore.getState();
    expect(state.open).toBe(true);
    expect(state.composition).toBe(standing);
    expect(state.subject).toBe('Later');
  });

  it('opens nothing when the pull clears the request', () => {
    renderHook(() => useComposeRequestHydration());

    act(() => {
      useComposeRequestStore.getState().applySnapshot(null);
    });

    expect(windows.openComposerWindow).not.toHaveBeenCalled();
    expect(useComposerStore.getState().open).toBe(false);
  });
});

describe('the inline route', () => {
  it('opens the composer prefilled and threaded, consuming the engine slot', () => {
    renderHook(() => useComposeRequestHydration());
    landInline(replyRequest());

    const state = useComposerStore.getState();
    expect(state.open).toBe(true);
    expect(state.composition).toMatch(UUID);
    expect(state.from).toBe('work');
    expect(state.fromLabel).toBe('Work');
    expect(state.to).toBe('ada@example.test');
    expect(state.cc).toBe('');
    expect(state.bcc).toBe('');
    expect(state.subject).toBe('Re: Quarterly report');
    expect(state.request).toEqual({ kind: 'reply', inReplyTo: 'orig-1@server.test' });
    expect(ipc.takeComposeRequest).toHaveBeenCalledTimes(1);
    expect(windows.openComposerWindow).not.toHaveBeenCalled();
    expect(useComposerStore.getState().inlineExpected).toBe(false);
  });

  it('holds the quoted original as retained content that round-trips', () => {
    renderHook(() => useComposeRequestHydration());
    landInline(replyRequest());

    const editor = useComposerStore.getState().editor;
    // The trimmed editor cannot show a quote: it rides beneath the words,
    // spliced back wherever it stood on every save and send.
    expect(editor.content).toEqual([]);
    expect(editor.retained).toEqual([{ index: 0, block: quoteBlock() }]);
    expect(editorToDocument(editor)).toEqual(replyRequest().document);
  });

  it('shows the prefill in the fields, subject prefix and all', () => {
    renderHook(() => useComposeRequestHydration());
    render(<ComposerPane />);
    landInline(replyRequest({ kind: 'reply_all', cc: 'bob@example.test' }));

    expect((screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement).value).toBe(
      'ada@example.test',
    );
    expect((screen.getByRole('textbox', { name: 'Cc' }) as HTMLInputElement).value).toBe(
      'bob@example.test',
    );
    expect((screen.getByRole('textbox', { name: 'Subject' }) as HTMLInputElement).value).toBe(
      'Re: Quarterly report',
    );
    expect(screen.getByText('Work')).toBeTruthy();
  });

  it('labels the from with the account id when no account is known', () => {
    renderHook(() => useComposeRequestHydration());
    landInline(replyRequest({ from: 'ghost' }));

    expect(useComposerStore.getState().fromLabel).toBe('ghost');
  });

  it('saves the reply draft over the idle with its quote, then still threads the send', async () => {
    renderHook(() => useComposeRequestHydration());
    render(<ComposerPane />);
    landInline(replyRequest());
    const composition = useComposerStore.getState().composition as string;

    act(() => {
      useComposerStore.getState().setEditorContent([
        { type: 'paragraph', content: [{ type: 'text', text: 'Agreed' }] },
      ]);
    });
    act(() => vi.advanceTimersByTime(30_000));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      `{"drafts":{"save":{"composition":"${composition}","from":"work","to":"ada@example.test","cc":"","bcc":"","subject":"Re: Quarterly report",` +
        `"document":{"blocks":[${QUOTE_JSON},${paragraphBlock('Agreed')}],"attachments":[]},"then_close":false}}}`,
    );

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    });
    // The submit reads the staged bytes before it dispatches, so the intent
    // crosses one microtask after the click.
    await act(async () => {
      await Promise.resolve();
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"submit_rich_reply":{"from":"work","to":"ada@example.test","cc":"","in_reply_to":"orig-1@server.test","subject":"Re: Quarterly report",' +
        `"document":{"blocks":[${QUOTE_JSON},${paragraphBlock('Agreed')}],"attachments":[]},"blobs":[]}}`,
    );
  });

  it('closes a composition standing in the way through the Close discipline first', () => {
    renderHook(() => useComposeRequestHydration());
    const standing = newMessage();
    act(() => {
      const store = useComposerStore.getState();
      store.setField('to', 'you@test.local');
      store.setField('subject', 'Later');
      store.setEditorContent([
        { type: 'paragraph', content: [{ type: 'text', text: 'Half written' }] },
      ]);
    });

    landInline(replyRequest());

    // The standing composition's words went out as a final save, kept in
    // Drafts, before the request took the strip.
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      `{"drafts":{"save":{"composition":"${standing}","from":"work","to":"you@test.local","cc":"","bcc":"","subject":"Later",` +
        `"document":{"blocks":[${paragraphBlock('Half written')}],"attachments":[]},"then_close":true}}}`,
    );
    const state = useComposerStore.getState();
    expect(state.composition).not.toBe(standing);
    expect(state.subject).toBe('Re: Quarterly report');
  });
});
