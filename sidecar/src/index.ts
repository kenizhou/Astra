// SPDX-License-Identifier: GPL-3.0-only
/**
 * Sidecar entrypoint. The Rust host spawns
 *   node dist/index.cjs
 * writes JSONL commands to stdin and reads JSONL envelopes from stdout;
 * stderr carries human logs only. Ported from the kylins sidecar's loop and
 * runner, trimmed to chat: the init handshake answers ready; prompt, abort,
 * command, and permission_response arrive as lines; event, done, error,
 * result, llm_call, permission_request, and tool_result envelopes go out.
 */
import * as readline from 'node:readline';
import {
  parseCommand,
  writeEnvelope,
  type InCommand,
  type LlmCallData,
  type ModelConfig,
  type OutEnvelope,
} from './protocol';
import {
  BUDGET_SPENT_STATE,
  TurnBudget,
  resolveToolBudgetGraceMs,
  resolveToolBudgetMs,
} from './budget';
import { MockHarness } from './harness/mock';
import { PiAdapter } from './harness/pi-adapter';
import type { AgentHarness, HarnessEvent, HarnessSession } from './harness/types';
import { PermissionBridge, type PermissionPolicy } from './permissions/bridge';
import { createMailTools } from './tools/mailTools';

let harness: AgentHarness | null = null;
const sessions = new Map<string, HarnessSession>();

/** Init-delivered astra-cli path; null until init (or the host could not
 *  resolve the binary, in which case safe_exec fails loudly at call time). */
let cliPath: string | null = null;
function getCliPath(): string | null {
  return cliPath;
}

/** Init-delivered app data dir; null until init. Degrades safe_exec to a
 *  loud stderr warning, never a hard error. */
let dataDir: string | null = null;
function getDataDir(): string | null {
  return dataDir;
}

/** tier name to endpoint config, from init's model_configs. */
const modelConfigs = new Map<string, ModelConfig>();

/** The tier name the llm_call ledger rows carry: the mock harness reports
 *  'mock'; the pi harness reports the one configured tier, or 'unconfigured'
 *  when the host delivered none (rows stay honest about what ran). */
function ledgerTier(): string {
  if (harness?.id === 'mock') return 'mock';
  for (const tier of modelConfigs.keys()) return tier;
  return 'unconfigured';
}

/** Live permission policy table. init fills it in place so the bridge, which
 *  reads through this reference, sees the delivery immediately. Empty until
 *  then: every tool fails closed to needs-approval. */
const policyTable: Record<string, PermissionPolicy> = {};
const bridge = new PermissionBridge({ policy: () => policyTable });

/** Live view of the current turn's correlation id, set by the prompt branch
 *  BEFORE the prompt runs so every tool spawn of the turn carries it. */
const correlation: { current: string | null } = { current: null };

/** The tool budget every prompt runs under and the watchdog grace past it,
 *  resolved once at startup from the environment (the harness and the
 *  tests arm short budgets there). */
const TOOL_BUDGET_MS = resolveToolBudgetMs(process.env);
const TOOL_BUDGET_GRACE_MS = resolveToolBudgetGraceMs(process.env);

/** Live view of the current turn's tool budget; null between turns. One
 *  live turn at a time, enforced by the runner's refusal below: the view
 *  is single-slot, the same shape the correlation holder uses, and both
 *  are safe only while one prompt runs (a second live prompt would reset
 *  the first's clock and capture its correlation id). The runner's
 *  cleanup only clears its own turn's view. */
const turnBudget: { current: TurnBudget | null } = { current: null };

/** The five mail tools, shared by both harnesses (pi mounts them as
 *  customTools; the mock dispatches them for fixture-driven e2e). Deps are
 *  live views, so one instance serves every session, and both harnesses
 *  gate through the same budget view. */
const mailTools = createMailTools({
  bridge,
  cliPath: getCliPath,
  dataDir: getDataDir,
  correlationId: () => correlation.current,
  budget: () => turnBudget.current,
});

/** One prompt's ledger context while it runs. */
interface PromptCtx {
  promptId: string;
  correlationId: string;
}

/** Drives prompt commands into a HarnessSession and guarantees every prompt
 *  ends with a done envelope (the finally always sends it: aborts and
 *  exceptions are no exception, so the host never hangs). Inlined from the
 *  kylins runner, chat-only. Each turn runs under the wall-clock tool
 *  budget: the clock starts here at prompt receipt, the budget-spent state
 *  event crosses at the deadline, and a watchdog aborts the turn at the cap
 *  (the budget plus a grace) through the same machinery a host abort uses,
 *  so the done envelope lands on every path, text-only runs included. */
