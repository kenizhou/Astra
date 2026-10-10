// Runs the BUILT bundle (dist/index.cjs) through the pi runtime path up to
// the network boundary; no credentials needed. Ported from the kylins
// sidecar's pi-bundle-smoke.mjs to the Astra protocol: init harness=pi with
// a model_configs entry pointing at an unreachable endpoint, prompt, and
// assert the connection refusal surfaces as llm_call error rows (a
// diagnosable failure, not a bundle crash), done arrives, and the sidecar
// stays alive afterwards.
import { spawn } from 'node:child_process';
import readline from 'node:readline';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const DIST = path.resolve('dist/index.cjs');
const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'astra-pi-smoke-'));
const child = spawn(process.execPath, [DIST], { stdio: ['pipe', 'pipe', 'pipe'] });
let stderrTail = '';
child.stderr.on('data', (d) => {
  stderrTail = (stderrTail + d).slice(-4000);
});

const rl = readline.createInterface({ input: child.stdout });
const queue = [];
const waiters = [];
rl.on('line', (l) => {
  const w = waiters.shift();
  if (w) w(l);
  else queue.push(l);
});
const next = () =>
  new Promise((res, rej) => {
    const t = setTimeout(() => rej(new Error('TIMEOUT waiting for stdout line')), 60000);
    const take = (l) => {
      clearTimeout(t);
      res(JSON.parse(l));
    };
    const q = queue.shift();
    if (q !== undefined) take(q);
    else waiters.push(take);
  });
const send = (o) => child.stdin.write(JSON.stringify(o) + '\n');

const results = [];
const check = (name, ok, detail) => results.push({ name, ok, detail });

// 1. init with the pi harness and one unreachable endpoint.
send({
  type: 'init',
  harness: 'pi',
  cli_path: '',
  data_dir: dataDir,
  model_configs: {
    chat: { api: 'openai-completions', base_url: 'http://127.0.0.1:1/v1', model: 'smoke-model' },
  },
});
const ready = await next();
check(
  'init harness=pi -> ready (the bundle loads the pi SDK)',
  ready.type === 'ready' && ready.harnesses?.includes('pi') && ready.models?.includes('smoke-model'),
  JSON.stringify(ready),
);

// 2. prompt: pi reaches the network boundary and surfaces the connection
//    refusal as llm_call error rows, then state:error, then done.
send({
  type: 'prompt',
  id: 'turn-smoke',
  session_id: 'agent:main',
  text: 'hello from the bundle smoke test',
  correlation_id: 'corr-smoke-1',
});
let llmErrors = 0;
let firstLlmError = null;
let gotStateError = false;
let gotDone = false;
let errorEnvelope = null;
while (!gotDone) {
  const env = await next();
  if (env.type === 'llm_call' && env.call?.status === 'error') {
    llmErrors += 1;
    if (!firstLlmError) firstLlmError = env.call.error;
    check(
      `llm_call row ${llmErrors} carries the turn correlation id`,
      env.call.correlation_id === 'corr-smoke-1',
      String(env.call.correlation_id),
    );
  } else if (env.type === 'event' && env.event?.kind === 'state' && env.event?.state === 'error') {
    gotStateError = true;
  } else if (env.type === 'error') {
    errorEnvelope = env.message;
  } else if (env.type === 'done' && env.id === 'turn-smoke') {
    gotDone = true;
  }
}
check(
  'prompt -> at least one llm_call error row (connection refused surfaced)',
  llmErrors >= 1 && /connect|fetch|ECONN|refus/i.test(firstLlmError ?? ''),
  `${llmErrors} rows, first error: ${firstLlmError}`,
);
check('prompt -> state error event (the user-visible failure signal)', gotStateError, '');
check('prompt -> done envelope (the host never hangs)', gotDone, '');
check('prompt -> no error envelope and no crash', errorEnvelope === null, errorEnvelope ?? '');

// 3. the sidecar stays alive and responsive after the failed prompt.
send({ type: 'command', id: 'cmd-alive', name: 'ping', args: {} });
const ping = await next();
check(
  'the sidecar answers ping after the failed prompt',
  ping.type === 'result' && ping.value?.ok === true,
  JSON.stringify(ping),
);

// 4. the session JSONL persisted under the app data dir.
const sessDir = path.join(dataDir, 'agent', 'sessions');
const files = fs.existsSync(sessDir) ? fs.readdirSync(sessDir) : [];
check(
  'the session JSONL persisted under <data_dir>/agent/sessions',
  files.some((f) => f.endsWith('.jsonl')),
  files.join(',') || '(none)',
);

child.kill();
fs.rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });

let failed = 0;
for (const r of results) {
  console.log(`${r.ok ? 'PASS' : 'FAIL'}  ${r.name}${r.detail ? `  -- ${r.detail}` : ''}`);
  if (!r.ok) failed += 1;
}
if (failed > 0 && stderrTail) console.log('--- stderr tail ---\n' + stderrTail);
console.log(failed === 0 ? 'ALL CHECKS PASSED' : `${failed} CHECK(S) FAILED`);
process.exit(failed === 0 ? 0 : 1);
