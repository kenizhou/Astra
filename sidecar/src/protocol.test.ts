// SPDX-License-Identifier: GPL-3.0-only
/**
 * protocol.test.ts: every message shape round trips through the shared
 * fixtures under src/fixtures/protocol. A fixture file is one JSON object on
 * one line, byte-identical to a wire line: the in-* fixtures are parsed by
 * parseCommand here and by the Rust mirror's serde in the Task 5 test
 * (include_str over these same files), so the two sides cannot drift. The
 * out-* fixtures are structurally validated by isValidEnvelope, the same
 * knowledge the Rust mirror deserialises with.
 *
 * Module loading follows the kylins pattern: this package is
 * "type": "commonjs" and tests run under `node --test` (type stripping is
 * unflagged on node 24), which rejects ESM import syntax in .ts files, so
 * the module under test is bundled with esbuild into a temp CJS file in
 * before() and require()d.
 */
const { test, before, after }: typeof import('node:test') = require('node:test');
const assert: typeof import('node:assert/strict') = require('node:assert/strict');
const esbuild: typeof import('esbuild') = require('esbuild');
const os: typeof import('node:os') = require('node:os');
const path: typeof import('node:path') = require('node:path');
const fs: typeof import('node:fs') = require('node:fs');

interface ProtocolModule {
  parseCommand: (line: string) => unknown;
  isValidEnvelope: (v: unknown) => boolean;
}

let proto: ProtocolModule;
let bundlePath: string;

const FIXTURE_DIR = path.join(__dirname, 'fixtures', 'protocol');

before(async () => {
  bundlePath = path.join(os.tmpdir(), `astra-protocol-test-${process.pid}.cjs`);
  await esbuild.build({
    entryPoints: [path.resolve(__dirname, 'protocol.ts')],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    outfile: bundlePath,
    logLevel: 'silent',
  });
  proto = require(bundlePath) as ProtocolModule;
});

after(() => {
  try {
    fs.rmSync(bundlePath, { force: true });
  } catch (e) {
    process.stderr.write(`protocol.test: failed to remove temp bundle: ${String(e)}\n`);
  }
});

/** Every fixture file in the protocol dir, sorted, as {name, text, parsed}. */
function loadFixtures(): { name: string; text: string; parsed: Record<string, unknown> }[] {
  return fs
    .readdirSync(FIXTURE_DIR)
    .filter((f) => f.endsWith('.json'))
    .sort()
    .map((name) => {
      const text = fs.readFileSync(path.join(FIXTURE_DIR, name), 'utf8').trim();
      return { name, text, parsed: JSON.parse(text) as Record<string, unknown> };
    });
}

test('every in-* fixture parses field-for-field through parseCommand', () => {
  const ins = loadFixtures().filter((f) => f.name.startsWith('in-'));
  assert.ok(ins.length >= 5, `expected at least five in fixtures, saw ${ins.length}`);
  for (const f of ins) {
    const cmd = proto.parseCommand(f.text);
    assert.ok(cmd !== null, `${f.name}: must parse`);
    assert.deepEqual(cmd, f.parsed, `${f.name}: parsed shape equals the fixture byte-for-byte`);
    // A re-serialised round trip must parse to the same value again: the
    // parser neither drops nor renames a field the Rust mirror will send.
    assert.deepEqual(
      proto.parseCommand(JSON.stringify(f.parsed)),
      f.parsed,
      `${f.name}: round trip via stringify is stable`,
    );
  }
});

test('every out-* fixture is structurally valid as an envelope', () => {
  const outs = loadFixtures().filter((f) => f.name.startsWith('out-'));
  assert.ok(outs.length >= 10, `expected at least ten out fixtures, saw ${outs.length}`);
  for (const f of outs) {
    assert.equal(
      proto.isValidEnvelope(f.parsed),
      true,
      `${f.name}: must validate as an OutEnvelope`,
    );
  }
});

