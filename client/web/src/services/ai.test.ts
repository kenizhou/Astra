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
  composeAutocomplete,
  composeFromDraft,
  digestGenerate,
  dismissChips,
  knowledgeExtract,
  knowledgeForget,
  knowledgeList,
  latestDocument,
  onDraftPoolLanded,
  reportGenerate,
  snippetList,
  snippetSearch,
  splitMap,
  summariseMessage,
  translateMessage,
} from './ai';
import type {
  DigestView,
  DocumentKind,
  DocumentView,
  ExtractionSummary,
  KnowledgeView,
  SnippetView,
  SplitEntry,
  SplitSide,
  TranslateLanguage,
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

  it('lists one account\'s knowledge entries by the account alone', async () => {
    wire.invoke.mockResolvedValue([
      {
        id: 'k1',
        kind: 'fact',
        text: 'The launch lands on Friday.',
        source_account: 'work',
        source_key: 'imap:v1:u42',
        created_at: 1,
      },
    ]);
    const entries = await knowledgeList('work');
    expect(entries).toEqual([
      {
        id: 'k1',
        kind: 'fact',
        text: 'The launch lands on Friday.',
        source_account: 'work',
        source_key: 'imap:v1:u42',
        created_at: 1,
      },
    ]);
    expect(wire.invoke).toHaveBeenCalledWith('knowledge_list', { account: 'work' });
  });

  it('runs one extraction, the limit named or the cap implied', async () => {
    wire.invoke.mockResolvedValue({ extracted: 2, skipped_duplicates: 1, messages_considered: 3 });
    const summary = await knowledgeExtract('work');
    expect(summary).toEqual<ExtractionSummary>({
      extracted: 2,
      skipped_duplicates: 1,
      messages_considered: 3,
    });
    expect(wire.invoke).toHaveBeenCalledWith('knowledge_extract', { account: 'work' });
    await knowledgeExtract('work', 10);
    expect(wire.invoke).toHaveBeenLastCalledWith('knowledge_extract', { account: 'work', limit: 10 });
  });

  it('forgets one knowledge entry by its id', async () => {
    wire.invoke.mockResolvedValue(true);
    const removed = await knowledgeForget('k1');
    expect(removed).toBe(true);
    expect(wire.invoke).toHaveBeenCalledWith('knowledge_forget', { id: 'k1' });
  });

  it('lists the snippet library and searches it by the query alone', async () => {
    wire.invoke.mockResolvedValue([
      { id: 's1', title: 'Greeting', body: 'Thank you.', created_at: 1, updated_at: 2 },
    ]);
    const listed = await snippetList();
    expect(listed).toEqual<SnippetView[]>([
      { id: 's1', title: 'Greeting', body: 'Thank you.', created_at: 1, updated_at: 2 },
    ]);
    expect(wire.invoke).toHaveBeenCalledWith('snippet_list');

    await snippetSearch('the money one');
    expect(wire.invoke).toHaveBeenLastCalledWith('snippet_search', { query: 'the money one' });
  });

  it('asks the autocomplete with and without the trailing half', async () => {
    wire.invoke.mockResolvedValue('shipped it.');
    const suggestion = await composeAutocomplete('Thanks for the', ' soon.');
    expect(suggestion).toBe('shipped it.');
    expect(wire.invoke).toHaveBeenCalledWith('compose_autocomplete', {
      before: 'Thanks for the',
      after: ' soon.',
    });
    await composeAutocomplete('Thanks for the');
    expect(wire.invoke).toHaveBeenLastCalledWith('compose_autocomplete', {
      before: 'Thanks for the',
    });
  });

  it('translates one message by its reference and the language word', async () => {
    wire.invoke.mockResolvedValue('Corps du message.');
    const translation = await translateMessage(
      { account: 'work', key: 'imap:v1:u42' },
      'french',
    );
    expect(translation).toBe('Corps du message.');
    expect(wire.invoke).toHaveBeenCalledWith('translate', {
      account: 'work',
      key: 'imap:v1:u42',
      lang: 'french',
    });
  });

  it('pins the assist wire literals the Rust side serialises', () => {
    const entry: KnowledgeView = {
      id: 'k1',
      kind: 'preference',
      text: 'The user prefers plain text.',
      source_account: 'work',
      source_key: 'm1',
      created_at: 1,
    };
    expect(JSON.stringify(entry)).toBe(
      '{"id":"k1","kind":"preference","text":"The user prefers plain text.",' +
        '"source_account":"work","source_key":"m1","created_at":1}',
    );
    const summary: ExtractionSummary = {
      extracted: 2,
      skipped_duplicates: 1,
      messages_considered: 3,
    };
    expect(JSON.stringify(summary)).toBe(
      '{"extracted":2,"skipped_duplicates":1,"messages_considered":3}',
    );
    const snippet: SnippetView = {
      id: 's1',
      title: 'Greeting',
      body: 'Thank you for the note.',
      created_at: 1,
      updated_at: 2,
    };
    expect(JSON.stringify(snippet)).toBe(
      '{"id":"s1","title":"Greeting","body":"Thank you for the note.",' +
        '"created_at":1,"updated_at":2}',
    );
    const languages: TranslateLanguage[] = [
      'english',
      'chinese',
      'german',
      'french',
      'japanese',
      'spanish',
    ];
    expect(languages).toHaveLength(6);
  });

  it('generates the digest by the account, the days named or the default implied', async () => {
    wire.invoke.mockResolvedValue({
      period_start: 1,
      period_end: 2,
      content: 'Two replies wait.',
      regenerated: false,
    });
    const view = await digestGenerate('work');
    expect(view).toEqual<DigestView>({
      period_start: 1,
      period_end: 2,
      content: 'Two replies wait.',
      regenerated: false,
    });
    expect(wire.invoke).toHaveBeenCalledWith('digest_generate', { account: 'work' });
    await digestGenerate('work', 3);
    expect(wire.invoke).toHaveBeenLastCalledWith('digest_generate', { account: 'work', days: 3 });
  });

  it('generates the report over the same shape under its own command', async () => {
    wire.invoke.mockResolvedValue({
      period_start: 1,
      period_end: 2,
      content: 'A launch week.',
      regenerated: true,
    });
    const view = await reportGenerate('work', 7);
    expect(view).toEqual<DigestView>({
      period_start: 1,
      period_end: 2,
      content: 'A launch week.',
      regenerated: true,
    });
    expect(wire.invoke).toHaveBeenCalledWith('report_generate', {
      account: 'work',
      days: 7,
    });
  });

  it('reads the latest stored document of one kind', async () => {
    wire.invoke.mockResolvedValue({
      kind: 'digest',
      period_start: 1,
      period_end: 2,
      content: 'All quiet now.',
      created_at: 3,
    });
    const latest = await latestDocument('work', 'digest');
    expect(latest).toEqual<DocumentView>({
      kind: 'digest',
      period_start: 1,
      period_end: 2,
      content: 'All quiet now.',
      created_at: 3,
    });
    expect(wire.invoke).toHaveBeenCalledWith('latest_document', {
      account: 'work',
      kind: 'digest',
    });
    wire.invoke.mockResolvedValue(null);
    expect(await latestDocument('work', 'report')).toBeNull();
  });

  it('pulls the split map by the account alone', async () => {
    wire.invoke.mockResolvedValue([
      { key: 'm1', side: 'actioned' },
      { key: 'm2', side: 'fyi' },
    ]);
    const entries = await splitMap('work');
    expect(entries).toEqual<SplitEntry[]>([
      { key: 'm1', side: 'actioned' },
      { key: 'm2', side: 'fyi' },
    ]);
    expect(wire.invoke).toHaveBeenCalledWith('split_map', { account: 'work' });
  });

  it('pins the documents wire literals the Rust side serialises', () => {
    const generated: DigestView = {
      period_start: 1,
      period_end: 2,
      content: 'All quiet now.',
      regenerated: true,
    };
    expect(JSON.stringify(generated)).toBe(
      '{"period_start":1,"period_end":2,"content":"All quiet now.","regenerated":true}',
    );
    const latest: DocumentView = {
      kind: 'report',
      period_start: 1,
      period_end: 2,
      content: 'A launch week.',
      created_at: 3,
    };
    expect(JSON.stringify(latest)).toBe(
      '{"kind":"report","period_start":1,"period_end":2,"content":"A launch week.","created_at":3}',
    );
    const entry: SplitEntry = { key: 'm1', side: 'fyi' };
    expect(JSON.stringify(entry)).toBe('{"key":"m1","side":"fyi"}');
    const kinds: DocumentKind[] = ['digest', 'report'];
    expect(kinds).toHaveLength(2);
    const sides: SplitSide[] = ['actioned', 'fyi'];
    expect(sides).toHaveLength(2);
  });
});
