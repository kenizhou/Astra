// SPDX-License-Identifier: GPL-3.0-only
// The sending wire, pinned against the literal JSON the Rust side asserts:
// the submit and the outbox family against astra-app's protocol_tests.rs, and
// the queue's snapshot against astra-viewmodel's sending_tests.rs, so neither
// side of the wire can drift. Imported through ipc.ts, the one import point,
// so the re-export is pinned too.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import type { ComposerDocument, QueuedRow, QueuedState, SendingSnapshot } from './ipc';

import { intents, sending } from './ipc';

/** A one-paragraph document carrying `text`, the smallest document a send can hold. */
function document(text: string): ComposerDocument {
  return {
    blocks: [
      {
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
      },
    ],
    attachments: [],
  };
}

// Both literals are byte-for-byte the JSON astra-app's protocol_tests.rs
// asserts for the submit, so neither side of the wire can drift.
describe('the submit wire', () => {
  it('carries the recipients the document and an empty blob list', () => {
    expect(
      JSON.stringify(
        intents.submitRichMail({
          from: 'work',
          to: 'you@test.local',
          cc: '',
          bcc: '',
          subject: 'On its way',
          document: document('Sent words'),
          blobs: [],
        }),
      ),
    ).toBe(
      '{"submit_rich_mail":{"from":"work","to":"you@test.local","cc":"","bcc":"","subject":"On its way","document":{"blocks":[{"Paragraph":{"content":[{"Text":{"text":"Sent words","bold":false,"italic":false,"underline":false,"font_size":null,"color":null,"highlight":null,"link":null}}]}}],"attachments":[]},"blobs":[]}}',
    );
  });

  it('carries host resolved attachment bytes beside the document', () => {
    expect(
      JSON.stringify(
        intents.submitRichMail({
          from: 'work',
          to: 'you@test.local',
          cc: '',
          bcc: '',
          subject: '',
          document: document('With a file'),
          blobs: [{ handle: 'blob-1', bytes: [1, 2, 3] }],
        }),
      ),
    ).toBe(
      '{"submit_rich_mail":{"from":"work","to":"you@test.local","cc":"","bcc":"","subject":"","document":{"blocks":[{"Paragraph":{"content":[{"Text":{"text":"With a file","bold":false,"italic":false,"underline":false,"font_size":null,"color":null,"highlight":null,"link":null}}]}}],"attachments":[]},"blobs":[{"handle":"blob-1","bytes":[1,2,3]}]}}',
    );
  });
});

// Every literal is byte-for-byte the JSON astra-app's protocol_tests.rs asserts
// for the outbox family, with the queued reference under each carrying verb.
describe('the outbox wire', () => {
  it('crosses the show as a bare word', () => {
    expect(JSON.stringify(intents.showOutbox())).toBe('{"outbox":"show"}');
  });

  it('names the queued send every action acts on', () => {
    expect(JSON.stringify(intents.cancelQueuedSend({ account: 'work', op: 3 }))).toBe(
      '{"outbox":{"cancel":{"queued":{"account":"work","op":3}}}}',
    );
    expect(JSON.stringify(intents.sendQueuedNow({ account: 'home', op: 1 }))).toBe(
      '{"outbox":{"send_now":{"queued":{"account":"home","op":1}}}}',
    );
    expect(JSON.stringify(intents.confirmQueuedSent({ account: 'work', op: 7 }))).toBe(
      '{"outbox":{"confirm_sent":{"queued":{"account":"work","op":7}}}}',
    );
    expect(JSON.stringify(intents.confirmQueuedNotSent({ account: 'home', op: 2 }))).toBe(
      '{"outbox":{"confirm_not_sent":{"queued":{"account":"home","op":2}}}}',
    );
  });

  it('names the held send every gate verb acts on', () => {
    expect(JSON.stringify(intents.answerGate('hold-7', true))).toBe(
      '{"outbox":{"answer_gate":{"hold":"hold-7","send_anyway":true}}}',
    );
    expect(JSON.stringify(intents.answerGate('hold-7', false))).toBe(
      '{"outbox":{"answer_gate":{"hold":"hold-7","send_anyway":false}}}',
    );
    expect(JSON.stringify(intents.retryHeld('hold-2'))).toBe(
      '{"outbox":{"retry_held":{"hold":"hold-2"}}}',
    );
    expect(JSON.stringify(intents.withdrawHeld('hold-2'))).toBe(
      '{"outbox":{"withdraw_held":{"hold":"hold-2"}}}',
    );
  });
});

// Both literals are byte-for-byte the JSON astra-viewmodel's sending_tests.rs
// asserts on the Rust side, so neither side of the wire can drift.
describe('the sending snapshot wire', () => {
  it('mirrors the queue with its key state subject and arrival', () => {
    const wire = JSON.parse(
      '{"rows":[{"queued":{"account":"work","op":3},"state":"queued","subject":"Quarterly review","arrived_at":"2026-10-09T08:30:00Z"}]}',
    ) as SendingSnapshot;

    expect(wire.rows).toHaveLength(1);
    expect(wire.rows[0]).toEqual({
      queued: { account: 'work', op: 3 },
      state: 'queued',
      subject: 'Quarterly review',
      arrived_at: '2026-10-09T08:30:00Z',
    });
  });

  it('crosses every state as a plain word', () => {
    // The four words the Rust side pins, one row each; the CamelCase spelling
    // is not the wire.
    for (const state of ['queued', 'sending', 'unconfirmed', 'failed'] as const) {
      const raw =
        `{"queued":{"account":"work","op":3},"state":"${state}",` +
        '"subject":"Quarterly review","arrived_at":"2026-10-09T08:30:00Z"}';
      const wire = JSON.parse(raw) as QueuedRow;
      expect(wire.state).toBe(state satisfies QueuedState);
    }
  });

  it('crosses an empty queue as an empty row list', () => {
    const wire = JSON.parse('{"rows":[]}') as SendingSnapshot;
    expect(wire.rows).toEqual([]);
  });

  it('mirrors a refused row with an empty subject and instant', () => {
    const wire = JSON.parse(
      '{"queued":{"account":"home","op":1},"state":"failed","subject":"","arrived_at":""}',
    ) as QueuedRow;

    expect(wire.queued).toEqual({ account: 'home', op: 1 });
    expect(wire.state).toBe('failed');
    expect(wire.subject).toBe('');
    expect(wire.arrived_at).toBe('');
  });
});

describe('the sending pull', () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it('pulls the snapshot by its command name', async () => {
    invoke.mockResolvedValue({ rows: [] });
    await sending();
    expect(invoke).toHaveBeenCalledWith('sending');
  });
});
