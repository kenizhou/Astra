// SPDX-License-Identifier: GPL-3.0-only
/**
 * The search and act tools' tests: search_messages (a read, auto) and
 * mark_read, set_flagged, archive_message, move_to_trash (confirm-gated),
 * each through runGated beside the five mail tools, plus the truncation
 * policy on the read tool's model-facing output (the reference's
 * four-thousand-character bound; the ledger's two-hundred preview cap lives
 * in the desktop relay). The CLI is played by the committed stub, the same
 * fixture mailTools.test.ts drives, and the argv assertions pin the exact
 * grammar docs/cli.md spells.
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
let capOutput: (text: string, cap: number) => string;
let BridgeCtor: new (opts: {
  policy: () => Record<string, PermissionPolicy>;
  nextId?: () => string;
  write?: (e: unknown) => void;
}) => BridgeIface;
const bundlePaths: string[] = [];
let dataDir: string;

type Env = Record<string, unknown>;
type Capture = { argv: string[]; data_dir: string | null; correlation_id: string | null; cwd: string };

/** The policy rows the desktop host ships for the new tools: the search is
 *  a read, every act verb asks. The read row rides along so the cap test's
 *  existing tool never suspends on a fail-closed ask. */
const POLICY: Record<string, PermissionPolicy> = {
  search_messages: { level: 'auto' },
  mark_read: { level: 'confirm' },
  set_flagged: { level: 'confirm' },
  archive_message: { level: 'confirm' },
  move_to_trash: { level: 'confirm' },
  read_message: { level: 'auto' },
};

before(async () => {
  const mailBundle = path.join(os.tmpdir(), `astra-mailtools-actions-${process.pid}.cjs`);
  await esbuild.build({
    entryPoints: [path.resolve(__dirname, 'mailTools.ts')],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    outfile: mailBundle,
    logLevel: 'silent',
  });
  bundlePaths.push(mailBundle);
  const exported = require(mailBundle) as {
    createMailTools: typeof createMailTools;
    capOutput: typeof capOutput;
  };
  createMailTools = exported.createMailTools;
  capOutput = exported.capOutput;

  const bridgeBundle = path.join(os.tmpdir(), `astra-mailtools-actions-bridge-${process.pid}.cjs`);
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

  dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'astra-mailtools-actions-'));
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
      process.stderr.write(`mailToolsActions.test: failed to remove ${p}: ${String(e)}\n`);
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

/** The built tools beside the bridge they gate through, so a test can
 *  answer the ask its call opened. */
interface Rig {
  tools: Tool[];
  bridge: BridgeIface;
}

function makeTools(sent: Env[] = []): Rig {
  const bridge = new BridgeCtor({
    policy: () => POLICY,
    nextId: () => 'perm-1',
    write: (e) => sent.push(e as Env),
  });
  const tools = createMailTools({
    bridge: bridge as never,
    cliPath: () => process.execPath,
    dataDir: () => dataDir,
    correlationId: () => 'turn-42',
  });
  return { tools, bridge };
}

test('search_messages runs auto and its argv pins the query grammar', async () => {
  const sent: Env[] = [];
  const { tools } = makeTools(sent);
  const r = await call(toolNamed(tools, 'search_messages'), { query: 'quarterly report' });
  assert.equal(r.details.status, 'ok');
  assert.equal(sent.length, 0, 'a read row never asks');
  assert.deepEqual(lastCapture().argv, [
    'messages',
    'search',
    '--json',
    '--correlation-id',
    'turn-42',
    '--query',
    'quarterly report',
    '--limit',
    '20',
  ]);
});

test('search_messages carries the optional account, folder, and paging flags', async () => {
  const { tools } = makeTools();
  const r = await call(toolNamed(tools, 'search_messages'), {
    query: 'report',
    account: 'acct-1',
    folder: 'inbox',
    limit: 10,
    offset: 20,
  });
  assert.equal(r.details.status, 'ok');
  assert.deepEqual(lastCapture().argv, [
    'messages',
    'search',
    '--json',
    '--correlation-id',
    'turn-42',
    '--query',
    'report',
    '--account',
    'acct-1',
    '--folder',
    'inbox',
    '--limit',
    '10',
    '--offset',
    '20',
  ]);
});

test('mark_read asks first and a rejection executes nothing', async () => {
  const sent: Env[] = [];
  const { tools, bridge } = makeTools(sent);
  const before = captureCount();
  const p = call(toolNamed(tools, 'mark_read'), { account: 'acct-1', key: 'msg-1' });
  assert.equal(sent.length, 1, 'a confirm-gated act asks before executing');
  const req = sent[0]!;
  assert.equal(req.type, 'permission_request');
  assert.equal(req.tool, 'mark_read');
  assert.equal(req.level, 'confirm');
  assert.equal((req.args as Env).account, 'acct-1');
  assert.equal((req.args as Env).read, true, 'the card shows the default direction');
  bridge.respond('perm-1', false, false);
  const r = await p;
  assert.equal(r.details.status, 'rejected');
  assert.equal(captureCount(), before, 'a rejection still executes nothing');
});

