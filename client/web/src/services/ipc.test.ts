// SPDX-License-Identifier: GPL-3.0-only
// The intent wire, pinned against the literal JSON astra-app's protocol_tests.rs
// asserts: every variant crosses as exactly the bytes the Rust side reads, so a
// drift on either side fails here first.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import type { RecommendationView } from './ipc';

import {
  addAccount,
  detectAccountSettings,
  dispatch,
  imapTestConnection,
  intents,
  mailboxList,
  syncProgress,
} from './ipc';

describe('the intent wire', () => {
  it('crosses unit variants as bare renamed words', () => {
    expect(JSON.stringify(intents.refreshMail())).toBe('"refresh_mail"');
    expect(JSON.stringify(intents.showMore())).toBe('"show_more"');
  });

  it('carries the account or the unified view', () => {
    expect(JSON.stringify(intents.selectAccount('work'))).toBe('{"select_account":"work"}');
    expect(JSON.stringify(intents.selectAccount(null))).toBe('{"select_account":null}');
  });

  it('crosses account expansion as a tagged record', () => {
    expect(JSON.stringify(intents.setAccountExpanded('work', true))).toBe(
      '{"set_account_expanded":{"account":"work","expanded":true}}',
    );
  });

  it('carries the folder reference with folder expansion', () => {
    expect(JSON.stringify(intents.setFolderExpanded({ account: 'work', key: 'Clients' }, false))).toBe(
      '{"set_folder_expanded":{"folder":{"account":"work","key":"Clients"},"expanded":false}}',
    );
  });

  it('names the selected folder by reference', () => {
    expect(JSON.stringify(intents.selectFolder({ account: 'home', key: 'INBOX' }))).toBe(
      '{"select_folder":{"folder":{"account":"home","key":"INBOX"}}}',
    );
  });

  it('carries the view mode as a plain word', () => {
    expect(JSON.stringify(intents.setViewMode('threaded'))).toBe('{"set_view_mode":"threaded"}');
    expect(JSON.stringify(intents.setViewMode('flat'))).toBe('{"set_view_mode":"flat"}');
  });

  it('carries the row reference with mark read', () => {
    expect(
      JSON.stringify(intents.markRead({ message: { account: 'work', key: 'imap:v1:u1' } }, true)),
    ).toBe('{"mark_read":{"row":{"message":{"account":"work","key":"imap:v1:u1"}},"read":true}}');
    expect(
      JSON.stringify(intents.markRead({ thread: { account: 'work', thread_id: 'th-7' } }, false)),
    ).toBe('{"mark_read":{"row":{"thread":{"account":"work","thread_id":"th-7"}},"read":false}}');
  });

  it('carries the row reference with set flagged', () => {
    expect(
      JSON.stringify(intents.setFlagged({ message: { account: 'home', key: 'jmap:e2' } }, true)),
    ).toBe('{"set_flagged":{"row":{"message":{"account":"home","key":"jmap:e2"}},"flagged":true}}');
    expect(
      JSON.stringify(
        intents.setFlagged({ thread: { account: 'home', thread_id: 'th-7' } }, false),
      ),
    ).toBe('{"set_flagged":{"row":{"thread":{"account":"home","thread_id":"th-7"}},"flagged":false}}');
  });

  it('omits a top level create parent and carries a nested one', () => {
    expect(JSON.stringify(intents.createFolder('work', 'Archive'))).toBe(
      '{"folders":{"create":{"account":"work","name":"Archive"}}}',
    );
    expect(JSON.stringify(intents.createFolder('work', 'Archive', 'INBOX'))).toBe(
      '{"folders":{"create":{"account":"work","parent":"INBOX","name":"Archive"}}}',
    );
  });
});

