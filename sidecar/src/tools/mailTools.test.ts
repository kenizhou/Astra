// SPDX-License-Identifier: GPL-3.0-only
/**
 * mailTools unit tests: direct execute() calls against an injected
 * PermissionBridge, with the CLI played by the committed stub
 * (src/fixtures/stub-cli.cjs). safeExec spawns the cliPath binary (node)
 * with cwd pinned at the data dir, so the tests copy the stub under each
 * verb word (folders, messages, drafts) into a temp data dir: node resolves
 * the verb positional as the script file and the stub echoes its argv, the
 * ASTRA_* env subset, and its cwd as JSON, and appends every invocation to
 * stub-captures.jsonl. The argv assertions pin the exact grammar from
 * docs/cli.md: two positionals first, then --json, --correlation-id from
 * the turn, then the verb flags.
 *
 * Module loading follows the kylins pattern: this package is
 * "type": "commonjs" and tests run under `node --test`, so the modules
 * under test are bundled with esbuild into temp CJS files in before() and
 * require()d.
 */
import type { PermissionBridge, PermissionPolicy } from '../permissions/bridge.ts';
import type { MailToolDetails } from './mailTools.ts';

const { test, before, after }: typeof import('node:test') = require('node:test');
const assert: typeof import('node:assert/strict') = require('node:assert/strict');
const esbuild: typeof import('esbuild') = require('esbuild');
const os: typeof import('node:os') = require('node:os');
const path: typeof import('node:path') = require('node:path');
const fs: typeof import('node:fs') = require('node:fs');

interface Tool {
  name: string;
  execute(
    toolCallId: string,
    params: never,
    signal: unknown,
    onUpdate: unknown,
    ctx: unknown,
  ): Promise<{ content: { text: string }[]; details: MailToolDetails }>;
}

interface BridgeIface {
  respond(id: string, approved: boolean, alwaysAllow: boolean): void;
  rejectAll(reason: string): void;
}

type Deps = {
  bridge: PermissionBridge;
  cliPath: () => string | null;
  dataDir?: () => string | null;
  correlationId: () => string | null;
};

let createMailTools: (deps: Deps) => Tool[];
let BridgeCtor: new (opts: {
  policy: () => Record<string, PermissionPolicy>;
  nextId?: () => string;
  write?: (e: unknown) => void;
}) => BridgeIface;
const bundlePaths: string[] = [];
let dataDir: string;

type Env = Record<string, unknown>;
type Capture = { argv: string[]; data_dir: string | null; correlation_id: string | null; cwd: string };

const READS_AUTO: Record<string, PermissionPolicy> = {
  list_folders: { level: 'auto' },
  list_messages: { level: 'auto' },
  read_message: { level: 'auto' },
  save_draft: { level: 'confirm' },
  discard_draft: { level: 'confirm' },
};

before(async () => {
  // Two bundles: mailTools (the unit under test) and bridge (the class the
  // tools gate through, not re-exported by mailTools).
  const mailBundle = path.join(os.tmpdir(), `astra-mailtools-test-${process.pid}.cjs`);
  await esbuild.build({
    entryPoints: [path.resolve(__dirname, 'mailTools.ts')],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    outfile: mailBundle,
    logLevel: 'silent',
  });
  bundlePaths.push(mailBundle);
  createMailTools = (require(mailBundle) as { createMailTools: typeof createMailTools })
    .createMailTools;

  const bridgeBundle = path.join(os.tmpdir(), `astra-mailtools-bridge-${process.pid}.cjs`);
  await esbuild.build({
    entryPoints: [path.resolve(__dirname, '..', 'permissions', 'bridge.ts')],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    outfile: bridgeBundle,
    logLevel: 'silent',
  });
  bundlePaths.push(bridgeBundle);
  BridgeCtor = (require(bridgeBundle) as { PermissionBridge: typeof BridgeCtor })
    .PermissionBridge;

  // The stub data dir: the committed stub copied under each verb word so
  // `node folders ...` / `node messages ...` / `node drafts ...` all resolve
  // it as their script.
  dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'astra-mailtools-data-'));
  const stub = fs.readFileSync(path.resolve(__dirname, '..', 'fixtures', 'stub-cli.cjs'), 'utf8');
  for (const verb of ['folders', 'messages', 'drafts']) {
    fs.writeFileSync(path.join(dataDir, verb), stub);
  }
});

after(() => {
  for (const p of bundlePaths) {
    try {
      fs.rmSync(p, { force: true });
    } catch (e) {
      process.stderr.write(`mailTools.test: failed to remove ${p}: ${String(e)}\n`);
    }
  }
  fs.rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
});

function toolNamed(tools: Tool[], name: string): Tool {
  const t = tools.find((x) => x.name === name);
  assert.ok(t, `tool ${name} must exist`);
  return t;
}

