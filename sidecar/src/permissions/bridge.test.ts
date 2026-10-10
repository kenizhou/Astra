// SPDX-License-Identifier: GPL-3.0-only
/**
 * PermissionBridge unit tests: direct calls, no process spawn. The security
 * posture under test is fail-closed: a tool absent from the policy table
 * needs approval, a malformed level needs approval, and only an explicit
 * table grant (level auto, or the allowlist flag) lets a call run. The
 * envelope writer is a constructor seam, so tests capture sent envelopes in
 * an array instead of touching stdout.
 *
 * Module loading follows the kylins pattern: this package is
 * "type": "commonjs" and tests run under `node --test`, so the module under
 * test is bundled with esbuild into a temp CJS file in before() and
 * require()d.
 */
import type {
  PermissionBridge,
  PermissionBridgeOptions,
  PermissionPolicy,
} from './bridge.ts';

const { test, before, after }: typeof import('node:test') = require('node:test');
const assert: typeof import('node:assert/strict') = require('node:assert/strict');
const esbuild: typeof import('esbuild') = require('esbuild');
const os: typeof import('node:os') = require('node:os');
const path: typeof import('node:path') = require('node:path');
const fs: typeof import('node:fs') = require('node:fs');

const policy: Record<string, PermissionPolicy> = {
  read_message: { level: 'auto' },
  save_draft: { level: 'confirm' },
  discard_draft: { level: 'confirm_preview' },
};

type Env = Record<string, unknown>;

let Bridge: new (opts: PermissionBridgeOptions) => PermissionBridge;
let bundlePath: string;

before(async () => {
  bundlePath = path.join(os.tmpdir(), `astra-permbridge-test-${process.pid}.cjs`);
  await esbuild.build({
    entryPoints: [path.resolve(__dirname, 'bridge.ts')],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    outfile: bundlePath,
    logLevel: 'silent',
  });
  Bridge = (
    require(bundlePath) as {
      PermissionBridge: new (opts: PermissionBridgeOptions) => PermissionBridge;
    }
  ).PermissionBridge;
});

after(() => {
  try {
    fs.rmSync(bundlePath, { force: true });
  } catch (e) {
    process.stderr.write(`bridge.test: failed to remove temp bundle: ${String(e)}\n`);
  }
});

test('check maps the policy table to gate decisions; allowlist short-circuits', () => {
  const b = new Bridge({
    policy: () => ({ ...policy, save_draft: { level: 'confirm', allowlist: true } }),
  });
  assert.equal(b.check('read_message'), 'auto');
  assert.equal(b.check('save_draft'), 'auto', 'the allowlist flag short-circuits confirm');
  assert.equal(b.check('discard_draft'), 'needs-approval');
  assert.equal(b.check('list_folders'), 'needs-approval', 'an unlisted tool fails closed');
});

test('an empty policy table leaves every tool needing approval (fail closed)', () => {
  const b = new Bridge({ policy: () => ({}) });
  for (const tool of ['list_folders', 'list_messages', 'read_message', 'save_draft', 'discard_draft']) {
    assert.equal(b.check(tool), 'needs-approval', `${tool} must need approval`);
  }
});

test('a malformed level in the delivered policy fails closed', () => {
  const b = new Bridge({
    policy: () => ({ save_draft: { level: 'yolo' as never } }),
  });
  assert.equal(b.check('save_draft'), 'needs-approval');
  assert.equal(b.levelOf('save_draft'), 'confirm', 'a malformed level degrades to plain confirm');
});

test('levelOf reports confirm_preview only for confirm_preview entries', () => {
  const b = new Bridge({ policy: () => policy });
  assert.equal(b.levelOf('discard_draft'), 'confirm_preview');
  assert.equal(b.levelOf('save_draft'), 'confirm');
  assert.equal(b.levelOf('unknown_tool'), 'confirm');
});

