// SPDX-License-Identifier: GPL-3.0-only
/**
 * safeExec tests. The stub CLI is node itself running an inline script that
 * echoes its argv, env, and cwd as JSON, mirroring how the real astra-cli is
 * invoked: no shell anywhere, execFile with an argv array, cwd at the data
 * dir, and a scrubbed environment (PATH plus the ASTRA_* passthrough and the
 * injected ASTRA_DATA_DIR and ASTRA_CORRELATION_ID).
 *
 * Module loading follows the kylins pattern: this package is
 * "type": "commonjs" and tests run under `node --test`, so the module under
 * test is bundled with esbuild into a temp CJS file in before() and
 * require()d.
 */
import type { SafeExecOpts, SafeExecResult } from './safeExec.ts';

const { test, before, after }: typeof import('node:test') = require('node:test');
const assert: typeof import('node:assert/strict') = require('node:assert/strict');
const esbuild: typeof import('esbuild') = require('esbuild');
const os: typeof import('node:os') = require('node:os');
const path: typeof import('node:path') = require('node:path');
const fs: typeof import('node:fs') = require('node:fs');

interface StubEcho {
  argv: string[];
  env_keys: string[];
  data_dir: string | null;
  correlation_id: string | null;
  cwd: string;
}

let runSafeExec: (argv: string[], opts: SafeExecOpts) => Promise<SafeExecResult>;
let bundlePath: string;
let dataDir: string;

const ECHO_SCRIPT =
  'console.log(JSON.stringify({argv: process.argv.slice(1), env_keys: ' +
  'Object.keys(process.env).sort(), data_dir: process.env.ASTRA_DATA_DIR ?? null, ' +
  'correlation_id: process.env.ASTRA_CORRELATION_ID ?? null, cwd: process.cwd()}))';

before(async () => {
  bundlePath = path.join(os.tmpdir(), `astra-safeexec-test-${process.pid}.cjs`);
  await esbuild.build({
    entryPoints: [path.resolve(__dirname, 'safeExec.ts')],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    outfile: bundlePath,
    logLevel: 'silent',
  });
  runSafeExec = (
    require(bundlePath) as {
      runSafeExec: (argv: string[], opts: SafeExecOpts) => Promise<SafeExecResult>;
    }
  ).runSafeExec;
  dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'astra-safeexec-data-'));
});

after(() => {
  try {
    fs.rmSync(bundlePath, { force: true });
  } catch (e) {
    process.stderr.write(`safeExec.test: failed to remove temp bundle: ${String(e)}\n`);
  }
  fs.rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
});

/** Run the echo script under safeExec and parse its JSON output. The child
 *  argv is ['-e', ECHO_SCRIPT, ...extra]: node runs the inline script and
 *  the extra tokens become its process.argv tail, echoed verbatim. */
async function echo(extra: string[], opts?: Partial<SafeExecOpts>): Promise<StubEcho> {
  const r = await runSafeExec(['astra-cli', '-e', ECHO_SCRIPT, ...extra], {
    cliPath: process.execPath,
    allowlist: ['astra-cli'],
    dataDir,
    ...opts,
  });
  assert.equal(r.exitCode, 0, `echo stub must succeed: ${r.stderr}`);
  return JSON.parse(r.stdout.trim()) as StubEcho;
}

test('rejects a non-astra-cli prefix before touching the filesystem', async () => {
  await assert.rejects(
    () => runSafeExec(['curl', 'http://example.invalid'], { cliPath: '/bin/astra-cli', allowlist: ['astra-cli'] }),
    /prefix not allowed: curl/,
  );
  await assert.rejects(
    () => runSafeExec([], { cliPath: '/bin/astra-cli', allowlist: ['astra-cli'] }),
    /prefix not allowed/,
  );
});

test('rejects an unresolved cliPath loudly (null and the empty string)', async () => {
  for (const cliPath of [null, '']) {
    await assert.rejects(
      () =>
        runSafeExec(['astra-cli', 'accounts', 'list'], {
          cliPath,
          allowlist: ['astra-cli'],
        }),
      /cliPath not configured/,
    );
  }
});

test('argv reaches the child verbatim after the prefix token, no shell joining', async () => {
  const seen = await echo(['folders', 'list', '--account', 'a1', 'a b', '"quoted"']);
  // Under `node -e SCRIPT extra...` the echo stub's process.argv.slice(1) is
  // exactly the extra tokens, byte for byte: no shell joining anywhere.
  assert.deepEqual(seen.argv, [
    'folders',
    'list',
    '--account',
    'a1',
    'a b',
    '"quoted"',
  ]);
});

/** Node itself injects this fixed Windows system set into every spawned
 *  child whose env lacks it (SYSTEMROOT and friends): they come from the
 *  platform, not from this process's environment, so they are not leaks.
 *  Everything else in the child env must be PATH or ASTRA_*. */
