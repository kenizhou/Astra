// SPDX-License-Identifier: GPL-3.0-only
/**
 * The permission gating map's sidecar pin for the key tools (docs/cli.md):
 * createKeyTools yields exactly the five names the map lists, and every
 * tool's argv opens with the map's verb and subverb. The per-tool flag
 * grammar lives in keyTools.test.ts; this file holds the exhaustive map
 * itself, so adding, removing, or renaming a tool fails here beside the
 * host's policy and preview pins and the mail tools' own map pin. The CLI
 * is played by the committed stub, the same fixture the sibling suites
 * drive.
 */
import type { PermissionBridge, PermissionPolicy } from '../permissions/bridge.ts';
import type { MailToolDetails } from './gated.ts';

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

let createKeyTools: (deps: Deps) => Tool[];
let BridgeCtor: new (opts: {
  policy: () => Record<string, PermissionPolicy>;
  nextId?: () => string;
  write?: (e: unknown) => void;
}) => BridgeIface;
const bundlePaths: string[] = [];
let dataDir: string;

type Env = Record<string, unknown>;
type Capture = { argv: string[] };

/** The five policy rows exactly as the host's default_policy ships them:
 *  the two reads run, the import and the review actions ask. */
const POLICY: Record<string, PermissionPolicy> = {
  list_keys: { level: 'auto' },
  list_collected_keys: { level: 'auto' },
  import_key: { level: 'confirm' },
  promote_key: { level: 'confirm' },
  reject_key: { level: 'confirm' },
};

/** The gating map's rows: the tool, representative params, and the CLI
 *  verb and subverb its buildCliArgv must open with. */
const MAP: { tool: string; params: Record<string, unknown>; verb: string; subverb: string }[] = [
  { tool: 'list_keys', params: { account: 'acct-1' }, verb: 'keys', subverb: 'list' },
  { tool: 'list_collected_keys', params: { account: 'acct-1' }, verb: 'keys', subverb: 'collected' },
  {
    tool: 'import_key',
    params: { account: 'acct-1', material: 'armored cert' },
    verb: 'keys',
    subverb: 'import',
  },
  {
    tool: 'promote_key',
    params: { account: 'acct-1', standard: 'openpgp', fingerprint: 'ABCD' },
    verb: 'keys',
    subverb: 'promote',
  },
  {
    tool: 'reject_key',
    params: { account: 'acct-1', id: 7, standard: 'openpgp', fingerprint: 'ABCD' },
    verb: 'keys',
    subverb: 'reject',
  },
];

before(async () => {
  const keyBundle = path.join(os.tmpdir(), `astra-keytools-map-${process.pid}.cjs`);
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

  const bridgeBundle = path.join(os.tmpdir(), `astra-keytools-map-bridge-${process.pid}.cjs`);
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

  dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'astra-keytools-map-'));
  const stub = fs.readFileSync(path.resolve(__dirname, '..', 'fixtures', 'stub-cli.cjs'), 'utf8');
  fs.writeFileSync(path.join(dataDir, 'keys'), stub);
});

after(() => {
  for (const p of bundlePaths) {
    try {
      fs.rmSync(p, { force: true });
    } catch (e) {
      process.stderr.write(`keyToolsMap.test: failed to remove ${p}: ${String(e)}\n`);
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

test('the gating map pins the five key names and every verb and subverb', async () => {
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
  assert.deepEqual(
    tools.map((t) => t.name).sort(),
    MAP.map((row) => row.tool).sort(),
    'createKeyTools yields exactly the map\'s five tools, no more no less',
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
