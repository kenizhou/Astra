// SPDX-License-Identifier: GPL-3.0-only
/**
 * The turn invariant, pinned end to end against the built bundle
 * (dist/index.cjs; run `npm run build` first): one live turn per sidecar.
 * A second prompt while one is live is refused with an honest error
 * envelope BEFORE any single-slot state is touched, so the live turn
 * keeps its budget clock, its correlation id, and its pending approval.
 * Split from index.test.ts for the file-size gate; the harness here is
 * that file's own, trimmed to what this one flow needs.
 */
import type { ChildProcess } from 'node:child_process';

const { test, after }: typeof import('node:test') = require('node:test');
const assert: typeof import('node:assert/strict') = require('node:assert/strict');
const { spawn }: typeof import('node:child_process') = require('node:child_process');
const readline: typeof import('node:readline') = require('node:readline');
const path: typeof import('node:path') = require('node:path');
const fs: typeof import('node:fs') = require('node:fs');
const os: typeof import('node:os') = require('node:os');

const DIST = path.resolve(__dirname, '..', 'dist', 'index.cjs');

interface Spawned {
  child: ChildProcess;
  nextLine: () => Promise<Record<string, unknown>>;
  send: (obj: unknown) => void;
}

const children: ChildProcess[] = [];
const tmpDirs: string[] = [];
after(() => {
  for (const c of children) c.kill();
  for (const d of tmpDirs) {
    try {
      fs.rmSync(d, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
    } catch (e) {
      process.stderr.write(`turn-invariant.test: failed to remove ${d}: ${String(e)}\n`);
    }
  }
});

function spawnSidecar(): Spawned {
  assert.ok(fs.existsSync(DIST), 'dist/index.cjs is missing; run npm run build first');
  const child = spawn(process.execPath, [DIST], { stdio: ['pipe', 'pipe', 'pipe'] });
  children.push(child);
  child.stderr!.on('data', (d: Buffer) => process.stderr.write(`[sidecar] ${d}`));
  const rl = readline.createInterface({ input: child.stdout! });
  const queue: string[] = [];
  const waiters: Array<(line: string) => void> = [];
  rl.on('line', (line: string) => {
    const w = waiters.shift();
    if (w) w(line);
    else queue.push(line);
  });
  const nextLine = (): Promise<Record<string, unknown>> =>
    new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('timed out waiting for stdout line')), 8000);
      const onLine = (line: string): void => {
        clearTimeout(timer);
        resolve(JSON.parse(line) as Record<string, unknown>);
      };
      const queued = queue.shift();
      if (queued !== undefined) onLine(queued);
      else waiters.push(onLine);
    });
  const send = (obj: unknown): void => {
    child.stdin!.write(JSON.stringify(obj) + '\n');
  };
  return { child, nextLine, send };
}

/** A temp data dir with the stub CLI installed under the verb words. */
function stubDataDir(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'astra-sidecar-invariant-'));
  tmpDirs.push(dir);
  const stub = fs.readFileSync(path.resolve(__dirname, 'fixtures', 'stub-cli.cjs'), 'utf8');
  for (const verb of ['folders', 'messages', 'drafts']) {
    fs.writeFileSync(path.join(dir, verb), stub);
  }
  return dir;
}

/** A mock transcript fixture carrying a confirm-gated tool call, optionally
 *  holding each session's creation open for the given delay. */
function toolCallFixture(
  dir: string,
  name: string,
  args: unknown,
  createSessionDelayMs = 0,
): string {
  const fixturePath = path.join(dir, 'fixture.json');
  fs.writeFileSync(
    fixturePath,
    JSON.stringify({
      chunks: ['working on it'],
      chunk_delay_ms: 1,
      tool_call: { name, args },
      create_session_delay_ms: createSessionDelayMs,
    }),
  );
  return fixturePath;
}

async function until(
  s: Spawned,
  type: string,
  seen: string[],
): Promise<Record<string, unknown>> {
  for (;;) {
    const env = await s.nextLine();
    seen.push(
      env.type === 'event' ? `event:${(env.event as { kind: string }).kind}` : String(env.type),
    );
    if (env.type === type) return env;
  }
}

