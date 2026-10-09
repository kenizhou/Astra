// SPDX-License-Identifier: GPL-3.0-only
// The composer pane: a new message mints its composition and opens empty,
// the idle autosave dispatches the save intent with the exact payload and
// stays quiet while nothing changed, the badge follows the status surface
// for its own composition only, discard and close cross with exact payloads
// and shut the pane (clearing any pending compose request behind them), and
// the app start resume hydrates the stored draft into the composer once.
import { act, cleanup, fireEvent, render, renderHook, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { JSONContent } from '@tiptap/react';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn(), draftResume: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  draftResume: ipc.draftResume,
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

import type { ComposerDocument, ComposeRequestSnapshot, ResumeView } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useComposeRequestStore } from '@/stores/composeRequestStore';
import { useDraftStore } from '@/stores/draftStore';

import { ComposerPane, useDraftResumeAtStart } from './Composer';
import { useComposerStore } from './composerStore';
import { documentToEditor } from './documentWire';

const UUID =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

const plainRun = (text: string): string =>
  `{"Text":{"text":"${text}","bold":false,"italic":false,"underline":false,"font_size":null,"color":null,"highlight":null,"link":null}}`;

/** A pending request, standing in the store while the composer is open. */
const pendingRequest: ComposeRequestSnapshot = {
  kind: 'reply',
  from: 'work',
  to: 'ada@example.test',
  cc: '',
  subject: 'Re: Quarterly report',
  in_reply_to: 'orig-1@server.test',
  document: { blocks: [], attachments: [] },
};

/** The same request as a forward: no recipients, no threading header. */
function forwardRequest(): ComposeRequestSnapshot {
  return {
    ...pendingRequest,
    kind: 'forward',
    to: '',
    subject: 'Fwd: Quarterly report',
    in_reply_to: undefined,
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
    editor: documentToEditor(emptyDocument()),
    savedFingerprint: null,
    request: null,
  });
  useComposeRequestStore.setState({ snapshot: null });
}

function emptyDocument(): ComposerDocument {
  return { blocks: [{ Paragraph: { content: [] } }], attachments: [] };
}

/** Opens a composition the way the New message action does. */
function newMessage(): string {
  const account = useAccountStore.getState().accounts[0];
  if (account === undefined) {
    throw new Error('the new message test needs an account');
  }
  act(() => {
    useComposerStore.getState().newMessage({ id: account.accountId, label: account.displayName });
  });
  const composition = useComposerStore.getState().composition;
  expect(composition).toMatch(UUID);
  return composition as string;
}

beforeEach(() => {
  vi.useFakeTimers();
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.draftResume.mockReset();
  resetStores();
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe('the new message mint', () => {
  it('opens the pane empty with the account as its read only from', () => {
    newMessage();

    render(<ComposerPane />);

    expect(screen.getByRole('region', { name: 'Composer' })).toBeTruthy();
    expect(screen.getByText('Work')).toBeTruthy();
    expect((screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement).value).toBe('');
    expect((screen.getByRole('textbox', { name: 'Subject' }) as HTMLInputElement).value).toBe('');
  });

  it('draws nothing when no composition is open', () => {
    render(<ComposerPane />);

    expect(screen.queryByRole('region', { name: 'Composer' })).toBeNull();
  });
});

describe('the autosave handover', () => {
  it('dispatches the save intent after idle with the exact payload', () => {
    const composition = newMessage();
    render(<ComposerPane />);

    act(() => {
      const store = useComposerStore.getState();
      store.setField('to', 'you@test.local');
      store.setField('subject', 'Launch');
      store.setEditorContent([
        { type: 'paragraph', content: [{ type: 'text', text: 'Half written' }] },
      ]);
    });
    act(() => vi.advanceTimersByTime(30_000));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      `{"drafts":{"save":{"composition":"${composition}","from":"work","to":"you@test.local","cc":"","bcc":"","subject":"Launch",` +
        `"document":{"blocks":[{"Paragraph":{"content":[${plainRun('Half written')}]}}],"attachments":[]},"then_close":false}}}`,
    );
  });

  it('stays quiet while nothing changed and fires again after a new edit', () => {
    newMessage();
    render(<ComposerPane />);

    act(() => {
      useComposerStore.getState().setField('subject', 'One');
    });
    act(() => vi.advanceTimersByTime(30_000));
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);

    act(() => vi.advanceTimersByTime(30_000));
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);

    act(() => {
      useComposerStore.getState().setField('subject', 'Two');
    });
    act(() => vi.advanceTimersByTime(30_000));
    expect(ipc.dispatch).toHaveBeenCalledTimes(2);
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toContain('"subject":"Two"');
  });

  it('never counts an unedited resume as a change', () => {
    const view: ResumeView = {
      account: 'work',
      to: 'you@test.local',
      cc: '',
      bcc: '',
      subject: 'Later',
      body_text: 'Half written',
    };
    act(() => {
      useComposerStore
        .getState()
        .hydrate(view, { id: 'work', label: 'Work' }, 'compose-9');
    });
    render(<ComposerPane />);

    act(() => vi.advanceTimersByTime(120_000));

    expect(ipc.dispatch).not.toHaveBeenCalled();
  });
});

