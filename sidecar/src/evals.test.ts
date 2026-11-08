// SPDX-License-Identifier: GPL-3.0-only
/**
 * The sidecar eval runner: the scenarios under the repository's evals/
 * directory whose seams live in this sidecar (features "budget" and
 * "agent-tools"), driven over the BUILT bundle exactly as the astra-app
 * harness drives its own: the runner lives beside the seam it exercises.
 * The budget's seam is the stdio loop's clock (a past-budget tool call
 * answers skipped, the watchdog lands done, the budget-spent state event
 * crosses); the agent tool surface's seam is the same loop's gate (a
 * search riding its auto policy answers its page with no ask, an act
 * verb through the confirm gate runs only on the host's yes, and the
 * read's four-thousand-character bound cuts with its marker). Each
 * scenario spawns dist/index.cjs under the mock harness with the budget
 * env overrides its inputs carry, speaks JSONL, and asserts the wire.
 * Run `npm run build` first.
 *
 * The astra-app eval harness (crates/astra-app/src/evals) parses every
 * file in evals/ and pin-checks these scenarios but does not drive them;
 * this file is their runner, so a scenario that moves fails here.
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
const EVALS_DIR = path.resolve(__dirname, '..', '..', 'evals');

interface ScenarioInputs {
  tool_budget_ms: number;
  budget_grace_ms?: number;
  chunks: string[];
  chunk_delay_ms?: number;
  tool_call?: { name: string; args: unknown };
  policy?: Record<string, { level: string }>;
  /** The JSON document the stub CLI answers instead of its echo: the
   *  seeded page a real verb would print (a search page, say). */
  cli_page?: unknown;
  /** Grows the stub CLI's stdout by this many characters, past a tool's
   *  truncation bound. */
  stub_pad?: number;
}

interface ScenarioExpected {
  budget_event?: boolean | { session_id: string; state: string };
  no_budget_event?: boolean;
  tool_result?: {
    tool: string;
    status: string;
    output_includes?: string;
    truncated_at?: number;
  };
  max_text_deltas?: number;
  done?: boolean;
  /** The search leg: no permission ask may cross, and the model receives
   *  exactly the seeded page. */
  no_permission_ask?: boolean;
  output_equals_page?: boolean;
  /** The confirm legs: the ask crosses with this shape and this answer
   *  goes back. */
  permission_ask?: { tool: string; level: string };
  answer?: boolean;
  /** The approved leg: the CLI ran, its capture naming this verb word. */
  cli_captured_verb?: string;
  /** The rejected leg: the CLI never ran at all. */
  cli_never_ran?: boolean;
}

interface Scenario {
  id: string;
  feature: 'budget' | 'agent-tools';
  mode: 'offline';
  inputs: ScenarioInputs;
  expected: ScenarioExpected;
}

const children: ChildProcess[] = [];
const tmpDirs: string[] = [];
after(() => {
  for (const c of children) c.kill();
  for (const d of tmpDirs) {
    try {
      fs.rmSync(d, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
    } catch (e) {
      process.stderr.write(`evals.test: failed to remove ${d}: ${String(e)}\n`);
    }
  }
});

/** Every scenario fixture in evals/ carrying the feature word, file name
 *  order: the list this runner drives, so a fixture landing there without
 *  a home here fails on the wire, never silently. */
function scenariosFor(feature: 'budget' | 'agent-tools'): Scenario[] {
  const prefix = feature === 'budget' ? 'budget-' : 'agent-tools-';
  const names = fs
    .readdirSync(EVALS_DIR)
    .filter((n) => n.startsWith(prefix) && n.endsWith('.json'))
    .sort();
  assert.ok(names.length > 0, `evals/ must hold ${feature} scenarios`);
  return names.map((name) => {
    const text = fs.readFileSync(path.join(EVALS_DIR, name), 'utf8');
    const parsed = JSON.parse(text) as Scenario;
    assert.equal(parsed.feature, feature, `${name} must carry the ${feature} feature word`);
    return parsed;
  });
}

/** A temp data dir with the stub CLI under the verb words, so a tool the
 *  budget or policy lets through really executes. */
function stubDataDir(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'astra-budget-evals-'));
  tmpDirs.push(dir);
  const stub = fs.readFileSync(path.resolve(__dirname, 'fixtures', 'stub-cli.cjs'), 'utf8');
  for (const verb of ['folders', 'messages', 'drafts']) {
    fs.writeFileSync(path.join(dir, verb), stub);
  }
  return dir;
}

interface Spawned {
  nextLine: () => Promise<Record<string, unknown>>;
  send: (obj: unknown) => void;
}