test('a second prompt while one is live is refused and never disturbs the live turn', async () => {
  const dir = stubDataDir();
  const fixture = toolCallFixture(dir, 'save_draft', {
    account: 'acct-1',
    from: 'ada@example.com',
    to: ['bob@example.com'],
    subject: 'Re: Friday',
    body: 'See you then.',
  });
  const s = spawnSidecar();
  s.send({
    type: 'init',
    harness: 'mock',
    cli_path: process.execPath,
    data_dir: dir,
    mock_fixture: fixture,
    policy: { save_draft: { level: 'confirm' } },
  });
  await s.nextLine();
  // The first turn suspends on its approval ask, so a second prompt from
  // another chat's session is live at the same time.
  s.send({
    type: 'prompt',
    id: 'turn-live',
    session_id: 'agent:chat:a',
    text: 'draft a reply',
    correlation_id: 'corr-live-1',
  });
  const seen: string[] = [];
  const req = await until(s, 'permission_request', seen);
  assert.equal(req.tool, 'save_draft');

  // The overlapping prompt from a second chat's session: refused with an
  // honest error envelope naming why, never a silent queue.
  s.send({
    type: 'prompt',
    id: 'turn-second',
    session_id: 'agent:chat:b',
    text: 'another question',
    correlation_id: 'corr-second-2',
  });
  const refusal = await until(s, 'error', seen);
  assert.equal(refusal.id, 'turn-second');
  assert.match(String(refusal.message), /live/i, 'the refusal names the live turn');

  // The live turn's pending approval was not bounced and its budget view
  // was not reset: answering the ask still runs the tool, the ledger row
  // still carries the first turn's correlation id, and the turn lands
  // done.
  s.send({ type: 'permission_response', id: req.id, approved: true, always_allow: false });
  const tr = await until(s, 'tool_result', seen);
  assert.equal(tr.status, 'ok', `the live turn kept its approval: ${seen.join(',')}`);
  const llm = await until(s, 'llm_call', seen);
  assert.equal(
    (llm.call as Record<string, unknown>).correlation_id,
    'corr-live-1',
    'the live turn keeps its correlation id',
  );
  const done = await until(s, 'done', seen);
  assert.equal(done.id, 'turn-live');

  // The refusal never wedged the runner: the next prompt runs and asks.
  s.send({
    type: 'prompt',
    id: 'turn-after',
    session_id: 'agent:chat:b',
    text: 'try again',
    correlation_id: 'corr-after-3',
  });
  const seenAfter: string[] = [];
  const ask = await until(s, 'permission_request', seenAfter);
  s.send({ type: 'permission_response', id: ask.id, approved: false, always_allow: false });
  const doneAfter = await until(s, 'done', seenAfter);
  assert.equal(doneAfter.id, 'turn-after');
});

test('a prompt arriving inside another prompt\'s session-creation window is refused', async () => {
  const dir = stubDataDir();
  // The slow creation holds every session open for a moment, so the second
  // prompt arrives while the first is still inside createSession: exactly
  // the window where a gate that checks before creating lets both through.
  const fixture = toolCallFixture(
    dir,
    'save_draft',
    {
      account: 'acct-1',
      from: 'ada@example.com',
      to: ['bob@example.com'],
      subject: 'Re: Friday',
      body: 'See you then.',
    },
    250,
  );
  const s = spawnSidecar();
  s.send({
    type: 'init',
    harness: 'mock',
    cli_path: process.execPath,
    data_dir: dir,
    mock_fixture: fixture,
    policy: { save_draft: { level: 'confirm' } },
  });
  await s.nextLine();
  s.send({
    type: 'prompt',
    id: 'turn-window-a',
    session_id: 'agent:chat:win-a',
    text: 'draft a reply',
    correlation_id: 'corr-win-1',
  });
  // The second prompt, for a session that also needs creating, arrives
  // inside the first prompt's creation window.
  s.send({
    type: 'prompt',
    id: 'turn-window-b',
    session_id: 'agent:chat:win-b',
    text: 'meanwhile',
    correlation_id: 'corr-win-2',
  });
  const seen: string[] = [];
  const refusal = await until(s, 'error', seen);
  assert.equal(refusal.id, 'turn-window-b');
  assert.match(String(refusal.message), /live/i, 'the refusal names the live turn');

  // The first turn keeps its correlation id and its budget: its ask
  // answers, its ledger row and its done all carry its own turn.
  const req = await until(s, 'permission_request', seen);
  s.send({ type: 'permission_response', id: req.id, approved: true, always_allow: false });
  const llm = await until(s, 'llm_call', seen);
  assert.equal(
    (llm.call as Record<string, unknown>).correlation_id,
    'corr-win-1',
    'the live turn keeps its correlation id through the window',
  );
  const done = await until(s, 'done', seen);
  assert.equal(done.id, 'turn-window-a');
});