describe('the save badge', () => {
  it('follows the status surface for its own composition only', () => {
    const composition = newMessage();
    render(<ComposerPane />);

    act(() => {
      useDraftStore.setState({
        snapshot: { composition: 'other-1', state: 'saving', saved_at: '' },
      });
    });
    expect(screen.queryByText('Saving draft')).toBeNull();

    act(() => {
      useDraftStore.setState({
        snapshot: { composition, state: 'saving', saved_at: '' },
      });
    });
    expect(screen.getByText('Saving draft')).toBeTruthy();

    act(() => {
      useDraftStore.setState({
        snapshot: {
          composition,
          state: 'saved',
          saved_at: '2026-10-09T08:30:00Z',
        },
      });
    });
    expect(screen.getByText('Saved 08:30 UTC')).toBeTruthy();
  });
});

describe('leaving the composer', () => {
  it('discards the stored draft and closes the pane locally', () => {
    const composition = newMessage();
    render(<ComposerPane />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Discard' }));
    });

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      `{"drafts":{"discard":{"composition":"${composition}"}}}`,
    );
    expect(useComposerStore.getState().open).toBe(false);
    expect(screen.queryByRole('region', { name: 'Composer' })).toBeNull();
  });

  it('closes through a final save with then_close and closes the pane locally', () => {
    const composition = newMessage();
    render(<ComposerPane />);

    act(() => {
      const store = useComposerStore.getState();
      store.setField('to', 'you@test.local');
      store.setField('subject', 'Later');
      store.setEditorContent([
        { type: 'paragraph', content: [{ type: 'text', text: 'Final window' }] },
      ]);
    });
    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Close composer' }));
    });

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      `{"drafts":{"save":{"composition":"${composition}","from":"work","to":"you@test.local","cc":"","bcc":"","subject":"Later",` +
        `"document":{"blocks":[{"Paragraph":{"content":[${plainRun('Final window')}]}}],"attachments":[]},"then_close":true}}}`,
    );
    expect(useComposerStore.getState().open).toBe(false);
    expect(screen.queryByRole('region', { name: 'Composer' })).toBeNull();
  });

  it('clears a pending compose request behind every close path', () => {
    newMessage();
    render(<ComposerPane />);
    act(() => {
      useComposeRequestStore.getState().applySnapshot(pendingRequest);
    });

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Discard' }));
    });
    expect(useComposeRequestStore.getState().snapshot).toBeNull();

    act(() => {
      useComposeRequestStore.getState().applySnapshot(pendingRequest);
      useComposerStore.getState().closePane();
    });
    expect(useComposeRequestStore.getState().snapshot).toBeNull();
  });
});

