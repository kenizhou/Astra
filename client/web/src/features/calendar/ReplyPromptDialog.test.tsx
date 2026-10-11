// SPDX-License-Identifier: GPL-3.0-only
// The reply-delivery prompt: the answer that landed but whose telling did
// not, with the summary, the organiser, the plain-language sentence, the note
// the reply carries, the send and don't-send decisions, the remember mapping
// onto the standing preference, and the subject override.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { ReplyPromptDialog } from './ReplyPromptDialog';
import type { ReplyPromptRow } from '@/services/ipc';

/** The prompt the fixtures raise: an accepted answer the server could not deliver. */
const prompt: ReplyPromptRow = {
  account: 'work',
  message: 'imap:v1:u7@INBOX',
  summary: 'Sprint planning',
  organizer: 'boss@example.test',
  response: 'accepted',
  comment: 'See you then',
  status_code: '5.2',
};

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe('the reply prompt dialog', () => {
  it('names the meeting, the organiser, the sentence, and the note the reply carries', () => {
    render(<ReplyPromptDialog prompt={prompt} />);

    expect(screen.getByText('Sprint planning')).toBeTruthy();
    expect(screen.getByText('boss@example.test')).toBeTruthy();
    expect(
      screen.getByText(
        'Your answer reached the calendar, but the server could not tell the organiser.',
      ),
    ).toBeTruthy();
    expect(screen.getByDisplayValue('See you then')).toBeTruthy();
  });

  it('sends and dismisses', () => {
    render(<ReplyPromptDialog prompt={prompt} />);

    fireEvent.click(screen.getByRole('button', { name: 'Send' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"answer_reply_prompt":{"send":true,"remember":false}}}',
    );
  });

  it('answers no and dismisses', () => {
    render(<ReplyPromptDialog prompt={prompt} />);

    fireEvent.click(screen.getByRole('button', { name: "Don't send" }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"answer_reply_prompt":{"send":false,"remember":false}}}',
    );
  });

  it('maps a remembered send and a remembered refusal onto the standing preference', () => {
    const { unmount } = render(<ReplyPromptDialog prompt={prompt} />);
    fireEvent.click(screen.getByLabelText('Remember this decision'));
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"answer_reply_prompt":{"send":true,"remember":true}}}',
    );
    unmount();

    render(<ReplyPromptDialog prompt={prompt} />);
    fireEvent.click(screen.getByLabelText('Remember this decision'));
    fireEvent.click(screen.getByRole('button', { name: "Don't send" }));
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"calendar":{"answer_reply_prompt":{"send":false,"remember":true}}}',
    );
  });

  it('carries the subject override when one is typed', () => {
    render(<ReplyPromptDialog prompt={prompt} />);

    fireEvent.change(screen.getByLabelText('Subject'), {
      target: { value: 'Zusage: Sprint planning' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"answer_reply_prompt":{"send":true,"remember":false,"reply_subject":"Zusage: Sprint planning"}}}',
    );
  });
});