test('requestApproval suspends until respond; the envelope shape is pinned', async () => {
  const sent: Env[] = [];
  const b = new Bridge({
    policy: () => policy,
    nextId: () => 'perm-1',
    write: (e) => sent.push(e as unknown as Env),
  });
  const p = b.requestApproval({
    toolCallId: 'call-3',
    tool: 'save_draft',
    level: 'confirm',
    args: { account: 'acct-1' },
  });
  assert.equal(sent.length, 1);
  assert.deepEqual(sent[0], {
    type: 'permission_request',
    id: 'perm-1',
    tool_call_id: 'call-3',
    tool: 'save_draft',
    level: 'confirm',
    args: { account: 'acct-1' },
  });
  b.respond('perm-1', true, true);
  assert.deepEqual(await p, { approved: true, alwaysAllow: true });
});

test('a confirm_preview request carries its preview payload in the envelope', async () => {
  const sent: Env[] = [];
  const b = new Bridge({
    policy: () => policy,
    nextId: () => 'perm-2',
    write: (e) => sent.push(e as unknown as Env),
  });
  const p = b.requestApproval({
    toolCallId: 'call-9',
    tool: 'discard_draft',
    level: 'confirm_preview',
    args: { account: 'acct-1', key: 'draft-9' },
    preview: { subject: 'Re: Friday', body_length: 42 },
  });
  assert.equal(sent.length, 1);
  assert.deepEqual(sent[0], {
    type: 'permission_request',
    id: 'perm-2',
    tool_call_id: 'call-9',
    tool: 'discard_draft',
    level: 'confirm_preview',
    args: { account: 'acct-1', key: 'draft-9' },
    preview: { subject: 'Re: Friday', body_length: 42 },
  });
  b.respond('perm-2', false, false);
  assert.deepEqual(await p, { approved: false, alwaysAllow: false });
});

test('respond with an unknown id warns on stderr but never throws', () => {
  const b = new Bridge({ policy: () => policy });
  const orig = process.stderr.write;
  let warned = '';
  process.stderr.write = ((chunk: unknown): boolean => {
    warned += String(chunk);
    return true;
  }) as typeof process.stderr.write;
  try {
    b.respond('nope', true, false);
  } finally {
    process.stderr.write = orig;
  }
  assert.match(warned, /unknown id nope/);
});

test('rejectAll rejects every suspended approval with no promise leak', async () => {
  const b = new Bridge({
    policy: () => policy,
    nextId: (() => {
      let n = 0;
      return () => `perm-${++n}`;
    })(),
    write: () => {},
  });
  const p1 = b.requestApproval({
    toolCallId: 'c1',
    tool: 'save_draft',
    level: 'confirm',
    args: {},
  });
  const p2 = b.requestApproval({
    toolCallId: 'c2',
    tool: 'discard_draft',
    level: 'confirm',
    args: {},
  });
  b.rejectAll('prompt aborted');
  await assert.rejects(p1, /aborted/);
  await assert.rejects(p2, /aborted/);
  b.rejectAll('again');
});

test('an envelope write failure rejects the pending promise instead of leaking it', async () => {
  const b = new Bridge({
    policy: () => policy,
    nextId: () => 'perm-x',
    write: () => {
      throw new Error('stdout dead');
    },
  });
  await assert.rejects(
    b.requestApproval({ toolCallId: 'c1', tool: 'save_draft', level: 'confirm', args: {} }),
    /stdout dead/,
  );
});

test('the default nextId is pid-prefixed so a restart never re-issues a host-held id', async () => {
  const sent: Env[] = [];
  const b = new Bridge({ policy: () => policy, write: (e) => sent.push(e as unknown as Env) });
  const p = b.requestApproval({
    toolCallId: 'c1',
    tool: 'save_draft',
    level: 'confirm',
    args: {},
  });
  const id = sent[0]?.id;
  assert.equal(typeof id, 'string');
  assert.match(id as string, new RegExp(`^perm-${process.pid}-\\d+$`));
  b.respond(id as string, true, false);
  assert.deepEqual(await p, { approved: true, alwaysAllow: false });
});
