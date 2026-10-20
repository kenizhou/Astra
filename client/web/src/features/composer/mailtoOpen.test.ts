// SPDX-License-Identifier: GPL-3.0-only
// The mailto flow's wire half: the scheme predicate the reading pane
// routes anchors on, the raw ask the pane emits for a clicked mailto
// link (the host owns the parse, so only the link crosses), and the
// subscription the composer opens from, with its payload handed through
// and its unlisten honoured.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const event = vi.hoisted(() => ({ emit: vi.fn(), listen: vi.fn() }));

vi.mock('@tauri-apps/api/event', () => ({ emit: event.emit, listen: event.listen }));

import { askMailtoOpen, isMailtoHref, onMailtoOpen } from './mailtoOpen';

beforeEach(() => {
  event.emit.mockReset();
  event.emit.mockResolvedValue(undefined);
  event.listen.mockReset();
});

describe('the mailto scheme predicate', () => {
  it('accepts a mailto link whatever the scheme case', () => {
    expect(isMailtoHref('mailto:ada@example.test')).toBe(true);
    expect(isMailtoHref('MAILTO:ada@example.test?subject=Lunch')).toBe(true);
  });

  it('refuses every other scheme and look-alikes', () => {
    expect(isMailtoHref('https://example.test/mailto:x')).toBe(false);
    expect(isMailtoHref('xmailto:ada@example.test')).toBe(false);
    expect(isMailtoHref('')).toBe(false);
  });
});

describe('the reading pane ask', () => {
  it('emits the raw link, never a parse of it', async () => {
    await askMailtoOpen('mailto:ada@example.test?subject=Lunch');

    expect(event.emit).toHaveBeenCalledWith('mailto_request', {
      uri: 'mailto:ada@example.test?subject=Lunch',
    });
  });
});

describe('the open subscription', () => {
  it('subscribes to the mailto open event and hands the payload over', async () => {
    const unlisten = vi.fn();
    event.listen.mockResolvedValue(unlisten);
    const seen: unknown[] = [];

    const teardown = await onMailtoOpen((open) => {
      seen.push(open);
    });

    const [name, handler] = event.listen.mock.calls[0] as [
      string,
      (event: { payload: unknown }) => void,
    ];
    expect(name).toBe('mailto_open');
    handler({
      payload: {
        from: 'work',
        to: 'ada@example.test',
        cc: '',
        bcc: '',
        subject: 'Lunch',
        body: 'Noon?',
      },
    });
    expect(seen).toEqual([
      {
        from: 'work',
        to: 'ada@example.test',
        cc: '',
        bcc: '',
        subject: 'Lunch',
        body: 'Noon?',
      },
    ]);

    teardown();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