function spawnSidecar(env: NodeJS.ProcessEnv): Spawned {
  assert.ok(fs.existsSync(DIST), 'dist/index.cjs is missing; run npm run build first');
  const child = spawn(process.execPath, [DIST], { stdio: ['pipe', 'pipe', 'pipe'], env });
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
      const timer = setTimeout(
        () => reject(new Error('timed out waiting for stdout line')),
        8000,
      );
      const onLine = (line: string): void => {
        clearTimeout(timer);
        resolve(JSON.parse(line) as Record<string, unknown>);
      };
      const queued = queue.shift();
      if (queued !== undefined) onLine(queued);
      else waiters.push(onLine);
    });
  return {
    nextLine,
    send: (obj: unknown) => {
      child.stdin!.write(JSON.stringify(obj) + '\n');
    },
  };
}

/** Drive one scenario over the wire: init under the mock harness with the
 *  fixture its inputs describe, one prompt, every envelope up to its done.
 *  A permission ask the scenario expects is answered as its fixture says;
 *  the stub data dir rides back for the executed-or-not assertions. */
async function driveScenario(
  s: Scenario,
): Promise<{ envelopes: Record<string, unknown>[]; dataDir: string }> {
  const dataDir = stubDataDir();
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'astra-budget-eval-'));
  tmpDirs.push(dir);
  const fixturePath = path.join(dir, 'fixture.json');
  const fixture: Record<string, unknown> = {
    chunks: s.inputs.chunks,
    chunk_delay_ms: s.inputs.chunk_delay_ms ?? 1,
  };
  if (s.inputs.tool_call !== undefined) fixture.tool_call = s.inputs.tool_call;
  fs.writeFileSync(fixturePath, JSON.stringify(fixture));

  const env: NodeJS.ProcessEnv = {
    ...process.env,
    ASTRA_AGENT_TOOL_BUDGET_MS: String(s.inputs.tool_budget_ms),
  };
  if (s.inputs.budget_grace_ms !== undefined) {
    env.ASTRA_AGENT_TOOL_BUDGET_GRACE_MS = String(s.inputs.budget_grace_ms);
  }
  if (s.inputs.cli_page !== undefined) {
    env.ASTRA_STUB_PAGE = JSON.stringify(s.inputs.cli_page);
  }
  if (s.inputs.stub_pad !== undefined) {
    env.ASTRA_STUB_PAD = String(s.inputs.stub_pad);
  }

  const sidecar = spawnSidecar(env);
  const init: Record<string, unknown> = {
    type: 'init',
    harness: 'mock',
    cli_path: process.execPath,
    data_dir: dataDir,
    mock_fixture: fixturePath,
  };
  if (s.inputs.policy !== undefined) init.policy = s.inputs.policy;
  sidecar.send(init);
  const ready = await sidecar.nextLine();
  assert.equal(ready.type, 'ready', `scenario ${s.id}: the handshake completes`);

  sidecar.send({
    type: 'prompt',
    id: `turn-${s.id}`,
    session_id: 'agent:main',
    text: 'clean the inbox',
    correlation_id: `corr-${s.id}`,
  });
  const envelopes: Record<string, unknown>[] = [];
  for (;;) {
    const envLine = await sidecar.nextLine();
    envelopes.push(envLine);
    if (envLine.type === 'permission_request') {
      assert.ok(
        s.expected.permission_ask !== undefined,
        `scenario ${s.id}: an ask crossed the wire the scenario never expected`,
      );
      sidecar.send({
        type: 'permission_response',
        id: envLine.id,
        approved: s.expected.answer ?? false,
        always_allow: false,
      });
    }
    if (envLine.type === 'done') break;
  }
  return { envelopes, dataDir };
}

function isStateEvent(e: Record<string, unknown>, state: string): boolean {
  if (e.type !== 'event') return false;
  const event = e.event as { kind?: string; state?: string } | undefined;
  return event?.kind === 'state' && event.state === state;
}

/** The stub CLI's captured invocations, if any: the proof an approved act
 *  really reached the CLI and a rejected one never did. */
function stubCaptures(dataDir: string): Array<{ argv: string[] }> {
  const file = path.join(dataDir, 'stub-captures.jsonl');
  if (!fs.existsSync(file)) return [];
  return fs
    .readFileSync(file, 'utf8')
    .split('\n')
    .filter((line) => line.trim() !== '')
    .map((line) => JSON.parse(line) as { argv: string[] });
}

