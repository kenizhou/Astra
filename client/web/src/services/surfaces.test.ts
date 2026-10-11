// SPDX-License-Identifier: GPL-3.0-only
// The snapshot wire, pinned against the literal JSON astra-viewmodel's
// view_tests.rs asserts: the list fields cross exactly as the Rust side
// writes them, and an at-rest snapshot crosses without them at all.
import { describe, expect, it } from 'vitest';

import type { MailboxListSnapshot, SearchState } from './surfaces';
import fixtureJson from './__fixtures__/mailbox_list.json';

// The literal a snapshot full of message rows crosses as: two trees, then
// the thread row (with its thread_count), the flat message row (without),
// the selection, the window, and the grouping mode.
const WITH_ROWS =
  '{"account_folders":[{"account_id":"work","display_name":"me@work.example","expanded":true,"folders":[' +
  '{"key":"INBOX","name":"Inbox","role":"inbox","unread":0,"depth":0,"has_children":false,"expanded":false}]}],' +
  '"rows":[' +
  '{"key":{"thread":{"account":"work","thread_id":"th-7"}},"account_id":"work","sender_name":"Grace Hopper","sender_address":"grace@example.test","subject":"Re: Compiler design","sent_at":"2026-10-08T16:05:00Z","unread":false,"flagged":true,"has_attachments":false,"thread_count":3},' +
  '{"key":{"message":{"account":"work","key":"imap:v1:u42"}},"account_id":"work","sender_name":"Ada Lovelace","sender_address":"ada@example.test","subject":"Quarterly report","sent_at":"2026-10-09T08:30:00Z","unread":true,"flagged":false,"has_attachments":true}],' +
  '"selection":{"thread":{"account":"work","thread_id":"th-7"}},' +
  '"visible_limit":100,"view_mode":"threaded"}';

// The at-rest shape: a snapshot listing no mail crosses with none of the
// list fields, byte-identically to the shape before they existed.
const AT_REST =
  '{"account_folders":[{"account_id":"home","display_name":"Home","expanded":false,"folders":[]}]}';

describe('the mailbox-list snapshot wire', () => {
  it('carries the list fields after the trees, field for field', () => {
    const snapshot = JSON.parse(WITH_ROWS) as MailboxListSnapshot;

    expect(snapshot.rows).toHaveLength(2);
    expect(snapshot.rows?.[0]).toEqual({
      key: { thread: { account: 'work', thread_id: 'th-7' } },
      account_id: 'work',
      sender_name: 'Grace Hopper',
      sender_address: 'grace@example.test',
      subject: 'Re: Compiler design',
      sent_at: '2026-10-08T16:05:00Z',
      unread: false,
      flagged: true,
      has_attachments: false,
      thread_count: 3,
    });
    // A flat row carries no thread count: the wire omits the field entirely.
    expect(snapshot.rows?.[1]).toEqual({
      key: { message: { account: 'work', key: 'imap:v1:u42' } },
      account_id: 'work',
      sender_name: 'Ada Lovelace',
      sender_address: 'ada@example.test',
      subject: 'Quarterly report',
      sent_at: '2026-10-09T08:30:00Z',
      unread: true,
      flagged: false,
      has_attachments: true,
    });
    expect(snapshot.selection).toEqual({ thread: { account: 'work', thread_id: 'th-7' } });
    expect(snapshot.visible_limit).toBe(100);
    expect(snapshot.view_mode).toBe('threaded');
  });

  it('crosses an at-rest snapshot without any of the list fields', () => {
    const snapshot = JSON.parse(AT_REST) as MailboxListSnapshot;

    expect(snapshot.account_folders).toHaveLength(1);
    expect(snapshot.rows).toBeUndefined();
    expect(snapshot.selection).toBeUndefined();
    expect(snapshot.visible_limit).toBeUndefined();
    expect(snapshot.view_mode).toBeUndefined();
  });
});

describe('the shared fixture', () => {
  it('is a well-formed flat-mode snapshot with two rows', () => {
    const fixture = fixtureJson as MailboxListSnapshot;

    expect(fixture.account_folders).toHaveLength(2);
    expect(fixture.rows).toHaveLength(2);
    expect(fixture.rows?.[0]?.key).toEqual({
      message: { account: 'work', key: 'imap:v1:u42' },
    });
    expect(fixture.rows?.[0]?.sender_name).toBe('Ada Lovelace');
    expect(fixture.rows?.[0]?.sender_address).toBe('ada@example.test');
    expect(fixture.rows?.[1]?.sender_name).toBe('');
    expect(fixture.rows?.[1]?.thread_count).toBeUndefined();
    expect(fixture.visible_limit).toBe(50);
    // Flat is the default the wire skips, and nothing is selected.
    expect('view_mode' in fixture).toBe(false);
    expect('selection' in fixture).toBe(false);
  });
});

// Both literals are byte-for-byte the JSON astra-viewmodel's search_tests.rs
// asserts, so the search state cannot drift between the host and the surface.
describe('the search state wire', () => {
  it('crosses with the query, the scope, the rows, and the depth', () => {
    const state = JSON.parse(
      '{"query":"from:ada monitor","scope":"current_folder","rows":[' +
        '{"key":{"thread":{"account":"work","thread_id":"th-7"}},"account_id":"work",' +
        '"sender_name":"Grace Hopper","sender_address":"grace@example.test",' +
        '"subject":"Re: Compiler design","sent_at":"2026-10-08T16:05:00Z",' +
        '"unread":false,"flagged":true,"has_attachments":false,"thread_count":3}],' +
        '"depth_months":3}',
    ) as SearchState;

    expect(state.query).toBe('from:ada monitor');
    expect(state.scope).toBe('current_folder');
    expect(state.rows).toEqual([
      {
        key: { thread: { account: 'work', thread_id: 'th-7' } },
        account_id: 'work',
        sender_name: 'Grace Hopper',
        sender_address: 'grace@example.test',
        subject: 'Re: Compiler design',
        sent_at: '2026-10-08T16:05:00Z',
        unread: false,
        flagged: true,
        has_attachments: false,
        thread_count: 3,
      },
    ]);
    expect(state.depth_months).toBe(3);
  });

  it('rides the snapshot as one more field, absent while no search runs', () => {
    const snapshot = JSON.parse(
      '{"account_folders":[],"search":{"query":"monitor","scope":"all_except_trash","rows":[],"depth_months":6}}',
    ) as MailboxListSnapshot;

    expect(snapshot.search).toEqual({
      query: 'monitor',
      scope: 'all_except_trash',
      rows: [],
      depth_months: 6,
    });

    const atRest = JSON.parse(AT_REST) as MailboxListSnapshot;
    expect(atRest.search).toBeUndefined();
  });
});