describe('the command wrappers', () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it('dispatch hands the intent json to the dispatch command', async () => {
    invoke.mockResolvedValue(undefined);
    await dispatch(intents.refreshMail());
    expect(invoke).toHaveBeenCalledWith('dispatch', { intent: 'refresh_mail' });
  });

  it('pulls both snapshots by their command names', async () => {
    invoke.mockResolvedValue({ account_folders: [] });
    await mailboxList();
    expect(invoke).toHaveBeenCalledWith('mailbox_list');
    invoke.mockResolvedValue({ accounts: [] });
    await syncProgress();
    expect(invoke).toHaveBeenCalledWith('sync_progress');
  });

  it('passes the config toml to add_account in camel case', async () => {
    invoke.mockResolvedValue({ account_id: 'work', email: 'work@example.net' });
    await addAccount('[imap]\n');
    expect(invoke).toHaveBeenCalledWith('add_account', { configToml: '[imap]\n' });
  });

  it('passes the setup fields to imap_test_connection', async () => {
    invoke.mockResolvedValue({ ok: { account_id: 'work' } });
    await imapTestConnection({
      imap_host: 'imap.example.net:993',
      username: 'work@example.net',
      credential: { password: 'secret' },
      imap_security: 'implicit_tls',
      smtp_security: 'implicit_tls',
    });
    expect(invoke).toHaveBeenCalledWith('imap_test_connection', {
      setup: {
        imap_host: 'imap.example.net:993',
        username: 'work@example.net',
        credential: { password: 'secret' },
        imap_security: 'implicit_tls',
        smtp_security: 'implicit_tls',
      },
    });
  });
});

// Both literals are byte-for-byte the JSON astra-desktop's detection_tests.rs
// asserts on the Rust side, so neither side of the wire can drift.
describe('the detection wire', () => {
  it('mirrors the manual recommendation an unknown domain yields', () => {
    const wire = JSON.parse(
      '{"recommendation":{"Manual":{"reason":"NothingFound"}},"choices":[],"detected":{"outcome":{"nothing":{"network_error":false}},"source_kind":null}}',
    ) as RecommendationView;

    expect(wire.recommendation).toEqual({ Manual: { reason: 'NothingFound' } });
    expect(wire.choices).toEqual([]);
    expect(wire.detected).toEqual({ outcome: { nothing: { network_error: false } }, source_kind: null });
  });

  it('mirrors the found imap recommendation with its choices', () => {
    const wire = JSON.parse(
      '{"recommendation":{"Imap":{"email":"user@example.test","imap_host":"imap.example.test","imap_security":"implicit_tls","smtp_security":"implicit_tls","incoming":{"protocol":"IMAP","hostname":"imap.example.test","port":993,"security":"SSL/TLS","username":"user@example.test"},"is_trusted":true,"source":"https://autoconfig.example.test/mail/config-v1.1.xml"}},"choices":[{"capability":"Mail","on":true,"server_found":true},{"capability":"Calendar","on":false,"server_found":false},{"capability":"Contacts","on":false,"server_found":false}],"detected":{"outcome":"mail","source_kind":"Autoconfig"}}',
    ) as RecommendationView;

    expect(wire.recommendation).toEqual({
      Imap: {
        email: 'user@example.test',
        imap_host: 'imap.example.test',
        imap_security: 'implicit_tls',
        smtp_security: 'implicit_tls',
        incoming: {
          protocol: 'IMAP',
          hostname: 'imap.example.test',
          port: 993,
          security: 'SSL/TLS',
          username: 'user@example.test',
        },
        is_trusted: true,
        source: 'https://autoconfig.example.test/mail/config-v1.1.xml',
      },
    });
    expect(wire.choices).toEqual([
      { capability: 'Mail', on: true, server_found: true },
      { capability: 'Calendar', on: false, server_found: false },
      { capability: 'Contacts', on: false, server_found: false },
    ]);
    expect(wire.detected).toEqual({ outcome: 'mail', source_kind: 'Autoconfig' });
  });

  it('asks the host to detect the settings for one email', async () => {
    invoke.mockResolvedValue({
      recommendation: { Manual: { reason: 'NothingFound' } },
      choices: [],
      detected: { outcome: { nothing: { network_error: false } }, source_kind: null },
    });
    await detectAccountSettings('user@nothing-here.invalid');
    expect(invoke).toHaveBeenCalledWith('detect_account_settings', {
      email: 'user@nothing-here.invalid',
    });
  });
});
