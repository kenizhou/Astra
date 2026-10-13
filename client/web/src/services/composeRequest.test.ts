// SPDX-License-Identifier: GPL-3.0-only
// The compose wire, pinned against the literal JSON the Rust side asserts:
// the compose family and its two threaded submits against astra-app's
// protocol_tests.rs and protocol_submit_tests.rs, and the request snapshot
// against astra-viewmodel's compose_request_tests.rs, so neither side of the
// wire can drift. Imported through ipc.ts, the one import point, so the
// re-export is pinned too.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import type { ComposeKind, ComposeRequestSnapshot, ComposerDocument } from './ipc';

import { composeRequest, intents, takeComposeRequest } from './ipc';

/** A one-paragraph document carrying `text`, the smallest document a send holds. */
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

// Every literal is byte-for-byte the JSON astra-app's protocol_tests.rs asserts
// for the compose family, with the message reference under each verb.
describe('the compose family wire', () => {
  it('names the message each verb answers', () => {
    expect(JSON.stringify(intents.replyTo({ account: 'work', key: 'm1' }))).toBe(
      '{"compose":{"reply":{"message":{"account":"work","key":"m1"}}}}',
    );
    expect(JSON.stringify(intents.replyAllTo({ account: 'work', key: 'm1' }))).toBe(
      '{"compose":{"reply_all":{"message":{"account":"work","key":"m1"}}}}',
    );
    expect(JSON.stringify(intents.forward({ account: 'home', key: 'jmap:e2' }))).toBe(
      '{"compose":{"forward":{"message":{"account":"home","key":"jmap:e2"}}}}',
    );
  });
});

// Both literals are byte-for-byte the JSON astra-app's
// protocol_submit_tests.rs asserts for the threaded submits, so neither side
// of the wire can drift.
describe('the threaded submit wire', () => {
  it('carries the reply with the threading header beside the document', () => {
    expect(
      JSON.stringify(
        intents.submitRichReply({
          from: 'work',
          to: 'ada@example.test',
          cc: '',
          in_reply_to: 'orig-1@server.test',
          subject: 'Re: Quarterly report',
          document: document('Agreed'),
          blobs: [],
        }),
      ),
    ).toBe(
      '{"submit_rich_reply":{"from":"work","to":"ada@example.test","cc":"","in_reply_to":"orig-1@server.test","subject":"Re: Quarterly report","document":{"blocks":[{"Paragraph":{"content":[{"Text":{"text":"Agreed","bold":false,"italic":false,"underline":false,"font_size":null,"color":null,"highlight":null,"link":null}}]}}],"attachments":[]},"blobs":[]}}',
    );
  });

  it('carries host-resolved attachment bytes beside a reply', () => {
    expect(
      JSON.stringify(
        intents.submitRichReply({
          from: 'work',
          to: 'ada@example.test',
          cc: 'carol@example.test',
          in_reply_to: 'orig-1@server.test',
          subject: '',
          document: document('With a file'),
          blobs: [{ handle: 'blob-1', bytes: [1, 2, 3] }],
        }),
      ),
    ).toBe(
      '{"submit_rich_reply":{"from":"work","to":"ada@example.test","cc":"carol@example.test","in_reply_to":"orig-1@server.test","subject":"","document":{"blocks":[{"Paragraph":{"content":[{"Text":{"text":"With a file","bold":false,"italic":false,"underline":false,"font_size":null,"color":null,"highlight":null,"link":null}}]}}],"attachments":[]},"blobs":[{"handle":"blob-1","bytes":[1,2,3]}]}}',
    );
  });

  it('carries the forward with no threading header', () => {
    expect(
      JSON.stringify(
        intents.submitRichForward({
          from: 'work',
          to: 'bob@example.test',
          subject: 'Fwd: Quarterly report',
          document: document('Look at this'),
          blobs: [],
        }),
      ),
    ).toBe(
      '{"submit_rich_forward":{"from":"work","to":"bob@example.test","subject":"Fwd: Quarterly report","document":{"blocks":[{"Paragraph":{"content":[{"Text":{"text":"Look at this","bold":false,"italic":false,"underline":false,"font_size":null,"color":null,"highlight":null,"link":null}}]}}],"attachments":[]},"blobs":[]}}',
    );
  });
});