function call(t: Tool, params: unknown, toolCallId = 'call-1') {
  return t.execute(toolCallId, params as never, undefined, undefined, undefined);
}

/** The last stub invocation recorded in stub-captures.jsonl. */
function lastCapture(): Capture {
  const lines = fs
    .readFileSync(path.join(dataDir, 'stub-captures.jsonl'), 'utf8')
    .trim()
    .split('\n');
  assert.ok(lines.length > 0, 'the stub recorded at least one invocation');
  return JSON.parse(lines[lines.length - 1]!) as Capture;
}

function captureCount(): number {
  const p = path.join(dataDir, 'stub-captures.jsonl');
  if (!fs.existsSync(p)) return 0;
  return fs.readFileSync(p, 'utf8').trim().split('\n').filter((l) => l.length > 0).length;
}

function makeTools(policy: Record<string, PermissionPolicy>, sent: Env[] = [], nextId?: () => string): Tool[] {
  return createMailTools({
    bridge: new BridgeCtor({
      policy: () => policy,
      ...(nextId ? { nextId } : {}),
      write: (e) => sent.push(e as Env),
    }) as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
}

test('the five tools ship with the locked names', () => {
  const tools = makeTools({});
  assert.deepEqual(tools.map((t) => t.name), [
    'list_folders',
    'list_messages',
    'read_message',
    'save_draft',
    'discard_draft',
  ]);
});

test('list_folders runs auto and the argv pins the grammar with the turn correlation id', async () => {
  const sent: Env[] = [];
  const tools = makeTools(READS_AUTO, sent);
  const r = await call(toolNamed(tools, 'list_folders'), { account: 'acct-1' });
  assert.equal(r.details.status, 'ok');
  assert.equal(sent.length, 0, 'an auto read never asks');
  const cap = lastCapture();
  assert.deepEqual(cap.argv, [
    'folders',
    'list',
    '--json',
    '--correlation-id',
    'turn-42',
    '--account',
    'acct-1',
  ]);
  assert.equal(cap.correlation_id, 'turn-42', 'ASTRA_CORRELATION_ID rides the spawn');
  assert.equal(cap.data_dir, dataDir, 'ASTRA_DATA_DIR is the data dir');
  assert.equal(cap.cwd, dataDir, 'the child cwd is the data dir');
  // The tool output is the CLI envelope document the stub printed: the
  // stdout echo names the verb script it ran and the argv after it.
  const out = JSON.parse(r.details.output!) as { script: string; argv: string[] };
  assert.equal(out.script, 'folders');
  assert.deepEqual(out.argv.slice(0, 5), ['list', '--json', '--correlation-id', 'turn-42', '--account']);
});

test('list_messages carries the optional folder and limit flags', async () => {
  const tools = makeTools(READS_AUTO);
  const r = await call(toolNamed(tools, 'list_messages'), {
    account: 'acct-1',
    folder: 'inbox',
    limit: 25,
  });
  assert.equal(r.details.status, 'ok');
  assert.deepEqual(lastCapture().argv, [
    'messages',
    'list',
    '--json',
    '--correlation-id',
    'turn-42',
    '--account',
    'acct-1',
    '--folder',
    'inbox',
    '--limit',
    '25',
  ]);
});

test('read_message repeats the --key flag for every key', async () => {
  const tools = makeTools(READS_AUTO);
  const r = await call(toolNamed(tools, 'read_message'), {
    account: 'acct-1',
    keys: ['msg-1', 'msg-2'],
  });
  assert.equal(r.details.status, 'ok');
  assert.deepEqual(lastCapture().argv, [
    'messages',
    'read',
    '--json',
    '--correlation-id',
    'turn-42',
    '--account',
    'acct-1',
    '--key',
    'msg-1',
    '--key',
    'msg-2',
  ]);
});

test('save_draft under confirm: the card asks first and nothing executes while it is open', async () => {
  const sent: Env[] = [];
  let n = 0;
  const bridge = new BridgeCtor({
    policy: () => READS_AUTO,
    nextId: () => `perm-${++n}`,
    write: (e) => sent.push(e as Env),
  });
  const tools = createMailTools({
    bridge: bridge as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
  const before = captureCount();
  const p = call(toolNamed(tools, 'save_draft'), {
    account: 'acct-1',
    from: 'ada@example.com',
    to: ['bob@example.com'],
    subject: 'Re: Friday',
    body: 'See you then.',
  });
  assert.equal(sent.length, 1, 'a confirm-gated write asks before executing');
  const req = sent[0]!;
  assert.equal(req.type, 'permission_request');
  assert.equal(req.id, 'perm-1');
  assert.equal(req.tool_call_id, 'call-1');
  assert.equal(req.tool, 'save_draft');
  assert.equal(req.level, 'confirm');
  assert.equal((req.args as Env).account, 'acct-1');
  assert.equal(captureCount(), before, 'nothing executed while the ask is open');
  bridge.respond('perm-1', false, false);
  const r = await p;
  assert.equal(r.details.status, 'rejected');
  assert.equal(captureCount(), before, 'a rejection still executes nothing');
});

test('save_draft with an owned bridge: approve, then the argv pins --to joins and --plain', async () => {
  const sent: Env[] = [];
  let n = 0;
  const bridge = new BridgeCtor({
    policy: () => READS_AUTO,
    nextId: () => `perm-${++n}`,
    write: (e) => sent.push(e as Env),
  });
  const tools = createMailTools({
    bridge: bridge as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
  const p = call(toolNamed(tools, 'save_draft'), {
    account: 'acct-1',
    from: 'ada@example.com',
    to: ['bob@example.com', 'carol@example.com'],
    cc: ['dan@example.com'],
    subject: 'Re: Friday',
    body: 'See you then.',
  });
  assert.equal(sent.length, 1);
  bridge.respond('perm-1', true, false);
  const r = await p;
  assert.equal(r.details.status, 'ok');
  assert.deepEqual(lastCapture().argv, [
    'drafts',
    'save',
    '--json',
    '--correlation-id',
    'turn-42',
    '--account',
    'acct-1',
    '--from',
    'ada@example.com',
    '--to',
    'bob@example.com,carol@example.com',
    '--cc',
    'dan@example.com',
    '--subject',
    'Re: Friday',
    '--plain',
    'See you then.',
  ]);
});

test('a rejected approval executes nothing', async () => {
  const sent: Env[] = [];
  const bridge = new BridgeCtor({
    policy: () => READS_AUTO,
    nextId: () => 'perm-9',
    write: (e) => sent.push(e as Env),
  });
  const tools = createMailTools({
    bridge: bridge as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
  const before = captureCount();
  const p = call(toolNamed(tools, 'discard_draft'), { account: 'acct-1', key: 'draft-9' });
  assert.equal(sent.length, 1);
  bridge.respond('perm-9', false, false);
  const r = await p;
  assert.equal(r.details.status, 'rejected');
  assert.equal(r.details.output, undefined, 'nothing executed on rejection');
  assert.equal(captureCount(), before, 'no stub invocation landed');
});

test('rejectAll while suspended yields skipped (abort, not error)', async () => {
  const bridge = new BridgeCtor({
    policy: () => READS_AUTO,
    nextId: () => 'perm-5',
    write: () => {},
  });
  const tools = createMailTools({
    bridge: bridge as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
  const p = call(toolNamed(tools, 'save_draft'), {
    account: 'acct-1',
    from: 'a@example.com',
    to: ['b@example.com'],
    subject: 's',
    body: 'b',
  });
  bridge.rejectAll('prompt aborted');
  const r = await p;
  assert.equal(r.details.status, 'skipped');
  assert.match(r.details.output!, /aborted/);
});

test('an empty policy table fails closed: a read asks for approval too', async () => {
  const sent: Env[] = [];
  const bridge = new BridgeCtor({
    policy: () => ({}),
    nextId: () => 'perm-fc',
    write: (e) => sent.push(e as Env),
  });
  const tools = createMailTools({
    bridge: bridge as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
  const p = call(toolNamed(tools, 'list_folders'), { account: 'acct-1' });
  assert.equal(sent.length, 1, 'with no table entry the tool needs approval');
  assert.equal(sent[0]!.tool, 'list_folders');
  bridge.respond('perm-fc', false, false);
  assert.equal((await p).details.status, 'rejected');
});

test('an unconfigured cliPath surfaces as a loud error after approval', async () => {
  const bridge = new BridgeCtor({
    policy: () => READS_AUTO,
    nextId: () => 'perm-cfg',
    write: () => {},
  });
  const tools = createMailTools({
    bridge: bridge as never,
    cliPath: () => null,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
  const p = call(toolNamed(tools, 'save_draft'), {
    account: 'acct-1',
    from: 'a@example.com',
    to: ['b@example.com'],
    subject: 's',
    body: 'b',
  });
  bridge.respond('perm-cfg', true, false);
  const r = await p;
  assert.equal(r.details.status, 'error');
  assert.match(r.details.output!, /cliPath not configured/);
});

test('a missing correlation id omits the flag pair; the CLI mints its own', async () => {
  const tools = createMailTools({
    bridge: new BridgeCtor({ policy: () => READS_AUTO, write: () => {} }) as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => null,
  });
  const r = await call(toolNamed(tools, 'list_folders'), { account: 'acct-1' });
  assert.equal(r.details.status, 'ok');
  const cap = lastCapture();
  assert.deepEqual(cap.argv, ['folders', 'list', '--json', '--account', 'acct-1']);
  assert.equal(cap.correlation_id, null, 'no ASTRA_CORRELATION_ID without a turn id');
});
