// SPDX-License-Identifier: GPL-3.0-only
// The fixture harness's contract: every pull the shell's hydration makes
// resolves with the snapshot the surface renders, mutations settle quietly,
// and no event ever fires, so a plain browser shows the real UI at rest.
import { describe, expect, it, vi } from 'vitest';

import calendarFixture from './calendar_snapshot.json';
import cjkThread from './cjk_thread.json';
import { invoke, listen, pushFixtureEvent, resetFixtures, setFixtureResponse } from './harness';
import mailboxListFixture from './mailbox_list.json';
import mcpStatusFixture from './mcp_status.json';
import peopleFixture from './people_snapshot.json';
import settingsFixture from './settings_snapshot.json';

describe('fixture harness', () => {
  it('marks the window as Tauri so the hydration path runs', () => {
    expect('__TAURI_INTERNALS__' in window).toBe(true);
  });

  it('serves the recorded snapshots for the hydrated surfaces', async () => {
    await expect(invoke('mailbox_list')).resolves.toEqual(mailboxListFixture);
    await expect(invoke('calendar')).resolves.toEqual(calendarFixture);
    await expect(invoke('people')).resolves.toEqual(peopleFixture);
    await expect(invoke('settings')).resolves.toEqual(settingsFixture);
    await expect(invoke('mcp_status')).resolves.toEqual(mcpStatusFixture);
  });

  it('hands a fresh copy on every pull, so a surface mutating its snapshot never poisons the next pull', async () => {
    const first = (await invoke('mailbox_list')) as { account_folders: unknown[] };
    first.account_folders.length = 0;
    const second = (await invoke('mailbox_list')) as { account_folders: unknown[] };
    expect(second.account_folders).toHaveLength(mailboxListFixture.account_folders.length);
  });

  it('answers the surfaces at rest with their empty shapes', async () => {
    await expect(invoke('sync_progress')).resolves.toEqual({ accounts: [] });
    await expect(invoke('sending')).resolves.toEqual({ rows: [] });
    await expect(invoke('split_map', { account: 'work' })).resolves.toEqual([]);
    await expect(invoke('agent_status')).resolves.toEqual({ state: 'idle', model: null });
  });

  it('answers the nullable pulls with null', async () => {
    await expect(invoke('reading')).resolves.toBeNull();
    await expect(invoke('draft_status')).resolves.toBeNull();
    await expect(invoke('compose_request')).resolves.toBeNull();
  });

  it('settles an unrecorded command as null and says so once per command', async () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => undefined);
    await expect(invoke('dispatch', { intent: 'refresh_mail' })).resolves.toBeNull();
    await expect(invoke('dispatch', { intent: 'refresh_mail' })).resolves.toBeNull();
    const mentions = info.mock.calls.filter((args) => String(args[0]).includes('dispatch'));
    expect(mentions).toHaveLength(1);
    info.mockRestore();
  });

  it('registers listeners as inert: an unlisten comes back and no event fires', async () => {
    const handler = vi.fn();
    const unlisten = await listen('surface_changed', handler);
    expect(typeof unlisten).toBe('function');
    expect(() => unlisten()).not.toThrow();
    expect(handler).not.toHaveBeenCalled();
  });
});

describe('fixture harness event injection', () => {
  it('delivers a pushed event to every registered listener, deep cloned', async () => {
    const received: unknown[] = [];
    const unA = await listen('surface_changed', (e) => received.push(e.payload));
    const unB = await listen('surface_changed', (e) => received.push(e.payload));
    const payload = { surface: 'reading' };
    pushFixtureEvent('surface_changed', payload);
    payload.surface = 'mailbox_list'; // mutating the caller's object after the push
    expect(received).toEqual([{ surface: 'reading' }, { surface: 'reading' }]);
    unA();
    unB();
    pushFixtureEvent('surface_changed', { surface: 'calendar' });
    expect(received).toHaveLength(2); // both unlistened: no delivery
  });

  it('applies pushes in dispatch order', async () => {
    const order: string[] = [];
    const un = await listen('surface_changed', (e) => order.push((e.payload as { surface: string }).surface));
    pushFixtureEvent('surface_changed', { surface: 'reading' });
    pushFixtureEvent('surface_changed', { surface: 'calendar' });
    pushFixtureEvent('surface_changed', { surface: 'people' });
    expect(order).toEqual(['reading', 'calendar', 'people']);
    un();
  });

  it('swaps a pull answer with setFixtureResponse, and resetFixtures restores it', async () => {
    await expect(invoke('reading')).resolves.toBeNull();
    setFixtureResponse('reading', { subject: 'swapped' });
    await expect(invoke('reading')).resolves.toEqual({ subject: 'swapped' });
    resetFixtures();
    await expect(invoke('reading')).resolves.toBeNull();
  });

  it('serves the CJK thread for the bi-font verification', async () => {
    setFixtureResponse('reading', cjkThread);
    const snapshot = (await invoke('reading')) as { subject: string; plain: string };
    expect(snapshot.subject).toBe('季度报告');
    expect(snapshot.plain).toContain('董事会');
    resetFixtures();
  });
});

describe('the production bundle never carries the harness', () => {
  it('aliases the Tauri bridge only in fixtures mode', async () => {
    const { tauriBridgeAliases } = await import('../../../fixture-alias');
    expect(tauriBridgeAliases('fixtures', '/cfg')['@tauri-apps/api/core']).toContain('harness.ts');
    expect(tauriBridgeAliases('fixtures', '/cfg')['@tauri-apps/api/event']).toContain('harness.ts');
    expect(tauriBridgeAliases('production', '/cfg')).toEqual({});
    expect(tauriBridgeAliases('test', '/cfg')).toEqual({});
  });
});

describe('mounted AI surfaces never read null', () => {
  it('answers the AI read pulls with their empty shapes, so a chips strip renders empty instead of crashing', async () => {
    await expect(invoke('draft_chips')).resolves.toEqual([]);
    await expect(invoke('knowledge_list')).resolves.toEqual([]);
    await expect(invoke('snippet_list')).resolves.toEqual([]);
    await expect(invoke('snippet_search')).resolves.toEqual([]);
    await expect(invoke('people_suggestions')).resolves.toEqual([]);
    await expect(invoke('style_profile')).resolves.toBeNull();
  });
});

describe('the driver API', () => {
  it('lives on the window, so a driver shares the app instance', async () => {
    const api = (window as unknown as Record<string, { pushFixtureEvent: typeof pushFixtureEvent }>).__ASTRA_FIXTURES__;
    if (!api) {
      throw new Error('the fixture API is missing from the window');
    }
    expect(typeof api.pushFixtureEvent).toBe('function');
    const received: unknown[] = [];
    const un = await listen('surface_changed', (e) => received.push(e.payload));
    api.pushFixtureEvent('surface_changed', { surface: 'people' });
    expect(received).toEqual([{ surface: 'people' }]);
    un();
  });
});
