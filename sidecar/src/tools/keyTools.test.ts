// SPDX-License-Identifier: GPL-3.0-only
/**
 * keyTools unit tests: direct execute() calls against an injected
 * PermissionBridge, with the CLI played by the committed stub
 * (src/fixtures/stub-cli.cjs) copied under the `keys` verb word into a
 * temp data dir, the mailTools.test.ts pattern. The argv assertions pin
 * the exact grammar from docs/cli.md: two positionals first, then --json,
 * --correlation-id from the turn, then the verb flags. The five key tools
 * gate exactly as the gating map says: the two reads run auto, the import
 * and the two review actions ask at confirm.
 *
 * Module loading follows the kylins pattern: this package is
 * "type": "commonjs" and tests run under `node --test`, so the modules
 * under test are bundled with esbuild into temp CJS files in before() and
 * require()d.
 */
import type { PermissionBridge, PermissionPolicy } from '../permissions/bridge.ts';
import type { MailToolDetails } from './gated.ts';
import type { BudgetView } from '../budget.ts';

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
  budget?: () => BudgetView | null;
};

let createKeyTools: (deps: Deps) => Tool[];
let BridgeCtor: new (opts: {
  policy: () => Record<string, PermissionPolicy>;
  nextId?: () => string;
  write?: (e: unknown) => void;
}) => BridgeIface;
const bundlePaths: string[] = [];
let dataDir: string;

type Env = Record<string, unknown>;
type Capture = { argv: string[]; data_dir: string | null; correlation_id: string | null; cwd: string };

/** The five policy rows exactly as the host's default_policy ships them:
 *  the two reads run, the import and the review actions ask. */
const POLICY: Record<string, PermissionPolicy> = {
  list_keys: { level: 'auto' },
  list_collected_keys: { level: 'auto' },
  import_key: { level: 'confirm' },
  promote_key: { level: 'confirm' },
  reject_key: { level: 'confirm' },
};

before(async () => {
  const keyBundle = path.join(os.tmpdir(), `astra-keytools-test-${process.pid}.cjs`);
  await esbuild.build({
    entryPoints: [path.resolve(__dirname, 'keyTools.ts')],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    outfile: keyBundle,
    logLevel: 'silent',
  });
  bundlePaths.push(keyBundle);
  createKeyTools = (require(keyBundle) as { createKeyTools: typeof createKeyTools }).createKeyTools;

  const bridgeBundle = path.join(os.tmpdir(), `astra-keytools-bridge-${process.pid}.cjs`);
  await esbuild.build({
    entryPoints: [path.resolve(__dirname, '..', 'permissions', 'bridge.ts')],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    outfile: bridgeBundle,
    logLevel: 'silent',
  });
  bundlePaths.push(bridgeBundle);
  BridgeCtor = (require(bridgeBundle) as { PermissionBridge: typeof BridgeCtor }).PermissionBridge;

  dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'astra-keytools-data-'));
  const stub = fs.readFileSync(path.resolve(__dirname, '..', 'fixtures', 'stub-cli.cjs'), 'utf8');
  fs.writeFileSync(path.join(dataDir, 'keys'), stub);
});

