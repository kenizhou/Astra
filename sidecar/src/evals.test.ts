// SPDX-License-Identifier: GPL-3.0-only
/**
 * The budget eval runner: the scenarios under the repository's evals/
 * directory (feature "budget"), driven over the BUILT bundle exactly as
 * the astra-app harness drives its own: the runner lives beside the seam
 * it exercises, and the budget's seam is the sidecar's stdio loop. Each
 * scenario spawns dist/index.cjs under the mock harness with the budget
 * env overrides its inputs carry, speaks JSONL, and asserts the wire:
 * a past-budget tool call answers skipped, the watchdog lands done, and
 * the budget-spent state event crosses. Run `npm run build` first.
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
}

interface ScenarioExpected {
  budget_event?: boolean | { session_id: string; state: string };
  no_budget_event?: boolean;
  tool_result?: { tool: string; status: string; output_includes?: string };
  max_text_deltas?: number;
  done?: boolean;
}

interface Scenario {
  id: string;
  feature: 'budget';
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

/** Every budget scenario fixture in evals/, file name order: the list this
 *  runner drives, so a fixture landing there without a home here fails on
 *  the wire, never silently. */
function budgetScenarios(): Scenario[] {
  const names = fs
    .readdirSync(EVALS_DIR)
    .filter((n) => n.startsWith('budget-') && n.endsWith('.json'))
    .sort();
  assert.ok(names.length > 0, 'evals/ must hold budget scenarios');
  return names.map((name) => {
    const text = fs.readFileSync(path.join(EVALS_DIR, name), 'utf8');
    const parsed = JSON.parse(text) as Scenario;
    assert.equal(parsed.feature, 'budget', `${name} must carry the budget feature word`);
    return parsed;
  });
}

/** A temp data dir with the stub CLI under the verb words, so a tool the
 *  budget lets through really executes. */
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
 *  fixture its inputs describe, one prompt, every envelope up to its done. */
async function driveScenario(s: Scenario): Promise<Record<string, unknown>[]> {
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

  const sidecar = spawnSidecar(env);
  const init: Record<string, unknown> = {
    type: 'init',
    harness: 'mock',
    cli_path: process.execPath,
    data_dir: stubDataDir(),
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
    if (envLine.type === 'done') break;
  }
  return envelopes;
}

function isStateEvent(e: Record<string, unknown>, state: string): boolean {
  if (e.type !== 'event') return false;
  const event = e.event as { kind?: string; state?: string } | undefined;
  return event?.kind === 'state' && event.state === state;
}

/** Assert one scenario's expected outcomes against its collected wire. */
function assertExpected(s: Scenario, envelopes: Record<string, unknown>[]): void {
  const spentEvents = envelopes.filter((e) => isStateEvent(e, 'budget-spent'));

  if (s.expected.budget_event !== undefined) {
    assert.ok(
      spentEvents.length >= 1,
      `scenario ${s.id}: the budget-spent state event must cross the wire: ${JSON.stringify(envelopes.map((e) => e.type))}`,
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

for (const scenario of budgetScenarios()) {
  test(`eval scenario ${scenario.id} holds over the built bundle`, async () => {
    const envelopes = await driveScenario(scenario);
    assertExpected(scenario, envelopes);
  });
}
