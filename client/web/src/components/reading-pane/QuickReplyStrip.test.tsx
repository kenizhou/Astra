// SPDX-License-Identifier: GPL-3.0-only
// The quick-reply strip: docked at the reading pane's bottom, kylins'
// InlineReply idiom. Collapsed it is a one-line Reply affordance, shown only
// while a message is open; the click flags the next compose request for the
// inline route and dispatches the reply, so the core's prefill and threading
// land in the docked composer rather than a pop-out. Expanded it is the
// shared composer pane, sending through the same submit path; the pane
// leaving (a send, a discard) collapses the strip back to the affordance.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { JSONContent } from '@tiptap/react';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn(), takeComposeRequest: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  takeComposeRequest: ipc.takeComposeRequest,
}));

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: true,
}));

vi.mock('@/features/composer/Editor', () => ({
  Editor: (_props: { initialContent: JSONContent[]; onChange: (doc: JSONContent) => void }) => {
    return <div data-testid="editor-stub" />;
  },
  insertAtSelection: () => undefined,
}));

import type { ComposeRequestSnapshot, ReadingSnapshot } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useComposeRequestStore } from '@/stores/composeRequestStore';
import { useDraftStore } from '@/stores/draftStore';
import { useReadingStore } from '@/stores/readingStore';

import { useComposerStore } from '@/features/composer/composerStore';
import { documentToEditor } from '@/features/composer/documentWire';
import { QuickReplyStrip } from './QuickReplyStrip';

/** The open message the strip answers. */
function readingSnapshot(): ReadingSnapshot {
  return {
    message: { account: 'work', key: 'm1' },
    subject: 'Quarterly report',
    from_name: 'Ada Lovelace',
    from_address: 'ada@example.test',
    sent_at: '2026-10-09T08:30:00Z',
    plain: 'The numbers are in.',
    remote_image_count: 0,
  };
}

/** The reply prefill the core publishes for the strip's dispatch. */
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
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work' }] });
  useReadingStore.setState({ snapshot: null, showRemoteImages: false });
  useDraftStore.setState({ snapshot: null });
  useComposeRequestStore.setState({ snapshot: null });
  useComposerStore.setState({
    open: false,
    inlineExpected: false,
    resumeAttempted: true,
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
}

beforeEach(() => {
  vi.useFakeTimers();
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.takeComposeRequest.mockReset();
  ipc.takeComposeRequest.mockResolvedValue(null);
  resetStores();
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe('the collapsed strip', () => {
  it('shows the Reply affordance while a message is open', () => {
    useReadingStore.setState({ snapshot: readingSnapshot() });
    render(<QuickReplyStrip />);

    expect(screen.getByRole('button', { name: 'Reply' })).toBeTruthy();
  });

  it('draws nothing with no message open', () => {
    render(<QuickReplyStrip />);
    expect(screen.queryByRole('button', { name: 'Reply' })).toBeNull();
  });

  it('flags the next request inline and dispatches the reply on the open message', () => {
    useReadingStore.setState({ snapshot: readingSnapshot() });
    render(<QuickReplyStrip />);

    fireEvent.click(screen.getByRole('button', { name: 'Reply' }));

    expect(useComposerStore.getState().inlineExpected).toBe(true);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"compose":{"reply":{"message":{"account":"work","key":"m1"}}}}',
    );
  });
});

describe('the expanded strip', () => {
  /** Expands the strip the way its own click plus the landed request does. */
  function expandStrip(): void {
    useReadingStore.setState({ snapshot: readingSnapshot() });
    act(() => {
      useComposerStore.getState().expectInlineReply();
    });
    act(() => {
      useComposeRequestStore.getState().applySnapshot(replyRequest());
    });
    // The hydration hook is not mounted in this test; the route's inline arm
    // runs directly here, as the strip asked.
    act(() => {
      useComposerStore.getState().openRequest(replyRequest(), 'Work');
    });
  }

  it('docks the composer prefilled in place of the affordance', () => {
    render(<QuickReplyStrip />);
    expandStrip();

    expect(screen.queryByRole('button', { name: 'Reply' })).toBeNull();
    expect(screen.getByRole('region', { name: 'Composer' })).toBeTruthy();
    expect((screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement).value).toBe(
      'ada@example.test',
    );
    expect((screen.getByRole('textbox', { name: 'Subject' }) as HTMLInputElement).value).toBe(
      'Re: Quarterly report',
    );
    // The window's Close button does not stand in the strip.
    expect(screen.queryByRole('button', { name: 'Close composer' })).toBeNull();
  });

  it('sends through the same submit path and collapses', async () => {
    render(<QuickReplyStrip />);
    expandStrip();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
      await Promise.resolve();
    });

    const submit = JSON.stringify(ipc.dispatch.mock.calls[0]?.[0]);
    expect(submit).toContain('"submit_rich_reply"');
    expect(submit).toContain('"in_reply_to":"orig-1@server.test"');
    expect(useComposerStore.getState().open).toBe(false);
    expect(screen.getByRole('button', { name: 'Reply' })).toBeTruthy();
  });

  it('collapses on discard', () => {
    render(<QuickReplyStrip />);
    expandStrip();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Discard' }));
    });

    expect(useComposerStore.getState().open).toBe(false);
    expect(screen.getByRole('button', { name: 'Reply' })).toBeTruthy();
  });
});
