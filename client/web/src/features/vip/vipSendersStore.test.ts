// SPDX-License-Identifier: GPL-3.0-only
// The VIP sender store: the optimistic toggle, add/remove/has, the address
// normalisation (case and whitespace folded, the display form lifted to the
// bare addr-spec), the keying by address alone (cross-account), the
// device-local persistence (a malformed blob reads as empty, non-string
// entries drop), the blank-address refusal, and the storage failure that is
// logged, never thrown.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { normaliseAddress, useVipSendersStore } from './vipSendersStore';

const STORAGE_KEY = 'astra.vipSenders';

beforeEach(() => {
  localStorage.clear();
  useVipSendersStore.setState({ vip: new Set() });
});

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

describe('the VIP senders store', () => {
  it('marks an unmarked sender VIP and reports it', () => {
    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(false);
    useVipSendersStore.getState().toggle('ada@example.test');
    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(true);
  });

  it('clears the mark on the second toggle', () => {
    const store = useVipSendersStore.getState();
    store.toggle('ada@example.test');
    store.toggle('ada@example.test');
    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(false);
  });

  it('adds and removes idempotently', () => {
    const store = useVipSendersStore.getState();
    store.add('ada@example.test');
    store.add('ada@example.test');
    expect(useVipSendersStore.getState().vip.size).toBe(1);

    store.remove('ada@example.test');
    store.remove('ada@example.test');
    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(false);
    expect(useVipSendersStore.getState().vip.size).toBe(0);
  });

  it('normalises case and whitespace so spellings agree', () => {
    useVipSendersStore.getState().toggle('  ADA@Example.test ');
    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(true);
    expect([...useVipSendersStore.getState().vip]).toEqual(['ada@example.test']);

    // The same sender under another spelling clears the one mark.
    useVipSendersStore.getState().toggle('Ada@EXAMPLE.test');
    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(false);
  });

  it('lifts the display form to the bare addr-spec', () => {
    useVipSendersStore.getState().add('Ada <ada@example.test>');
    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(true);
    expect([...useVipSendersStore.getState().vip]).toEqual(['ada@example.test']);
  });

  it('keys by address alone: one mark stands for every account', () => {
    useVipSendersStore.getState().add('ada@example.test');
    // The store holds no account half: the same address reads VIP wherever
    // the row sits, unlike the per-account pin set.
    expect(useVipSendersStore.getState().vip.size).toBe(1);
    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(true);
  });

  it('refuses a blank address', () => {
    const store = useVipSendersStore.getState();
    store.toggle('');
    store.toggle('   ');
    store.add('');
    store.remove('  ');
    expect(useVipSendersStore.getState().vip.size).toBe(0);
    expect(useVipSendersStore.getState().has('')).toBe(false);
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
  });

  it('persists every toggle to localStorage', () => {
    useVipSendersStore.getState().toggle('ada@example.test');
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual(['ada@example.test']);

    useVipSendersStore.getState().toggle('bob@example.test');
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual([
      'ada@example.test',
      'bob@example.test',
    ]);

    useVipSendersStore.getState().toggle('ada@example.test');
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual(['bob@example.test']);
  });

  it('hydrates from localStorage on load', async () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(['ada@example.test', 'bob@example.test']));
    vi.resetModules();
    const fresh = await import('./vipSendersStore');
    expect(fresh.useVipSendersStore.getState().has('ada@example.test')).toBe(true);
    expect(fresh.useVipSendersStore.getState().has('bob@example.test')).toBe(true);
    expect(fresh.useVipSendersStore.getState().has('eve@example.test')).toBe(false);
  });

  it('normalises the persisted entries on load', async () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify(['  ADA@Example.test ', 'Ada <ada@example.test>']),
    );
    vi.resetModules();
    const fresh = await import('./vipSendersStore');
    expect([...fresh.useVipSendersStore.getState().vip]).toEqual(['ada@example.test']);
  });

  it('reads a malformed blob as empty', async () => {
    localStorage.setItem(STORAGE_KEY, '{not json');
    vi.resetModules();
    const fresh = await import('./vipSendersStore');
    expect(fresh.useVipSendersStore.getState().vip.size).toBe(0);
  });

  it('reads a non-array blob as empty', async () => {
    localStorage.setItem(STORAGE_KEY, '{"vip":[]}');
    vi.resetModules();
    const fresh = await import('./vipSendersStore');
    expect(fresh.useVipSendersStore.getState().vip.size).toBe(0);
  });

  it('drops the malformed entries and keeps the well-formed ones', async () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify(['ada@example.test', null, 42, '', '   ', { id: 'x' }, ' BOB@example.test ']),
    );
    vi.resetModules();
    const fresh = await import('./vipSendersStore');
    expect([...fresh.useVipSendersStore.getState().vip]).toEqual([
      'ada@example.test',
      'bob@example.test',
    ]);
  });

  it('logs a storage failure and never throws; the toggle still lands', () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('quota');
    });

    useVipSendersStore.getState().toggle('ada@example.test');

    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(true);
    expect(error).toHaveBeenCalled();
  });
});

describe('normaliseAddress', () => {
  it('folds to the bare addr-spec', () => {
    expect(normaliseAddress(' Ada <ADA@Example.test> ')).toBe('ada@example.test');
    expect(normaliseAddress('ADA@Example.test ')).toBe('ada@example.test');
    expect(normaliseAddress('ada@example.test')).toBe('ada@example.test');
  });

  it('reads a blank address as empty', () => {
    expect(normaliseAddress('')).toBe('');
    expect(normaliseAddress('   ')).toBe('');
    expect(normaliseAddress('Ada <>')).toBe('');
  });
});