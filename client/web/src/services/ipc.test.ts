// SPDX-License-Identifier: GPL-3.0-only
// The intent wire, pinned against the literal JSON astra-app's protocol_tests.rs
// asserts: every variant crosses as exactly the bytes the Rust side reads, so a
// drift on either side fails here first.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import type {
  ChipView,
  ComposerDocument,
  DraftPoolLanded,
  DraftStatusSnapshot,
  ReadingSnapshot,
  RecommendationView,
  ResumeView,
} from './ipc';

import {
  addAccount,
  detectAccountSettings,
  dispatch,
  draftResume,
  draftStatus,
  imapTestConnection,
  intents,
  mailboxList,
  reading,
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

  it('carries the row reference with archive and trash', () => {
    // The Rust protocol exercises these through the app API, not a byte pin;
    // these literals are the byte contract the webview relies on.
    const message = { message: { account: 'work', key: 'k1' } };
    const thread = { thread: { account: 'work', thread_id: 'th-7' } };
    expect(JSON.stringify(intents.archiveMessage(message))).toBe('{"archive_message":{"row":{"message":{"account":"work","key":"k1"}}}}');
    expect(JSON.stringify(intents.archiveMessage(thread))).toBe('{"archive_message":{"row":{"thread":{"account":"work","thread_id":"th-7"}}}}');
    expect(JSON.stringify(intents.moveToTrash(message))).toBe('{"move_to_trash":{"row":{"message":{"account":"work","key":"k1"}}}}');
    expect(JSON.stringify(intents.moveToTrash(thread))).toBe('{"move_to_trash":{"row":{"thread":{"account":"work","thread_id":"th-7"}}}}');
  });

  it('omits a top level create parent and carries a nested one', () => {
    expect(JSON.stringify(intents.createFolder('work', 'Archive'))).toBe(
      '{"folders":{"create":{"account":"work","name":"Archive"}}}',
    );
    expect(JSON.stringify(intents.createFolder('work', 'Archive', 'INBOX'))).toBe(
      '{"folders":{"create":{"account":"work","parent":"INBOX","name":"Archive"}}}',
    );
  });

  it('names the message the reading pane opens by reference', () => {
    expect(JSON.stringify(intents.openMessage({ account: 'work', key: 'imap:v1:u1' }))).toBe(
      '{"open_message":{"message":{"account":"work","key":"imap:v1:u1"}}}',
    );
  });

  it('crosses close reading as a bare renamed word', () => {
    expect(JSON.stringify(intents.closeReading())).toBe('"close_reading"');
  });

  // Both literals are byte-for-byte the JSON astra-app's protocol_tests.rs
  // asserts for the drafts family, so neither side of the wire can drift.
  it('nests the drafts save under its tag with the close flag both ways', () => {
    const document: ComposerDocument = {
      blocks: [
        {
          Paragraph: {
            content: [
              {
                Text: {
                  text: 'Half written',
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

    expect(
      JSON.stringify(
        intents.saveDraft({
          composition: 'compose-1',
          from: 'work',
          to: 'you@test.local',
          cc: '',
          bcc: '',
          subject: 'Later',
          document,
          then_close: false,
        }),
      ),
    ).toBe(
      '{"drafts":{"save":{"composition":"compose-1","from":"work","to":"you@test.local","cc":"","bcc":"","subject":"Later","document":{"blocks":[{"Paragraph":{"content":[{"Text":{"text":"Half written","bold":false,"italic":false,"underline":false,"font_size":null,"color":null,"highlight":null,"link":null}}]}}],"attachments":[]},"then_close":false}}}',
    );

    expect(
      JSON.stringify(
        intents.saveDraft({
          composition: 'compose-1',
          from: 'work',
          to: '',
          cc: '',
          bcc: '',
          subject: '',
          document: {
            blocks: [
              {
                Paragraph: {
                  content: [
                    {
                      Text: {
                        text: 'On my way out',
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
          },
          then_close: true,
        }),
      ),
    ).toBe(
      '{"drafts":{"save":{"composition":"compose-1","from":"work","to":"","cc":"","bcc":"","subject":"","document":{"blocks":[{"Paragraph":{"content":[{"Text":{"text":"On my way out","bold":false,"italic":false,"underline":false,"font_size":null,"color":null,"highlight":null,"link":null}}]}}],"attachments":[]},"then_close":true}}}',
    );
  });

  it('nests the drafts discard and close under their tags', () => {
    expect(JSON.stringify(intents.discardDraft('compose-1'))).toBe(
      '{"drafts":{"discard":{"composition":"compose-1"}}}',
    );
    expect(JSON.stringify(intents.closeDraft('compose-1'))).toBe(
      '{"drafts":{"close":{"composition":"compose-1"}}}',
    );
  });

  // Both literals are byte-for-byte the JSON astra-app's protocol_search_tests.rs
  // asserts, so neither side of the search wire can drift.
  it('carries the search query, or clears the search with an explicit null', () => {
    expect(JSON.stringify(intents.search('from:ada monitor'))).toBe(
      '{"search":{"query":"from:ada monitor"}}',
    );
    expect(JSON.stringify(intents.search(null))).toBe('{"search":{"query":null}}');
  });

  it('carries each search scope as a plain snake_case word', () => {
    expect(JSON.stringify(intents.setSearchScope('all_except_trash'))).toBe(
      '{"set_search_scope":{"scope":"all_except_trash"}}',
    );
    expect(JSON.stringify(intents.setSearchScope('current_folder'))).toBe(
      '{"set_search_scope":{"scope":"current_folder"}}',
    );
    expect(JSON.stringify(intents.setSearchScope('current_account'))).toBe(
      '{"set_search_scope":{"scope":"current_account"}}',
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

// Both literals are byte-for-byte the JSON astra-viewmodel's reading_tests.rs
// asserts on the Rust side, so neither side of the wire can drift.
describe('the reading wire', () => {
  it('mirrors the full snapshot with every optional key', () => {
    const wire = JSON.parse(
      '{"message":{"account":"work","key":"imap:v1:u42"},"subject":"Quarterly report","from_name":"Ada Lovelace","from_address":"ada@example.test","sent_at":"2026-10-09T08:30:00Z","html":"<p>See the report.</p>","plain":"See the report.","attachments":[{"id":1,"file_name":"report.pdf","media_type":"application/pdf","size":2048},{"id":2,"file_name":"chart.png","media_type":"image/png","size":512}],"remote_image_count":2}',
    ) as ReadingSnapshot;

    expect(wire.message).toEqual({ account: 'work', key: 'imap:v1:u42' });
    expect(wire.subject).toBe('Quarterly report');
    expect(wire.from_name).toBe('Ada Lovelace');
    expect(wire.from_address).toBe('ada@example.test');
    expect(wire.sent_at).toBe('2026-10-09T08:30:00Z');
    expect(wire.html).toBe('<p>See the report.</p>');
    expect(wire.plain).toBe('See the report.');
    expect(wire.attachments).toEqual([
      { id: 1, file_name: 'report.pdf', media_type: 'application/pdf', size: 2048 },
      { id: 2, file_name: 'chart.png', media_type: 'image/png', size: 512 },
    ]);
    expect(wire.remote_image_count).toBe(2);
  });

  it('mirrors the plain only snapshot with no optional keys on the wire', () => {
    const raw =
      '{"message":{"account":"home","key":"imap:v1:u7"},"subject":"Lunch","from_name":"Grace Hopper","from_address":"grace@example.test","sent_at":"2026-10-09T09:15:00Z","plain":"Sandwiches at noon.","remote_image_count":0}';
    // The at-rest optionals are absent from the wire, not carried as nulls or
    // empty collections: no html, no attachments key at all.
    expect(raw).not.toContain('"html"');
    expect(raw).not.toContain('"attachments"');

    const wire = JSON.parse(raw) as ReadingSnapshot;
    expect(wire.message).toEqual({ account: 'home', key: 'imap:v1:u7' });
    expect(wire.subject).toBe('Lunch');
    expect(wire.from_name).toBe('Grace Hopper');
    expect(wire.from_address).toBe('grace@example.test');
    expect(wire.sent_at).toBe('2026-10-09T09:15:00Z');
    expect(wire.html).toBeUndefined();
    expect(wire.plain).toBe('Sandwiches at noon.');
    expect(wire.attachments).toBeUndefined();
    expect(wire.remote_image_count).toBe(0);
  });

  it('pulls the reading snapshot by its command name', async () => {
    invoke.mockResolvedValue(null);
    await reading();
    expect(invoke).toHaveBeenCalledWith('reading');
  });
});

// The literal is byte-for-byte the JSON astra-composer's types_tests.rs parses,
// with the marks pinned the way astra-app's protocol_tests.rs asserts them, so
// the composer document cannot drift between editor and core.
describe('the composer document wire', () => {
  it('carries a paragraph of one marked run with every field present', () => {
    const raw =
      '{"blocks":[{"Paragraph":{"content":[{"Text":{"text":"Half written","bold":false,"italic":false,"underline":false,"font_size":null,"color":null,"highlight":null,"link":null}}]}}],"attachments":[]}';
    const wire = JSON.parse(raw) as ComposerDocument;

    expect(wire.blocks).toEqual([
      {
        Paragraph: {
          content: [
            {
              Text: {
                text: 'Half written',
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
    ]);
    expect(wire.attachments).toEqual([]);
  });

  it('carries an inline image with its attachment id and width', () => {
    const raw =
      '{"blocks":[{"Paragraph":{"content":[{"Image":{"attachment_id":"img-1","alt_text":"chart","width_px":70000}}]}}],"attachments":[]}';
    const wire = JSON.parse(raw) as ComposerDocument;

    expect(wire.blocks[0]).toEqual({
      Paragraph: {
        content: [{ Image: { attachment_id: 'img-1', alt_text: 'chart', width_px: 70000 } }],
      },
    });
  });
});

// Both literals are byte-for-byte the JSON astra-viewmodel's drafts_tests.rs
// asserts on the Rust side, so neither side of the wire can drift.
describe('the draft status wire', () => {
  it('mirrors the saved snapshot with the state and the instant', () => {
    const wire = JSON.parse(
      '{"composition":"compose-1","state":"saved","saved_at":"2026-10-09T08:30:00Z"}',
    ) as DraftStatusSnapshot;

    expect(wire.composition).toBe('compose-1');
    expect(wire.state).toBe('saved');
    expect(wire.saved_at).toBe('2026-10-09T08:30:00Z');
  });

  it('mirrors the saving snapshot with an empty instant', () => {
    const wire = JSON.parse('{"composition":"compose-2","state":"saving","saved_at":""}') as DraftStatusSnapshot;

    expect(wire.composition).toBe('compose-2');
    expect(wire.state).toBe('saving');
    expect(wire.saved_at).toBe('');
  });

  it('pulls the status by its command name', async () => {
    invoke.mockResolvedValue(null);
    await draftStatus();
    expect(invoke).toHaveBeenCalledWith('draft_status');
  });
});

// The literal is byte-for-byte the JSON astra-desktop's drafts_tests.rs asserts
// for the resume view, so the composer hydrates from the fields the host sends.
describe('the resume wire', () => {
  it('mirrors the stored draft as one flat record', () => {
    const wire = JSON.parse(
      '{"account":"work","to":"you@test.local","cc":"","bcc":"","subject":"Later","body_text":"Half written"}',
    ) as ResumeView;

    expect(wire.account).toBe('work');
    expect(wire.to).toBe('you@test.local');
    expect(wire.cc).toBe('');
    expect(wire.bcc).toBe('');
    expect(wire.subject).toBe('Later');
    expect(wire.body_text).toBe('Half written');
  });

  it('passes the account and composition to draft_resume by name', async () => {
    invoke.mockResolvedValue(null);
    await draftResume('work', 'compose-9');
    expect(invoke).toHaveBeenCalledWith('draft_resume', {
      account: 'work',
      composition: 'compose-9',
    });
  });
});

// The literal is byte-for-byte the JSON astra-app's gate and astra-desktop's
// ai_tests.rs assert for the chip and the draft-pool event payload, so the
// reading pane renders the fields the host sends.
describe('the ai replies wire', () => {
  it('crosses one chip as its reply beside its preview', () => {
    const chip: ChipView = { reply: 'Sounds good, shipped it.', preview: 'Sounds good...' };
    expect(JSON.stringify(chip)).toBe(
      '{"reply":"Sounds good, shipped it.","preview":"Sounds good..."}',
    );
  });

  it('names the landed message by its account and key', () => {
    const landed: DraftPoolLanded = { account: 'work', key: 'imap:v1:u42' };
    expect(JSON.stringify(landed)).toBe('{"account":"work","key":"imap:v1:u42"}');
  });
});