describe('the fields the request kind scopes', () => {
  it('offers the cc a reply carries and hides the bcc it cannot', () => {
    act(() => {
      useComposerStore.getState().openRequest(pendingRequest, 'Work');
    });
    render(<ComposerPane />);

    expect(screen.getByRole('button', { name: 'Show Cc' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Show Bcc' })).toBeNull();
    expect(screen.queryByRole('textbox', { name: 'Bcc' })).toBeNull();
  });

  it('shows the cc a reply-all prefilled, still without a bcc', () => {
    act(() => {
      useComposerStore.getState().openRequest(
        { ...pendingRequest, kind: 'reply_all', cc: 'bob@example.test' },
        'Work',
      );
    });
    render(<ComposerPane />);

    expect((screen.getByRole('textbox', { name: 'Cc' }) as HTMLInputElement).value).toBe(
      'bob@example.test',
    );
    expect(screen.getByRole('button', { name: 'Hide Cc' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Show Bcc' })).toBeNull();
  });

  it('hides the cc and bcc a forward cannot carry', () => {
    act(() => {
      useComposerStore.getState().openRequest(forwardRequest(), 'Work');
    });
    render(<ComposerPane />);

    expect(screen.queryByRole('button', { name: 'Show Cc' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Show Bcc' })).toBeNull();
    expect(screen.queryByRole('textbox', { name: 'Cc' })).toBeNull();
    expect(screen.queryByRole('textbox', { name: 'Bcc' })).toBeNull();
  });

  it('offers every field on a plain composition', () => {
    newMessage();
    render(<ComposerPane />);

    expect(screen.getByRole('button', { name: 'Show Cc' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Show Bcc' })).toBeTruthy();
  });
});

describe('the app start resume', () => {
  const view: ResumeView = {
    account: 'work',
    to: 'you@test.local',
    cc: 'cc@test.local',
    bcc: 'quiet@test.local',
    subject: 'Later',
    body_text: 'Half written\nSecond line',
  };

  it('hydrates the stored draft into the composer once', async () => {
    ipc.draftResume.mockResolvedValue(view);
    renderHook(() => useDraftResumeAtStart());

    await act(async () => {
      await Promise.resolve();
    });

    expect(ipc.draftResume).toHaveBeenCalledTimes(1);
    const [account, composition] = ipc.draftResume.mock.calls[0] ?? [];
    expect(account).toBe('work');
    expect(composition).toMatch(UUID);

    const store = useComposerStore.getState();
    expect(store.open).toBe(true);
    expect(store.composition).toBe(composition);
    expect(store.from).toBe('work');
    expect(store.to).toBe('you@test.local');
    expect(store.cc).toBe('cc@test.local');
    expect(store.bcc).toBe('quiet@test.local');
    expect(store.subject).toBe('Later');
    expect(JSON.stringify(store.editor.content)).toBe(
      `[{"type":"paragraph","content":[{"type":"text","text":"Half written"}]},{"type":"paragraph","content":[{"type":"text","text":"Second line"}]}]`,
    );
  });

  it('tries exactly once across remounts and leaves the pane shut with no draft', async () => {
    ipc.draftResume.mockResolvedValue(null);
    const { unmount } = renderHook(() => useDraftResumeAtStart());
    unmount();
    const second = renderHook(() => useDraftResumeAtStart());
    await act(async () => {
      await Promise.resolve();
    });
    second.unmount();

    expect(ipc.draftResume).toHaveBeenCalledTimes(1);
    expect(useComposerStore.getState().open).toBe(false);
  });

  it('shows the cc and bcc fields a resumed draft carries, hidden on a fresh one', async () => {
    ipc.draftResume.mockResolvedValue(view);
    render(<ComposerPane />);
    renderHook(() => useDraftResumeAtStart());

    await act(async () => {
      await Promise.resolve();
    });

    expect((screen.getByRole('textbox', { name: 'Cc' }) as HTMLInputElement).value).toBe(
      'cc@test.local',
    );
    expect((screen.getByRole('textbox', { name: 'Bcc' }) as HTMLInputElement).value).toBe(
      'quiet@test.local',
    );

    act(() => {
      useComposerStore.getState().closePane();
      newMessage();
    });

    expect(screen.queryByRole('textbox', { name: 'Cc' })).toBeNull();
    expect(screen.queryByRole('textbox', { name: 'Bcc' })).toBeNull();
  });

  it('logs a failed resume pull and leaves the pane shut', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    ipc.draftResume.mockRejectedValue(new Error('the store is unreachable'));

    renderHook(() => useDraftResumeAtStart());
    await act(async () => {
      await Promise.resolve();
    });

    expect(ipc.draftResume).toHaveBeenCalledTimes(1);
    expect(error).toHaveBeenCalledTimes(1);
    expect(useComposerStore.getState().open).toBe(false);
    error.mockRestore();
  });

  it('spends a standing request when the resume opens, so the send rides the plain submit', async () => {
    ipc.draftResume.mockResolvedValue(view);
    act(() => {
      useComposerStore.getState().openRequest(pendingRequest, 'Work');
    });
    useComposeRequestStore.getState().applySnapshot(pendingRequest);
    render(<ComposerPane />);
    renderHook(() => useDraftResumeAtStart());

    await act(async () => {
      await Promise.resolve();
    });

    // The resumed draft's fields stand in the pane and the request is spent.
    expect((screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement).value).toBe(
      'you@test.local',
    );
    expect(useComposerStore.getState().request).toBeNull();
    expect(useComposeRequestStore.getState().snapshot).toBeNull();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    });

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"submit_rich_mail":{"from":"work","to":"you@test.local","cc":"cc@test.local","bcc":"quiet@test.local","subject":"Later",' +
        `"document":{"blocks":[{"Paragraph":{"content":[${plainRun('Half written')}]}},{"Paragraph":{"content":[${plainRun(
          'Second line',
        )}]}}],"attachments":[]},"blobs":[]}}`,
    );
  });
});