test('mark_read approved: the argv pins the key, and read false rides --unread', async () => {
  const { tools, bridge } = makeTools();
  const p = call(toolNamed(tools, 'mark_read'), { account: 'acct-1', key: 'msg-1' });
  bridge.respond('perm-1', true, false);
  const r = await p;
  assert.equal(r.details.status, 'ok');
  assert.deepEqual(lastCapture().argv, [
    'messages',
    'mark-read',
    '--json',
    '--correlation-id',
    'turn-42',
    '--account',
    'acct-1',
    '--key',
    'msg-1',
  ]);

  const p2 = call(toolNamed(tools, 'mark_read'), { account: 'acct-1', key: 'msg-1', read: false }, 'call-2');
  bridge.respond('perm-1', true, false);
  assert.equal((await p2).details.status, 'ok');
  assert.deepEqual(lastCapture().argv, [
    'messages',
    'mark-read',
    '--json',
    '--correlation-id',
    'turn-42',
    '--account',
    'acct-1',
    '--key',
    'msg-1',
    '--unread',
  ]);
});

test('set_flagged approved: flagged false rides --clear', async () => {
  const { tools, bridge } = makeTools();
  const p = call(toolNamed(tools, 'set_flagged'), { account: 'acct-1', key: 'msg-1', flagged: false });
  bridge.respond('perm-1', true, false);
  const r = await p;
  assert.equal(r.details.status, 'ok');
  assert.deepEqual(lastCapture().argv, [
    'messages',
    'set-flagged',
    '--json',
    '--correlation-id',
    'turn-42',
    '--account',
    'acct-1',
    '--key',
    'msg-1',
    '--clear',
  ]);
});

test('archive_message and move_to_trash carry their own subverbs', async () => {
  const { tools, bridge } = makeTools();
  const a = call(toolNamed(tools, 'archive_message'), { account: 'acct-1', key: 'msg-1' });
  bridge.respond('perm-1', true, false);
  assert.equal((await a).details.status, 'ok');
  assert.deepEqual(lastCapture().argv.slice(0, 8), [
    'messages',
    'archive',
    '--json',
    '--correlation-id',
    'turn-42',
    '--account',
    'acct-1',
    '--key',
  ]);

  const t = call(toolNamed(tools, 'move_to_trash'), { account: 'acct-1', key: 'msg-1' }, 'call-2');
  bridge.respond('perm-1', true, false);
  assert.equal((await t).details.status, 'ok');
  assert.deepEqual(lastCapture().argv.slice(0, 2), ['messages', 'trash']);
});

test('capOutput leaves a short output whole and cuts a long one at the bound', () => {
  const cap = 4000;
  assert.equal(capOutput('short', cap), 'short');
  assert.equal(capOutput('x'.repeat(cap), cap), 'x'.repeat(cap), 'exactly at the cap is whole');
  const cut = capOutput('x'.repeat(cap + 1), cap);
  assert.equal(cut.length, cap + `[truncated at ${cap} characters]`.length);
  assert.match(cut, /\[truncated at 4000 characters\]$/);
});

test('read_message caps its model-facing output at the read bound', async () => {
  process.env.ASTRA_STUB_PAD = '6000';
  try {
    const { tools } = makeTools();
    const r = await call(toolNamed(tools, 'read_message'), {
      account: 'acct-1',
      keys: ['msg-1'],
    });
    assert.equal(r.details.status, 'ok');
    assert.equal(
      r.details.output!.length,
      4000 + '[truncated at 4000 characters]'.length,
      'the padded stub output is cut at the reference read bound',
    );
    assert.match(r.details.output!, /\[truncated at 4000 characters\]$/);
  } finally {
    delete process.env.ASTRA_STUB_PAD;
  }
});

test('the pi allowlist and the tool names are one list', async () => {
  // Two hand-maintained lists gate the same fifteen tools: a typo in either
  // silently disables a tool or fail-closes it in the live surface while
  // every other test stays green. The parity pin keeps them one list. The
  // adapter loads as an ESM bundle with its packages external (pi's own
  // code mixes import.meta with dynamic require, so neither bundled format
  // evaluates), written inside the sidecar so node resolves the externals.
  const adapterBundle = path.resolve(__dirname, '..', '..', '.parity-allowlist.mjs');
  await esbuild.build({
    entryPoints: [path.resolve(__dirname, '..', 'harness', 'pi-adapter.ts')],
    bundle: true,
    platform: 'node',
    format: 'esm',
    packages: 'external',
    outfile: adapterBundle,
    logLevel: 'silent',
  });
  const keyBundle = path.join(os.tmpdir(), `astra-parity-keytools-${process.pid}.cjs`);
  await esbuild.build({
    entryPoints: [path.resolve(__dirname, 'keyTools.ts')],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    outfile: keyBundle,
    logLevel: 'silent',
  });
  try {
    const adapter = (await import(
      /* prettier-ignore */ require('node:url').pathToFileURL(adapterBundle).href
    )) as { CHAT_TOOL_ALLOWLIST: readonly string[] };

    const deps = {
      bridge: new BridgeCtor({ policy: () => POLICY }) as never,
      cliPath: () => null,
      correlationId: () => null,
    };
    const keyTools = (require(keyBundle) as { createKeyTools: (d: Deps) => Tool[] })
      .createKeyTools(deps);
    const names = [...createMailTools(deps), ...keyTools].map((tool) => tool.name);
    assert.deepEqual(
      [...adapter.CHAT_TOOL_ALLOWLIST].sort(),
      names.sort(),
      'the allowlist and the mail plus key tools name exactly the same tools',
    );
  } finally {
    fs.rmSync(adapterBundle, { force: true });
    fs.rmSync(keyBundle, { force: true });
  }
});
