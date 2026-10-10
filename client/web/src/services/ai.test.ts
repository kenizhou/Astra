// SPDX-License-Identifier: GPL-3.0-only
// The AI replies service wire, pinned against the desktop host's command
// surface: every wrapper invokes the command its Rust side registers with
// the account and key payload the serde there reads, and the draft-pool
// event subscription maps the host's payload onto the message reference
// the reading pane matches against its open row.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const wire = vi.hoisted(() => ({
  invoke: vi.fn(),
  events: [] as string[],
  handlers: [] as Array<(event: { payload: unknown }) => void>,
  unlistens: 0,
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: wire.invoke }));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, handler: (event: { payload: unknown }) => void) => {
    wire.events.push(event);
    wire.handlers.push(handler);
    return () => {
      wire.unlistens += 1;
    };
  }),
}));

import {
  accountStyleProfile,
  chipsFor,
  composeFromDraft,
  dismissChips,
  onDraftPoolLanded,
  summariseMessage,
} from './ai';

beforeEach(() => {
  wire.invoke.mockReset();
  wire.events.length = 0;
  wire.handlers.length = 0;
  wire.unlistens = 0;
});

describe('the ai replies service', () => {
  it('pulls the chips for one message by its reference', async () => {
    wire.invoke.mockResolvedValue([{ reply: 'Shipped.', preview: 'Shipped.' }]);
    const chips = await chipsFor({ account: 'work', key: 'imap:v1:u42' });
    expect(chips).toEqual([{ reply: 'Shipped.', preview: 'Shipped.' }]);
    expect(wire.invoke).toHaveBeenCalledWith('draft_chips', {
      account: 'work',
      key: 'imap:v1:u42',
    });
  });

  it('summarises one message by its reference', async () => {
    wire.invoke.mockResolvedValue('The launch is ready.');
    const summary = await summariseMessage({ account: 'work', key: 'imap:v1:u42' });
    expect(summary).toBe('The launch is ready.');
    expect(wire.invoke).toHaveBeenCalledWith('summarise', {
      account: 'work',
      key: 'imap:v1:u42',
    });
  });

  it('pulls the account style note by the account alone', async () => {
    wire.invoke.mockResolvedValue('Crisp and warm.');
    const note = await accountStyleProfile('work');
    expect(note).toBe('Crisp and warm.');
    expect(wire.invoke).toHaveBeenCalledWith('style_profile', { account: 'work' });
  });

  it('dismisses one message\'s chips by its reference', async () => {
    wire.invoke.mockResolvedValue(undefined);
    await dismissChips({ account: 'work', key: 'imap:v1:u42' });
    expect(wire.invoke).toHaveBeenCalledWith('chips_dismiss', {
      account: 'work',
      key: 'imap:v1:u42',
    });
  });

  it('opens the reply composer on one accepted chip, draft and all', async () => {
    wire.invoke.mockResolvedValue(undefined);
    await composeFromDraft({ account: 'work', key: 'imap:v1:u42' }, 'Shipped.');
    expect(wire.invoke).toHaveBeenCalledWith('compose_from_draft', {
      account: 'work',
      key: 'imap:v1:u42',
      replyText: 'Shipped.',
    });
  });

  it('maps the draft-pool event onto the landed message reference', async () => {
    const seen: Array<{ account: string; key: string }> = [];
    const unlisten = await onDraftPoolLanded((message) => {
      seen.push(message);
    });
    expect(wire.events).toEqual(['draft_pool']);
    expect(wire.handlers).toHaveLength(1);
    wire.handlers[0]?.({ payload: { account: 'work', key: 'imap:v1:u42' } });
    expect(seen).toEqual([{ account: 'work', key: 'imap:v1:u42' }]);
    unlisten();
    expect(wire.unlistens).toBe(1);
  });
});
