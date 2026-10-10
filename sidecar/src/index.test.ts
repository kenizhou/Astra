// SPDX-License-Identifier: GPL-3.0-only
/**
 * End-to-end tests against the built bundle (dist/index.cjs): run
 * `npm run build` first. Spawns the sidecar exactly as the Rust host will
 * and speaks JSONL over stdin/stdout, driven by the MockHarness fixture
 * transcripts (no model, no credentials). The permission-flow tests mount
 * the committed stub CLI (src/fixtures/stub-cli.cjs) through the init
 * envelope's cli_path and data_dir, so the whole loop is exercised: prompt
 * with a correlation id, fixture tool_call, permission_request,
 * permission_response, safeExec into the stub, tool_result, done.
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
      process.stderr.write(`index.test: failed to remove ${d}: ${String(e)}\n`);
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
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'astra-sidecar-e2e-'));
  tmpDirs.push(dir);
  const stub = fs.readFileSync(path.resolve(__dirname, 'fixtures', 'stub-cli.cjs'), 'utf8');
  for (const verb of ['folders', 'messages', 'drafts']) {
    fs.writeFileSync(path.join(dir, verb), stub);
  }
  return dir;
}

/** A mock transcript fixture carrying a tool call; returns its path. */
function toolCallFixture(dir: string, name: string, args: unknown): string {
  const fixturePath = path.join(dir, 'fixture.json');
  fs.writeFileSync(
    fixturePath,
    JSON.stringify({
      chunks: ['working on it'],
      chunk_delay_ms: 1,
      tool_call: { name, args },
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

test('init with the mock harness answers ready with the mock model', async () => {
  const s = spawnSidecar();
  s.send({ type: 'init', harness: 'mock', cli_path: '', data_dir: stubDataDir() });
  const ready = await s.nextLine();
  assert.equal(ready.type, 'ready');
  assert.deepEqual(ready.models, ['mock-1']);
  assert.deepEqual(ready.harnesses, ['mock', 'pi']);
});

test('init with an unknown harness answers an error envelope, not ready', async () => {
  const s = spawnSidecar();
  s.send({ type: 'init', harness: 'teleport', cli_path: '', data_dir: stubDataDir() });
  const e = await s.nextLine();
  assert.equal(e.type, 'error');
  assert.match(String(e.message), /unknown harness/);
});

test('a prompt streams text deltas, one llm_call, then done with the id', async () => {
  const dir = stubDataDir();
  const s = spawnSidecar();
  s.send({
    type: 'init',
    harness: 'mock',
    cli_path: '',
    data_dir: dir,
    mock_fixture: path.resolve(__dirname, 'fixtures', 'mock-reply.json'),
  });
  await s.nextLine(); // ready
  s.send({
    type: 'prompt',
    id: 'turn-1',
    session_id: 'agent:main',
    text: 'hello',
    correlation_id: 'corr-e2e-1',
  });
  let deltas = 0;
  let llm: Record<string, unknown> | null = null;
  for (;;) {
    const env = await s.nextLine();
    if (env.type === 'event' && (env.event as { kind: string }).kind === 'text_delta') deltas += 1;
    if (env.type === 'llm_call') llm = env.call as Record<string, unknown>;
    if (env.type === 'done') {
      assert.equal(env.id, 'turn-1');
      break;
    }
  }
  assert.ok(deltas >= 1, 'expected at least one text_delta');
  assert.ok(llm, 'expected exactly one llm_call envelope');
  assert.equal(llm!.session_id, 'agent:main');
  assert.equal(llm!.prompt_id, 'turn-1');
  assert.equal(llm!.correlation_id, 'corr-e2e-1', 'the turn correlation id rides the ledger row');
  assert.equal(llm!.caller, 'agent-chat');
  assert.equal(llm!.status, 'ok');
});

test('abort mid prompt still ends with done (the UI never hangs)', async () => {
  const s = spawnSidecar();
  s.send({ type: 'init', harness: 'mock', cli_path: '', data_dir: stubDataDir() });
  await s.nextLine();
  s.send({ type: 'prompt', id: 'turn-a', session_id: 'agent:main', text: 'long reply please', correlation_id: 'c-a' });
  s.send({ type: 'abort', id: 'turn-a', session_id: 'agent:main' });
  for (;;) {
    const env = await s.nextLine();
    if (env.type === 'done') {
      assert.equal(env.id, 'turn-a');
      break;
    }
  }
});

test('a garbage line is skipped and later commands still work', async () => {
  const s = spawnSidecar();
  s.child.stdin!.write('this is not json{{{\n');
  s.send({ type: 'init', harness: 'mock', cli_path: '', data_dir: stubDataDir() });
  const ready = await s.nextLine();
  assert.equal(ready.type, 'ready');
  s.send({ type: 'command', id: 'cmd-1', name: 'ping', args: {} });
  const r = await s.nextLine();
  assert.equal(r.type, 'result');
  assert.equal(r.id, 'cmd-1');
  assert.equal((r.value as Record<string, unknown>).ok, true);
});

test('an unknown command answers an error envelope with the id; ping answers result', async () => {
  const s = spawnSidecar();
  s.send({ type: 'init', harness: 'mock', cli_path: '', data_dir: stubDataDir() });
  await s.nextLine();
  s.send({ type: 'command', id: 'cmd-x', name: 'nope', args: {} });
  const e = await s.nextLine();
  assert.equal(e.type, 'error');
  assert.equal(e.id, 'cmd-x');
  assert.match(String(e.message), /unknown command/);
  // the sidecar stays alive
  s.send({ type: 'command', id: 'cmd-2', name: 'ping', args: {} });
  const r = await s.nextLine();
  assert.equal(r.type, 'result');
  assert.equal(r.id, 'cmd-2');
});

test('a prompt before init answers an error envelope carrying the id', async () => {
  const s = spawnSidecar();
  s.send({ type: 'prompt', id: 'turn-early', session_id: 'agent:main', text: 'hi', correlation_id: 'c' });
  const e = await s.nextLine();
  assert.equal(e.type, 'error');
  assert.equal(e.id, 'turn-early');
  assert.match(String(e.message), /before init/);
});

test('an unhandled rejection answers an error envelope and exits 1', async () => {
  const s = spawnSidecar();
  const exited = new Promise((resolve) => s.child.on('exit', resolve));
  s.send({
    type: 'init',
    harness: 'mock',
    cli_path: '',
    data_dir: stubDataDir(),
    mock_fixture: 'reject-create-session',
  });
  await s.nextLine(); // ready
  s.send({ type: 'prompt', id: 'turn-boom', session_id: 'agent:main', text: 'boom', correlation_id: 'c' });
  const err = await s.nextLine();
  assert.equal(err.type, 'error');
  assert.match(String(err.message), /createSession rejected|unhandled rejection/);
  assert.equal(await exited, 1);
});

test('permission flow end to end: ask, approve, stub CLI run, correlation id everywhere', async () => {
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
    policy: {
      list_folders: { level: 'auto' },
      list_messages: { level: 'auto' },
      read_message: { level: 'auto' },
      save_draft: { level: 'confirm' },
      discard_draft: { level: 'confirm' },
    },
  });
  await s.nextLine(); // ready
  s.send({
    type: 'prompt',
    id: 'turn-7',
    session_id: 'agent:main',
    text: 'file that reply',
    correlation_id: 'corr-wire-9',
  });

  const seen: string[] = [];
  const req = await until(s, 'permission_request', seen);
  assert.deepEqual(seen.slice(0, -1), ['event:state', 'event:text_delta', 'event:tool_call']);
  assert.equal(req.tool, 'save_draft');
  assert.equal(req.level, 'confirm');
  assert.equal(typeof req.tool_call_id, 'string');
  assert.match(String(req.id), /^perm-\d+-1$/);

  s.send({ type: 'permission_response', id: req.id, approved: true, always_allow: false });
  const tr = await until(s, 'tool_result', seen);
  assert.equal(tr.tool_call_id, req.tool_call_id);
  assert.equal(tr.tool, 'save_draft');
  assert.equal(tr.status, 'ok');
  // The stub's echo rides the tool_result output: the verb script it ran,
  // the argv after it carrying the turn correlation id, the same id the
  // llm_call row carries.
  const out = JSON.parse(String(tr.output)) as {
    script: string;
    argv: string[];
    correlation_id: string | null;
  };
  assert.equal(out.script, 'drafts');
  assert.deepEqual(out.argv.slice(0, 4), ['save', '--json', '--correlation-id', 'corr-wire-9']);
  assert.equal(out.correlation_id, 'corr-wire-9');
  const done = await until(s, 'done', seen);
  assert.equal(done.id, 'turn-7');
  // The audit capture in the data dir ties the invocation to the same turn.
  const captures = fs
    .readFileSync(path.join(dir, 'stub-captures.jsonl'), 'utf8')
    .trim()
    .split('\n')
    .map((l) => JSON.parse(l) as { correlation_id: string | null; cwd: string });
  assert.equal(captures.length, 1);
  assert.equal(captures[0]!.correlation_id, 'corr-wire-9');
  assert.equal(captures[0]!.cwd, dir);
});

test('permission flow: a rejected approval answers tool_result rejected and no output', async () => {
  const dir = stubDataDir();
  const fixture = toolCallFixture(dir, 'discard_draft', {
    account: 'acct-1',
    key: 'draft-9',
  });
  const s = spawnSidecar();
  s.send({
    type: 'init',
    harness: 'mock',
    cli_path: process.execPath,
    data_dir: dir,
    mock_fixture: fixture,
    policy: { discard_draft: { level: 'confirm' } },
  });
  await s.nextLine();
  s.send({
    type: 'prompt',
    id: 'turn-8',
    session_id: 'agent:main',
    text: 'bin it',
    correlation_id: 'corr-wire-10',
  });
  const seen: string[] = [];
  const req = await until(s, 'permission_request', seen);
  s.send({ type: 'permission_response', id: req.id, approved: false, always_allow: false });
  const tr = await until(s, 'tool_result', seen);
  assert.equal(tr.tool, 'discard_draft');
  assert.equal(tr.status, 'rejected');
  assert.ok(!('output' in tr), 'a rejected call never executed, no output');
  const done = await until(s, 'done', seen);
  assert.equal(done.id, 'turn-8');
});

test('an allowlisted policy row skips the permission ask entirely', async () => {
  const dir = stubDataDir();
  const fixture = toolCallFixture(dir, 'list_folders', { account: 'acct-1' });
  const s = spawnSidecar();
  s.send({
    type: 'init',
    harness: 'mock',
    cli_path: process.execPath,
    data_dir: dir,
    mock_fixture: fixture,
    policy: { list_folders: { level: 'confirm', allowlist: true } },
  });
  await s.nextLine();
  s.send({
    type: 'prompt',
    id: 'turn-9',
    session_id: 'agent:main',
    text: 'show the folders',
    correlation_id: 'corr-wire-11',
  });
  const seen: string[] = [];
  const tr = await until(s, 'tool_result', seen);
  assert.ok(!seen.includes('permission_request'), `allowlist skips the ask, saw ${seen.join(',')}`);
  assert.equal(tr.status, 'ok');
  await until(s, 'done', seen);
});