class PromptRunner {
  private readonly byPromptId = new Map<string, {
    session: HarnessSession;
    cancelWatchdog: () => void;
  }>();
  private readonly activeCtx = new Map<HarnessSession, PromptCtx>();
  private readonly wired = new WeakSet<HarnessSession>();

  async runPrompt(session: HarnessSession, cmd: InCommand & { type: 'prompt' }): Promise<void> {
    if (!this.wired.has(session)) {
      const sessionId = cmd.session_id;
      // The wiring happens once per session, so the first prompt's tier and
      // caller are this session's identity for every ledger row it produces.
      const tier = ledgerTier();
      const caller = 'agent-chat';
      session.subscribe((event) => {
        if (event.kind === 'llm_call') {
          const usage = event.usage;
          const call: LlmCallData = {
            session_id: sessionId,
            tier,
            caller,
            model: usage.model,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            cache_read_tokens: usage.cache_read_tokens,
            cache_write_tokens: usage.cache_write_tokens,
            duration_ms: usage.duration_ms,
            status: usage.status,
          };
          const ctx = this.activeCtx.get(session);
          if (ctx) {
            call.prompt_id = ctx.promptId;
            call.correlation_id = ctx.correlationId;
          }
          if (usage.error !== undefined) call.error = usage.error;
          writeEnvelope({ type: 'llm_call', call });
          return; // ledger bypass: never forwarded as a UI event
        }
        if (event.kind === 'tool_result') {
          // The audited tool outcome: its own envelope type, same bypass
          // shape as llm_call.
          const env: OutEnvelope = {
            type: 'tool_result',
            tool_call_id: event.tool_call_id,
            tool: event.name,
            status: event.status,
          };
          if (event.output !== undefined) env.output = event.output;
          writeEnvelope(env);
          return;
        }
        writeEnvelope({ type: 'event', session_id: sessionId, event });
      });
      this.wired.add(session);
    }
    // The budget clock starts at prompt receipt: the view the tool gate
    // consults, the deadline announcement, and the watchdog cap.
    const budget = new TurnBudget(TOOL_BUDGET_MS);
    turnBudget.current = budget;
    let announcedSpent = false;
    const announceSpent = (): void => {
      if (announcedSpent) return;
      announcedSpent = true;
      writeEnvelope({
        type: 'event',
        session_id: cmd.session_id,
        event: { kind: 'state', state: BUDGET_SPENT_STATE },
      });
    };
    const deadlineTimer = setTimeout(announceSpent, budget.remainingMs());
    const capTimer = setTimeout(() => {
      announceSpent();
      session.abort();
      bridge.rejectAll('tool budget cap reached');
    }, TOOL_BUDGET_MS + TOOL_BUDGET_GRACE_MS);
    // Unref'd so a pending timer never holds the process open; the turn's
    // own IO keeps the loop alive while the prompt runs.
    deadlineTimer.unref();
    capTimer.unref();
    const cancelWatchdog = (): void => {
      clearTimeout(deadlineTimer);
      clearTimeout(capTimer);
    };
    this.byPromptId.set(cmd.id, { session, cancelWatchdog });
    this.activeCtx.set(session, { promptId: cmd.id, correlationId: cmd.correlation_id });
    try {
      await session.prompt(cmd.text, {
        session_id: cmd.session_id,
        correlation_id: cmd.correlation_id,
      });
    } catch (e) {
      writeEnvelope({ type: 'error', id: cmd.id, message: String(e) });
    } finally {
      cancelWatchdog();
      if (turnBudget.current === budget) turnBudget.current = null;
      this.byPromptId.delete(cmd.id);
      this.activeCtx.delete(session);
      writeEnvelope({ type: 'done', id: cmd.id });
    }
  }

  abort(promptId: string): void {
    const entry = this.byPromptId.get(promptId);
    if (!entry) return;
    // A host-aborted turn never sees the watchdog's events; the finally in
    // runPrompt lands the done envelope.
    entry.cancelWatchdog();
    entry.session.abort();
  }

  /** Whether a prompt is still running. The turn invariant: the budget and
   *  correlation holders are single-slot module state, safe only while one
   *  turn is live, so the command loop refuses a second prompt while this
   *  answers true rather than letting it reset the live turn's clock or
   *  bounce its pending approval. */
  hasLive(): boolean {
    return this.byPromptId.size > 0;
  }
}

const runner = new PromptRunner();

