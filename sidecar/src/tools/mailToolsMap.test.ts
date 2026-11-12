// SPDX-License-Identifier: GPL-3.0-only
/**
 * The permission gating map's sidecar pin (docs/cli.md): createMailTools
 * yields exactly the ten names the map lists, and every tool's argv opens
 * with the map's verb and subverb. The per-tool flag grammar lives in
 * mailTools.test.ts and mailToolsActions.test.ts; this file holds the
 * exhaustive map itself, so adding, removing, or renaming a tool fails
 * here beside the host's policy and preview pins. The CLI is played by
 * the committed stub, the same fixture the sibling suites drive.
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
type Capture = { argv: string[] };

/** The ten policy rows exactly as the host's default_policy ships them:
 *  the four reads run, the six writes ask. */
const POLICY: Record<string, PermissionPolicy> = {
  list_folders: { level: 'auto' },
  list_messages: { level: 'auto' },
  read_message: { level: 'auto' },
  search_messages: { level: 'auto' },
  save_draft: { level: 'confirm' },
  discard_draft: { level: 'confirm' },
  mark_read: { level: 'confirm' },
  set_flagged: { level: 'confirm' },
  archive_message: { level: 'confirm' },
  move_to_trash: { level: 'confirm' },
};

/** The gating map's rows: the tool, representative params, and the CLI
 *  verb and subverb its buildCliArgv must open with. */
const MAP: { tool: string; params: Record<string, unknown>; verb: string; subverb: string }[] = [
  { tool: 'list_folders', params: { account: 'acct-1' }, verb: 'folders', subverb: 'list' },
  { tool: 'list_messages', params: { account: 'acct-1' }, verb: 'messages', subverb: 'list' },
  { tool: 'read_message', params: { account: 'acct-1', keys: ['msg-1'] }, verb: 'messages', subverb: 'read' },
  { tool: 'search_messages', params: { query: 'report' }, verb: 'messages', subverb: 'search' },
  {
    tool: 'save_draft',
    params: { account: 'acct-1', from: 'ada@example.com', to: ['bob@example.com'], subject: 's', body: 'b' },
    verb: 'drafts',
    subverb: 'save',
  },
  { tool: 'discard_draft', params: { account: 'acct-1', key: 'draft-1' }, verb: 'drafts', subverb: 'discard' },
  { tool: 'mark_read', params: { account: 'acct-1', key: 'msg-1' }, verb: 'messages', subverb: 'mark-read' },
  { tool: 'set_flagged', params: { account: 'acct-1', key: 'msg-1' }, verb: 'messages', subverb: 'set-flagged' },
  { tool: 'archive_message', params: { account: 'acct-1', key: 'msg-1' }, verb: 'messages', subverb: 'archive' },
  { tool: 'move_to_trash', params: { account: 'acct-1', key: 'msg-1' }, verb: 'messages', subverb: 'trash' },
];

before(async () => {
  const mailBundle = path.join(os.tmpdir(), `astra-mailtools-map-${process.pid}.cjs`);
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

  const bridgeBundle = path.join(os.tmpdir(), `astra-mailtools-map-bridge-${process.pid}.cjs`);
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

  dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'astra-mailtools-map-'));
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
      process.stderr.write(`mailToolsMap.test: failed to remove ${p}: ${String(e)}\n`);
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

test('the gating map pins the ten names and every verb and subverb', async () => {
  const sent: Env[] = [];
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
  assert.deepEqual(
    tools.map((t) => t.name).sort(),
    MAP.map((row) => row.tool).sort(),
    'createMailTools yields exactly the map\'s ten tools, no more no less',
  );
  for (const row of MAP) {
    const asksBefore = sent.length;
    const p = call(toolNamed(tools, row.tool), row.params);
    if (sent.length > asksBefore) {
      assert.equal((sent[asksBefore]! as Env).level, 'confirm', `${row.tool} asks at confirm`);
      bridge.respond('perm-1', true, false);
    }
    const r = await p;
    assert.equal(r.details.status, 'ok', `${row.tool} runs under the shipped policy`);
    assert.deepEqual(
      lastCapture().argv.slice(0, 2),
      [row.verb, row.subverb],
      `${row.tool} opens with the map's verb and subverb`,
    );
  }
});