after(() => {
  for (const p of bundlePaths) {
    try {
      fs.rmSync(p, { force: true });
    } catch (e) {
      process.stderr.write(`keyTools.test: failed to remove ${p}: ${String(e)}\n`);
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

function makeTools(
  policy: Record<string, PermissionPolicy>,
  sent: Env[] = [],
  budget?: () => BudgetView | null,
): Tool[] {
  return createKeyTools({
    bridge: new BridgeCtor({
      policy: () => policy,
      write: (e) => sent.push(e as Env),
    }) as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
    ...(budget ? { budget } : {}),
  });
}

test('the five key tools ship with the locked names', () => {
  const tools = makeTools({});
  assert.deepEqual(tools.map((t) => t.name), [
    'list_keys',
    'list_collected_keys',
    'import_key',
    'promote_key',
    'reject_key',
  ]);
});

test('list_keys runs auto and the argv pins the grammar with the turn correlation id', async () => {
  const sent: Env[] = [];
  const tools = makeTools(POLICY, sent);
  const r = await call(toolNamed(tools, 'list_keys'), { account: 'acct-1' });
  assert.equal(r.details.status, 'ok');
  assert.equal(sent.length, 0, 'an auto read never asks');
  const cap = lastCapture();
  assert.deepEqual(cap.argv, [
    'keys',
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
});

test('list_collected_keys runs auto with the collected subverb', async () => {
  const sent: Env[] = [];
  const tools = makeTools(POLICY, sent);
  const r = await call(toolNamed(tools, 'list_collected_keys'), { account: 'acct-1' });
  assert.equal(r.details.status, 'ok');
  assert.equal(sent.length, 0);
  assert.deepEqual(lastCapture().argv, [
    'keys',
    'collected',
    '--json',
    '--correlation-id',
    'turn-42',
    '--account',
    'acct-1',
  ]);
});

test('import_key under confirm: the card asks first and nothing executes while it is open', async () => {
  const sent: Env[] = [];
  const bridge = new BridgeCtor({
    policy: () => POLICY,
    nextId: () => 'perm-1',
    write: (e) => sent.push(e as Env),
  });
  const tools = createKeyTools({
    bridge: bridge as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
  const before = captureCount();
  const p = call(toolNamed(tools, 'import_key'), { account: 'acct-1', material: 'cert text' });
  assert.equal(sent.length, 1, 'a confirm-gated write asks before executing');
  const req = sent[0]!;
  assert.equal(req.type, 'permission_request');
  assert.equal(req.id, 'perm-1');
  assert.equal(req.tool_call_id, 'call-1');
  assert.equal(req.tool, 'import_key');
  assert.equal(req.level, 'confirm');
  assert.equal((req.args as Env).account, 'acct-1');
  assert.equal((req.args as Env).material, 'cert text');
  assert.equal(captureCount(), before, 'nothing executed while the ask is open');
  bridge.respond('perm-1', false, false);
  const r = await p;
  assert.equal(r.details.status, 'rejected');
  assert.equal(captureCount(), before, 'a rejection still executes nothing');
});

test('import_key approved: the argv carries --plain and the optional --standard', async () => {
  const sent: Env[] = [];
  const bridge = new BridgeCtor({
    policy: () => POLICY,
    nextId: () => 'perm-1',
    write: (e) => sent.push(e as Env),
  });
  const tools = createKeyTools({
    bridge: bridge as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
  const p = call(toolNamed(tools, 'import_key'), {
    account: 'acct-1',
    material: 'armored cert',
    standard: 'openpgp',
  });
  assert.equal(sent.length, 1);
  assert.equal((sent[0]!.args as Env).standard, 'openpgp');
  bridge.respond('perm-1', true, false);
  const r = await p;
  assert.equal(r.details.status, 'ok');
  assert.deepEqual(lastCapture().argv, [
    'keys',
    'import',
    '--json',
    '--correlation-id',
    'turn-42',
    '--account',
    'acct-1',
    '--plain',
    'armored cert',
    '--standard',
    'openpgp',
  ]);
});

test('promote_key approved: the argv pins the fingerprint pair, the email, and the verified switch', async () => {
  const sent: Env[] = [];
  const bridge = new BridgeCtor({
    policy: () => POLICY,
    nextId: () => 'perm-1',
    write: (e) => sent.push(e as Env),
  });
  const tools = createKeyTools({
    bridge: bridge as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
  const p = call(toolNamed(tools, 'promote_key'), {
    account: 'acct-1',
    standard: 'openpgp',
    fingerprint: 'ABCD',
    email: 'peer@example.com',
    verified: true,
  });
  assert.equal(sent.length, 1);
  assert.equal(sent[0]!.level, 'confirm');
  assert.equal((sent[0]!.args as Env).verified, true);
  bridge.respond('perm-1', true, false);
  const r = await p;
  assert.equal(r.details.status, 'ok');
  assert.deepEqual(lastCapture().argv, [
    'keys',
    'promote',
    '--json',
    '--correlation-id',
    'turn-42',
    '--account',
    'acct-1',
    '--standard',
    'openpgp',
    '--fingerprint',
    'ABCD',
    '--email',
    'peer@example.com',
    '--verified',
  ]);
});

test('promote_key without the optionals omits the email and the verified switch', async () => {
  const sent: Env[] = [];
  const bridge = new BridgeCtor({
    policy: () => POLICY,
    nextId: () => 'perm-2',
    write: (e) => sent.push(e as Env),
  });
  const tools = createKeyTools({
    bridge: bridge as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
  const p = call(toolNamed(tools, 'promote_key'), {
    account: 'acct-1',
    standard: 'smime',
    fingerprint: 'ABCD',
  });
  assert.equal((sent[0]!.args as Env).verified, false, 'the default acceptance is unverified');
  bridge.respond('perm-2', true, false);
  assert.equal((await p).details.status, 'ok');
  assert.deepEqual(lastCapture().argv, [
    'keys',
    'promote',
    '--json',
    '--correlation-id',
    'turn-42',
    '--account',
    'acct-1',
    '--standard',
    'smime',
    '--fingerprint',
    'ABCD',
  ]);
});

test('reject_key approved: the argv pins the id and the fingerprint pair', async () => {
  const sent: Env[] = [];
  const bridge = new BridgeCtor({
    policy: () => POLICY,
    nextId: () => 'perm-1',
    write: (e) => sent.push(e as Env),
  });
  const tools = createKeyTools({
    bridge: bridge as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
  const p = call(toolNamed(tools, 'reject_key'), {
    account: 'acct-1',
    id: 7,
    standard: 'openpgp',
    fingerprint: 'ABCD',
  });
  assert.equal(sent.length, 1);
  assert.equal((sent[0]!.args as Env).id, 7);
  bridge.respond('perm-1', true, false);
  const r = await p;
  assert.equal(r.details.status, 'ok');
  assert.deepEqual(lastCapture().argv, [
    'keys',
    'reject',
    '--json',
    '--correlation-id',
    'turn-42',
    '--account',
    'acct-1',
    '--id',
    '7',
    '--standard',
    'openpgp',
    '--fingerprint',
    'ABCD',
  ]);
});

test('rejectAll while suspended yields skipped (abort, not error)', async () => {
  const bridge = new BridgeCtor({
    policy: () => POLICY,
    nextId: () => 'perm-5',
    write: () => {},
  });
  const tools = createKeyTools({
    bridge: bridge as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
  const p = call(toolNamed(tools, 'promote_key'), {
    account: 'acct-1',
    standard: 'openpgp',
    fingerprint: 'ABCD',
  });
  bridge.rejectAll('prompt aborted');
  const r = await p;
  assert.equal(r.details.status, 'skipped');
  assert.match(r.details.output!, /aborted/);
});

test('an empty policy table fails closed: a key read asks for approval too', async () => {
  const sent: Env[] = [];
  const bridge = new BridgeCtor({
    policy: () => ({}),
    nextId: () => 'perm-fc',
    write: (e) => sent.push(e as Env),
  });
  const tools = createKeyTools({
    bridge: bridge as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
  const p = call(toolNamed(tools, 'list_keys'), { account: 'acct-1' });
  assert.equal(sent.length, 1, 'with no table entry the tool needs approval');
  assert.equal(sent[0]!.tool, 'list_keys');
  bridge.respond('perm-fc', false, false);
  assert.equal((await p).details.status, 'rejected');
});

test('a missing correlation id omits the flag pair; the CLI mints its own', async () => {
  const tools = createKeyTools({
    bridge: new BridgeCtor({ policy: () => POLICY, write: () => {} }) as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => null,
  });
  const r = await call(toolNamed(tools, 'list_keys'), { account: 'acct-1' });
  assert.equal(r.details.status, 'ok');
  const cap = lastCapture();
  assert.deepEqual(cap.argv, ['keys', 'list', '--json', '--account', 'acct-1']);
  assert.equal(cap.correlation_id, null, 'no ASTRA_CORRELATION_ID without a turn id');
});

test('a spent budget answers skipped before the gate asks or executes', async () => {
  const sent: Env[] = [];
  const tools = makeTools(POLICY, sent, () => ({ spent: () => true }));
  const before = captureCount();
  const r = await call(toolNamed(tools, 'list_keys'), { account: 'acct-1' });
  assert.equal(r.details.status, 'skipped', 'past the bound the model is told no');
  assert.match(r.details.output!, /budget/, 'the reason names the budget');
  assert.equal(sent.length, 0, 'a spent budget never opens an ask');
  assert.equal(captureCount(), before, 'nothing executed past the bound');
});

test('a spent budget skips a confirm-gated key tool without opening the card', async () => {
  const sent: Env[] = [];
  const bridge = new BridgeCtor({
    policy: () => POLICY,
    nextId: () => 'perm-b',
    write: (e) => sent.push(e as Env),
  });
  const tools = createKeyTools({
    bridge: bridge as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
    budget: () => ({ spent: () => true }),
  });
  const before = captureCount();
  const p = call(toolNamed(tools, 'import_key'), { account: 'acct-1', material: 'cert' });
  // A run that wrongly opens the ask suspends; answer it so this test
  // fails on the assertions instead of hanging.
  await new Promise((r) => setTimeout(r, 50));
  if (sent.length > 0) {
    bridge.respond(String((sent[0]! as Env).id), false, false);
  }
  const r = await p;
  assert.equal(r.details.status, 'skipped');
  assert.match(r.details.output!, /budget/);
  assert.equal(sent.length, 0, 'the bound outranks the permission ask');
  assert.equal(captureCount(), before, 'nothing executed');
});
