// SPDX-License-Identifier: GPL-3.0-only
// The fixture harness: a drop-in for `@tauri-apps/api/core` and
// `@tauri-apps/api/event` that vite aliases in under `--mode fixtures`
// (`npm run dev:fixtures`), so the real UI runs in a plain browser against
// the recorded snapshots in this directory. Design-review and QA skills
// drive that server; no Tauri host and no live account is involved.
//
// The harness is read-only by contract: pulls resolve with the recorded
// snapshot (a fresh copy each time), mutating commands settle as null and
// say so once per command, and no event ever fires, so every surface shows
// its at-rest state. Setting the `__TAURI_INTERNALS__` sentinel at module
// load is what lets the hydration path run: surfaceEvents reads it as its
// Tauri presence guard.

import calendarSnapshot from './calendar_snapshot.json';
import mailboxListSnapshot from './mailbox_list.json';
import mcpStatusSnapshot from './mcp_status.json';
import peopleSnapshot from './people_snapshot.json';
import settingsSnapshot from './settings_snapshot.json';

const marker = window as unknown as Record<string, unknown>;
if (!('__TAURI_INTERNALS__' in marker)) {
  marker.__TAURI_INTERNALS__ = {};
}

/** The recorded answer for every pull a surface makes at rest. */
const SNAPSHOTS: Record<string, unknown> = {
  mailbox_list: mailboxListSnapshot,
  sync_progress: { accounts: [] },
  reading: null,
  draft_status: null,
  compose_request: null,
  sending: { rows: [] },
  calendar: calendarSnapshot,
  people: peopleSnapshot,
  settings: settingsSnapshot,
  mcp_status: mcpStatusSnapshot,
  agent_status: { state: 'idle', model: null },
  split_map: [],
};

const announced = new Set<string>();

/** The `invoke` the aliased core module serves. */
export function invoke<T>(cmd: string, _args?: Record<string, unknown>): Promise<T> {
  if (cmd in SNAPSHOTS) {
    return Promise.resolve(structuredClone(SNAPSHOTS[cmd]) as T);
  }
  if (!announced.has(cmd)) {
    announced.add(cmd);
    console.info(`fixture-harness: ${cmd} is not recorded; settled as null`);
  }
  return Promise.resolve(null as T);
}

/** The `listen` the aliased event module serves: inert, no event ever fires. */
export function listen<T>(
  _event: string,
  _handler: (event: { payload: T }) => void,
): Promise<() => void> {
  return Promise.resolve(() => undefined);
}
