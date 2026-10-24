// SPDX-License-Identifier: GPL-3.0-only
// The fixture harness: a drop-in for `@tauri-apps/api/core` and
// `@tauri-apps/api/event` that vite aliases in under `--mode fixtures`
// (`npm run dev:fixtures`), so the real UI runs in a plain browser against
// the recorded snapshots in this directory. Design-review and QA skills
// drive that server; no Tauri host and no live account is involved.
//
// The harness is read-only at rest: pulls resolve with the recorded
// snapshot (a fresh copy each time), and mutating commands settle as null
// and say so once per command. Event injection (the CAND-1 contract) makes
// the other states reachable: a driver calls `setFixtureResponse` to swap
// what a pull answers and `pushFixtureEvent` to emit a fixture event;
// pushes are deep clones delivered in dispatch order, and unlisten
// semantics match the Tauri bridge's. Setting the `__TAURI_INTERNALS__`
// sentinel at module
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
  /* The platform legs' pulls: the diagnostics tail renders empty and the
     default-mail latch counts as asked, so the fixture server shows the
     shell without the ask-once banner. */
  log_tail: [],
  default_mail_asked: true,
  /* The AI read pulls a mounted surface can fire: empty, never null, so a
     chips strip or a suggestion list renders empty instead of crashing on
     a null it never typed. */
  draft_chips: [],
  knowledge_list: [],
  snippet_list: [],
  snippet_search: [],
  people_suggestions: [],
  style_profile: null,
};

const announced = new Set<string>();

/** The `invoke` the aliased core module serves. */
export function invoke<T>(cmd: string, _args?: Record<string, unknown>): Promise<T> {
  if (overrides.has(cmd)) {
    return Promise.resolve(structuredClone(overrides.get(cmd)) as T);
  }
  if (cmd in SNAPSHOTS) {
    return Promise.resolve(structuredClone(SNAPSHOTS[cmd]) as T);
  }
  if (!announced.has(cmd)) {
    announced.add(cmd);
    console.info(`fixture-harness: ${cmd} is not recorded; settled as null`);
  }
  return Promise.resolve(null as T);
}

type FixtureListener = (event: { payload: unknown }) => void;

const listeners = new Map<string, Set<FixtureListener>>();
const overrides = new Map<string, unknown>();

/**
 * The `listen` the aliased event module serves. Inert until a driver pushes
 * a fixture event; the returned unlisten removes the handler, matching the
 * Tauri bridge's cleanup semantics.
 */
export function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<() => void> {
  let set = listeners.get(event);
  if (!set) {
    set = new Set();
    listeners.set(event, set);
  }
  const entry = handler as FixtureListener;
  set.add(entry);
  return Promise.resolve(() => {
    set.delete(entry);
    if (set.size === 0) {
      listeners.delete(event);
    }
  });
}

/**
 * Pushes one fixture event to every listener registered for it, in dispatch
 * order, each receiving a deep clone of the payload (never a reference into
 * the caller's or the recorded JSON).
 */
export function pushFixtureEvent<T>(event: string, payload: T): void {
  const set = listeners.get(event);
  if (!set) {
    return;
  }
  for (const handler of [...set]) {
    handler({ payload: structuredClone(payload) });
  }
}

/**
 * Swaps what one pull command answers, so a driver can walk a surface
 * through its states: set the response, then push the surface_changed event
 * and the app re-pulls the new snapshot.
 */
export function setFixtureResponse(cmd: string, value: unknown): void {
  overrides.set(cmd, value);
}

/** Restores every pull to its recorded answer. */
export function resetFixtures(): void {
  overrides.clear();
}

/* The driver-facing API on the app's own module instance: vite resolves the
   aliased import (`@tauri-apps/api/event`) and a driver's direct path import
   to different instances, so a driver that imports the file by path would
   push into a void. The window handle is the one channel both sides share. */
marker.__ASTRA_FIXTURES__ = { pushFixtureEvent, resetFixtures, setFixtureResponse };