/** Assert one scenario's expected outcomes against its collected wire. */
function assertExpected(s: Scenario, envelopes: Record<string, unknown>[], dataDir: string): void {
  const spentEvents = envelopes.filter((e) => isStateEvent(e, 'budget-spent'));

  if (s.expected.budget_event !== undefined) {
    // Exactly once, not at least once: the runner's announced flag is the
    // only guard against a double note in the panel's log, so a regression
    // to two announcements must fail here.
    assert.equal(
      spentEvents.length,
      1,
      `scenario ${s.id}: the budget-spent state event crosses the wire exactly once: ${JSON.stringify(envelopes.map((e) => e.type))}`,
    );
    const shape = s.expected.budget_event;
    if (typeof shape === 'object') {
      assert.equal(spentEvents[0]!.session_id, shape.session_id);
      assert.equal(
        (spentEvents[0]!.event as { state: string }).state,
        shape.state,
      );
    }
  }
  if (s.expected.no_budget_event) {
    assert.equal(
      spentEvents.length,
      0,
      `scenario ${s.id}: a turn inside its budget never announces the bound`,
    );
  }

  const asks = envelopes.filter((e) => e.type === 'permission_request');
  if (s.expected.no_permission_ask) {
    assert.equal(
      asks.length,
      0,
      `scenario ${s.id}: an auto-policy read asks nobody: ${JSON.stringify(asks)}`,
    );
  }
  if (s.expected.permission_ask !== undefined) {
    const want = s.expected.permission_ask;
    assert.equal(asks.length, 1, `scenario ${s.id}: the confirm gate asks once`);
    assert.equal(asks[0]!.tool, want.tool, `scenario ${s.id}: the ask names its tool`);
    assert.equal(asks[0]!.level, want.level, `scenario ${s.id}: the ask carries its level`);
  }

  if (s.expected.tool_result !== undefined) {
    const want = s.expected.tool_result;
    const tr = envelopes.find((e) => e.type === 'tool_result');
    assert.ok(tr, `scenario ${s.id}: a tool_result envelope must land, never a silent drop`);
    assert.equal(tr.tool, want.tool);
    assert.equal(tr.status, want.status, `scenario ${s.id}: the audited outcome`);
    if (want.output_includes !== undefined) {
      assert.match(
        String(tr.output ?? ''),
        new RegExp(want.output_includes),
        `scenario ${s.id}: the model is told why`,
      );
    }
    if (want.truncated_at !== undefined) {
      const marker = `[truncated at ${want.truncated_at} characters]`;
      const output = String(tr.output ?? '');
      assert.equal(
        output.length,
        want.truncated_at + marker.length,
        `scenario ${s.id}: the read is cut exactly at its bound with the marker`,
      );
      assert.ok(
        output.endsWith(marker),
        `scenario ${s.id}: the cut names its bound: ${output.slice(-60)}`,
      );
    }
  }

  if (s.expected.output_equals_page) {
    const tr = envelopes.find((e) => e.type === 'tool_result');
    assert.ok(tr, `scenario ${s.id}: the search's page must cross the wire`);
    assert.deepEqual(
      JSON.parse(String(tr!.output ?? '')),
      s.inputs.cli_page,
      `scenario ${s.id}: the model receives the seeded page shape verbatim`,
    );
  }

  if (s.expected.cli_captured_verb !== undefined) {
    const verb = s.expected.cli_captured_verb;
    const ran = stubCaptures(dataDir).some((capture) => capture.argv.includes(verb));
    assert.ok(ran, `scenario ${s.id}: the approved act really reached the CLI (${verb})`);
  }
  if (s.expected.cli_never_ran) {
    assert.equal(
      stubCaptures(dataDir).length,
      0,
      `scenario ${s.id}: a rejected act executes nothing, not one CLI spawn`,
    );
  }

  if (s.expected.max_text_deltas !== undefined) {
    const deltas = envelopes.filter((e) => {
      if (e.type !== 'event') return false;
      return (e.event as { kind?: string }).kind === 'text_delta';
    }).length;
    assert.ok(
      deltas <= s.expected.max_text_deltas,
      `scenario ${s.id}: the watchdog cut the turn, saw ${deltas} deltas`,
    );
  }

  if (s.expected.done) {
    const last = envelopes[envelopes.length - 1]!;
    assert.equal(last.type, 'done', `scenario ${s.id}: the turn lands done, never a hang`);
    assert.equal(last.id, `turn-${s.id}`);
  }
}

for (const scenario of [...scenariosFor('budget'), ...scenariosFor('agent-tools')]) {
  test(`eval scenario ${scenario.id} holds over the built bundle`, async () => {
    const { envelopes, dataDir } = await driveScenario(scenario);
    assertExpected(scenario, envelopes, dataDir);
  });
}
