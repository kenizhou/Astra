// SPDX-License-Identifier: GPL-3.0-only
// The intent wire, pinned against the literal JSON astra-app's protocol_tests.rs
// asserts: every variant crosses as exactly the bytes the Rust side reads, so a
// drift on either side fails here first.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import { addAccount, dispatch, imapTestConnection, intents, mailboxList, syncProgress } from './ipc';

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