async function handleCommand(cmd: InCommand): Promise<void> {
  switch (cmd.type) {
    case 'init': {
      cliPath = cmd.cli_path || null;
      dataDir = cmd.data_dir;
      modelConfigs.clear();
      for (const [tier, cfg] of Object.entries(cmd.model_configs ?? {})) {
        modelConfigs.set(tier, cfg);
      }
      // Fill the table in place: the bridge holds this reference.
      for (const key of Object.keys(policyTable)) delete policyTable[key];
      Object.assign(policyTable, cmd.policy ?? {});
      if (cmd.harness === 'mock') {
        harness = new MockHarness({ fixturePath: cmd.mock_fixture, tools: mailTools });
      } else if (cmd.harness === 'pi') {
        harness = new PiAdapter({ dataDir: cmd.data_dir, modelConfigs, mailTools });
      } else {
        writeEnvelope({ type: 'error', id: null, message: `unknown harness: ${cmd.harness}` });
        return;
      }
      // The mock reports its fixture model; pi's configured models are the
      // init delivery, so the ready list covers exactly what init knows.
      const models =
        harness.id === 'mock'
          ? ['mock-1']
          : [...new Set([...modelConfigs.values()].map((c) => c.model))];
      writeEnvelope({ type: 'ready', models, harnesses: ['mock', 'pi'] });
      break;
    }
    case 'prompt': {
      if (!harness) {
        writeEnvelope({ type: 'error', id: cmd.id, message: 'prompt before init' });
        return;
      }
      // The turn invariant, enforced here: one live turn per sidecar. A
      // second prompt (per-chat sessions make overlapping prompts
      // reachable) is refused with an honest error envelope BEFORE any
      // single-slot state is touched, so the live turn keeps its budget
      // clock, its correlation id, and its pending approval.
      if (runner.hasLive()) {
        writeEnvelope({
          type: 'error',
          id: cmd.id,
          message: 'a turn is already live; wait for its done or abort it before prompting again',
        });
        return;
      }
      // The correlation holder is set BEFORE the prompt runs so every tool
      // spawn of the turn stamps the id.
      correlation.current = cmd.correlation_id;
      let session = sessions.get(cmd.session_id);
      if (!session) {
        session = await harness.createSession({
          harnessOptions: { sessionId: cmd.session_id },
        });
        sessions.set(cmd.session_id, session);
      }
      // Deliberately not awaited: stdin must keep reading while a prompt
      // runs (abort lines can only arrive that way).
      void runner.runPrompt(session, cmd);
      break;
    }
    case 'abort': {
      runner.abort(cmd.id);
      // A tool suspended in requestApproval would otherwise hang the prompt
      // forever: reject every pending approval so its execute body settles
      // with tool_result status=skipped. Late host responses for those ids
      // are unknown-id warnings (bridge.respond), never throws.
      bridge.rejectAll('prompt aborted');
      break;
    }
    case 'command': {
      // Every command gets an answer: result on success, an error envelope
      // carrying the command id on failure; the host never hangs waiting.
      try {
        if (cmd.name !== 'ping') {
          throw new Error(`unknown command: ${cmd.name}`);
        }
        writeEnvelope({
          type: 'result',
          id: cmd.id,
          value: { ok: true, harness: harness?.id ?? null },
        });
      } catch (e) {
        writeEnvelope({ type: 'error', id: cmd.id, message: String(e) });
      }
      break;
    }
    case 'permission_response': {
      bridge.respond(cmd.id, cmd.approved, cmd.always_allow);
      break;
    }
  }
}

const rl = readline.createInterface({ input: process.stdin });
rl.on('line', (line: string) => {
  const cmd = parseCommand(line);
  if (!cmd) {
    process.stderr.write(`sidecar: skipping unparsable line: ${line}\n`);
    return;
  }
  void handleCommand(cmd);
});

// When the host exits (stdin closes) we exit too: zombie protection in both
// directions.
process.stdin.on('end', () => process.exit(0));
process.stdout.on('error', (e: NodeJS.ErrnoException) => {
  if (e.code === 'EPIPE') process.exit(0);
  process.stderr.write(`sidecar: stdout error: ${String(e)}\n`);
});
process.on('uncaughtException', (e) => {
  writeEnvelope({ type: 'error', id: null, message: String(e) });
  process.exit(1);
});
// handleCommand is async and gets voided (stdin must keep reading during a
// prompt), so any awaited rejection in it (a createSession failure) is an
// unhandledRejection. Handle it explicitly: emit an error envelope, exit.
process.on('unhandledRejection', (reason) => {
  writeEnvelope({ type: 'error', id: null, message: `unhandled rejection: ${String(reason)}` });
  process.exit(1);
});