const NODE_INJECTED_WINDOWS_ENV: readonly string[] = [
  'HOMEDRIVE',
  'HOMEPATH',
  'LOGONSERVER',
  'SYSTEMDRIVE',
  'SYSTEMROOT',
  'TEMP',
  'USERDOMAIN',
  'USERNAME',
  'USERPROFILE',
  'WINDIR',
];

test('the child environment is scrubbed: only PATH and ASTRA_* survive', async () => {
  process.env.ASTRA_TEST_PASSTHROUGH = 'host-set';
  process.env.SIDEKICK_TEST_SECRET = 'must-not-leak';
  try {
    const seen = await echo(['accounts', 'list'], { correlationId: 'corr-77' });
    for (const key of seen.env_keys) {
      const allowed =
        key.startsWith('ASTRA_') || key === 'PATH' || NODE_INJECTED_WINDOWS_ENV.includes(key);
      assert.ok(
        allowed,
        `env key ${key} survives the scrub; only PATH and ASTRA_* may`,
      );
    }
    assert.ok(seen.env_keys.includes('ASTRA_TEST_PASSTHROUGH'), 'ASTRA_* passthrough arrives');
    assert.ok(!seen.env_keys.includes('SIDEKICK_TEST_SECRET'), 'a non-ASTRA secret is dropped');
    assert.equal(seen.data_dir, dataDir, 'ASTRA_DATA_DIR is the delivered data dir');
    assert.equal(seen.correlation_id, 'corr-77', 'ASTRA_CORRELATION_ID rides every spawn');
  } finally {
    delete process.env.ASTRA_TEST_PASSTHROUGH;
    delete process.env.SIDEKICK_TEST_SECRET;
  }
});

test('the child cwd is the data dir', async () => {
  const seen = await echo([]);
  assert.equal(seen.cwd, dataDir);
});

test('a falsy dataDir still spawns, without ASTRA_DATA_DIR and with a loud warning', async () => {
  const writes: string[] = [];
  const orig = process.stderr.write;
  process.stderr.write = ((chunk: unknown) => {
    writes.push(String(chunk));
    return true;
  }) as typeof orig;
  try {
    const r = await runSafeExec(
      ['astra-cli', '-e', 'console.log(process.env.ASTRA_DATA_DIR ?? "")'],
      { cliPath: process.execPath, allowlist: ['astra-cli'], dataDir: '' },
    );
    assert.equal(r.exitCode, 0);
    assert.equal(r.stdout.trim(), '', 'no ASTRA_DATA_DIR injected when dataDir is falsy');
  } finally {
    process.stderr.write = orig;
  }
  assert.ok(
    writes.some((w) => /dataDir was not delivered/.test(w)),
    `expected the loud dataDir warning, saw: ${writes.join('|')}`,
  );
});

test('an empty correlationId is not injected and warns loudly', async () => {
  const writes: string[] = [];
  const orig = process.stderr.write;
  process.stderr.write = ((chunk: unknown) => {
    writes.push(String(chunk));
    return true;
  }) as typeof orig;
  try {
    const r = await runSafeExec(
      ['astra-cli', '-e', 'console.log(process.env.ASTRA_CORRELATION_ID ?? "")'],
      { cliPath: process.execPath, allowlist: ['astra-cli'], dataDir, correlationId: '' },
    );
    assert.equal(r.exitCode, 0);
    assert.equal(r.stdout.trim(), '', 'an empty correlation id never reaches the child env');
  } finally {
    process.stderr.write = orig;
  }
  assert.ok(
    writes.some((w) => /correlationId/.test(w) && /ASTRA_CORRELATION_ID/.test(w)),
    `expected the loud correlationId warning, saw: ${writes.join('|')}`,
  );
});

test('stdout is truncated at 50KB with a marker', async () => {
  const r = await runSafeExec(['astra-cli', '-e', 'process.stdout.write("x".repeat(60 * 1024))'], {
    cliPath: process.execPath,
    allowlist: ['astra-cli'],
    dataDir,
  });
  assert.ok(r.stdout.length <= 51 * 1024);
  assert.match(r.stdout, /\[truncated\]$/);
});

test('a non-zero exit does not throw: the exit code and stderr ride back', async () => {
  const r = await runSafeExec(
    ['astra-cli', '-e', 'process.stderr.write("usage: boom"); process.exit(2)'],
    { cliPath: process.execPath, allowlist: ['astra-cli'], dataDir },
  );
  assert.equal(r.exitCode, 2);
  assert.match(r.stderr, /boom/);
});

test('a missing binary rejects loudly at spawn level (not an exit code)', async () => {
  await assert.rejects(
    () =>
      runSafeExec(['astra-cli', 'accounts', 'list'], {
        cliPath: path.join(dataDir, 'definitely-not-here.exe'),
        allowlist: ['astra-cli'],
        dataDir,
      }),
    /safe_exec: failed to run astra-cli/,
  );
});