// Both literals are byte-for-byte the JSON astra-viewmodel's
// compose_request_tests.rs asserts on the Rust side, so neither side of the
// wire can drift.
describe('the compose request snapshot wire', () => {
  it('mirrors the reply request with the threading header and the quote', () => {
    const wire = JSON.parse(
      '{"kind":"reply","from":"work","to":"ada@example.test","cc":"","subject":"Re: Quarterly report","in_reply_to":"orig-1@server.test","document":{"blocks":[{"Quote":{"style":"Indented","attribution":{"line":"On 9 Oct 2026 at 08:00, Ada Lovelace <ada@example.test> wrote:","headers":[{"label":"From","value":"Ada Lovelace <ada@example.test>"},{"label":"Sent","value":"9 Oct 2026 at 08:00"},{"label":"To","value":"me@work.local, Bob <bob@example.test>"},{"label":"Subject","value":"Quarterly report"}]},"body_html":"<div style=\\"white-space: pre-wrap;\\">The numbers are in.</div>","body_plain":"The numbers are in."}}],"attachments":[]}}',
    ) as ComposeRequestSnapshot;

    expect(wire.kind).toBe('reply');
    expect(wire.from).toBe('work');
    expect(wire.to).toBe('ada@example.test');
    expect(wire.cc).toBe('');
    expect(wire.subject).toBe('Re: Quarterly report');
    expect(wire.in_reply_to).toBe('orig-1@server.test');
    expect(wire.document.blocks).toEqual([
      {
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
      },
    ]);
    expect(wire.document.attachments).toEqual([]);
  });

  it('mirrors the forward request with no threading header key', () => {
    const raw =
      '{"kind":"forward","from":"work","to":"","cc":"","subject":"Fwd: Quarterly report","document":{"blocks":[{"Quote":{"style":"Indented","attribution":{"line":"Forwarded message","headers":[{"label":"Subject","value":"Quarterly report"}]},"body_html":"<div style=\\"white-space: pre-wrap;\\">The numbers are in.</div>","body_plain":"The numbers are in."}}],"attachments":[]}}';
    // The at-rest optional is absent from the wire, not carried as null: no
    // in_reply_to key at all, because a forward answers no message.
    expect(raw).not.toContain('"in_reply_to"');

    const wire = JSON.parse(raw) as ComposeRequestSnapshot;
    expect(wire.kind).toBe('forward');
    expect(wire.to).toBe('');
    expect(wire.subject).toBe('Fwd: Quarterly report');
    expect(wire.in_reply_to).toBeUndefined();
    expect(wire.document.blocks[0]).toEqual({
      Quote: {
        style: 'Indented',
        attribution: {
          line: 'Forwarded message',
          headers: [{ label: 'Subject', value: 'Quarterly report' }],
        },
        body_html: '<div style="white-space: pre-wrap;">The numbers are in.</div>',
        body_plain: 'The numbers are in.',
      },
    });
  });

  it('crosses the kinds as plain words', () => {
    // The three words the Rust side pins, one snapshot each; the CamelCase
    // spelling is not the wire.
    for (const kind of ['reply', 'reply_all', 'forward'] as const) {
      const raw =
        `{"kind":"${kind}","from":"work","to":"","cc":"","subject":"",` +
        '"document":{"blocks":[],"attachments":[]}}';
      const wire = JSON.parse(raw) as ComposeRequestSnapshot;
      expect(wire.kind).toBe(kind satisfies ComposeKind);
    }
  });
});

describe('the compose request pull', () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it('pulls the snapshot by its command name', async () => {
    invoke.mockResolvedValue(null);
    await composeRequest();
    expect(invoke).toHaveBeenCalledWith('compose_request');
  });

  it('takes the pending request by its command name, consuming the slot', async () => {
    invoke.mockResolvedValue(null);
    await takeComposeRequest();
    expect(invoke).toHaveBeenCalledWith('take_compose_request');
  });
});