test('an envelope fixture with a field corrupted no longer validates', () => {
  const raw = JSON.parse(
    fs.readFileSync(path.join(FIXTURE_DIR, 'out-permission-request.json'), 'utf8'),
  );
  assert.equal(proto.isValidEnvelope(raw), true);
  const badLevel = { ...raw, level: 'confirm_preview' as string, preview: { subject: 's' } };
  assert.equal(proto.isValidEnvelope(badLevel), true, 'confirm_preview with a preview is valid');
  assert.equal(proto.isValidEnvelope({ ...raw, level: 'yolo' }), false, 'unknown level fails');
  assert.equal(proto.isValidEnvelope({ ...raw, tool_call_id: 3 }), false, 'non-string id fails');
  assert.equal(proto.isValidEnvelope({ ...raw, type: 'mystery' }), false, 'unknown type fails');
  assert.equal(proto.isValidEnvelope(null), false);
});

test('unknown command type is rejected (fail closed, never a guess)', () => {
  assert.equal(proto.parseCommand('{"type":"teleport","id":"x"}'), null);
});

test('malformed prompt is rejected: every required field must be a string', () => {
  const good = {
    type: 'prompt',
    id: 'turn-1',
    session_id: 'agent:main',
    text: 'hi',
    correlation_id: 'corr-1',
  };
  assert.ok(proto.parseCommand(JSON.stringify(good)));
  for (const drop of ['id', 'session_id', 'text', 'correlation_id']) {
    const broken = { ...good };
    delete broken[drop as keyof typeof good];
    assert.equal(proto.parseCommand(JSON.stringify(broken)), null, `missing ${drop} rejects`);
  }
  assert.equal(
    proto.parseCommand(JSON.stringify({ ...good, correlation_id: 42 })),
    null,
    'non-string correlation_id rejects',
  );
});

test('malformed init is rejected: policy and model_configs fail closed', () => {
  const good = {
    type: 'init',
    harness: 'pi',
    cli_path: '',
    data_dir: '/tmp/d',
  };
  assert.ok(proto.parseCommand(JSON.stringify(good)), 'minimal init parses');
  // policy: unknown level, non-boolean allowlist, non-object entry.
  assert.equal(
    proto.parseCommand(JSON.stringify({ ...good, policy: { save_draft: { level: 'yolo' } } })),
    null,
  );
  assert.equal(
    proto.parseCommand(
      JSON.stringify({ ...good, policy: { save_draft: { level: 'confirm', allowlist: 'yes' } } }),
    ),
    null,
  );
  assert.equal(proto.parseCommand(JSON.stringify({ ...good, policy: { save_draft: 42 } })), null);
  assert.equal(proto.parseCommand(JSON.stringify({ ...good, policy: [] })), null);
  // model_configs: missing base_url/model, unknown api, non-object value.
  assert.equal(
    proto.parseCommand(
      JSON.stringify({ ...good, model_configs: { chat: { api: 'openai-completions', model: 'm' } } }),
    ),
    null,
  );
  assert.equal(
    proto.parseCommand(
      JSON.stringify({
        ...good,
        model_configs: { chat: { api: 'carrier-pigeon', base_url: 'https://x', model: 'm' } },
      }),
    ),
    null,
  );
  assert.equal(
    proto.parseCommand(JSON.stringify({ ...good, model_configs: { chat: 'nope' } })),
    null,
  );
  // A valid config parses with the api_key optional field carried verbatim.
  const withKey = proto.parseCommand(
    JSON.stringify({
      ...good,
      model_configs: { chat: { api: 'openai-completions', base_url: 'https://x', model: 'm', api_key: 'sk-1' } },
    }),
  ) as { model_configs?: Record<string, unknown> };
  assert.deepEqual(withKey.model_configs, {
    chat: { api: 'openai-completions', base_url: 'https://x', model: 'm', api_key: 'sk-1' },
  });
});

test('malformed abort, command, and permission_response are rejected', () => {
  assert.equal(proto.parseCommand('{"type":"abort","id":"a"}'), null, 'session_id required');
  assert.equal(proto.parseCommand('{"type":"command","id":"c","name":"ping"}'), null, 'args required');
  assert.equal(
    proto.parseCommand('{"type":"command","id":"c","name":"ping","args":[]}'),
    null,
    'args must be an object',
  );
  assert.equal(
    proto.parseCommand('{"type":"permission_response","id":"p","approved":"yes","always_allow":false}'),
    null,
    'booleans required',
  );
});

test('non-JSON and non-object lines are rejected without throwing', () => {
  assert.equal(proto.parseCommand('this is not json{{{'), null);
  assert.equal(proto.parseCommand('"just a string"'), null);
  assert.equal(proto.parseCommand('null'), null);
  assert.equal(proto.parseCommand(''), null);
});
