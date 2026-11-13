// SPDX-License-Identifier: GPL-3.0-only
/**
 * The shared gated-tool plumbing every virtual permission tool rides:
 * the budget-then-bridge gate (runGated), the CLI argv grammar
 * (buildCliArgv), the safe_exec spawn and output shaping (execCli), and
 * the defineMailTool identity helper. Extracted from mailTools.ts so the
 * key tools (keyTools.ts) share the one gate and the mail tools keep
 * their headroom under the line cap.
 *
 * The gate's order is load-bearing: the turn budget first (a spent budget
 * never opens a permission card), then the bridge's fail-closed check (an
 * absent policy row means needs-approval), then the spawn. The tools
 * NEVER throw: the outcome rides in details (MailToolDetails) and the
 * harness turns it into the audited tool_result envelope. A non-zero CLI
 * exit is still 'ok' with the exit envelope in the output for the model
 * to read (the safe_exec contract); only spawn-level failures and gate
 * throws map to 'error'.
 *
 * pi 0.84.2 customTools shape (verified against the kylins adapter and
 * dist/core/extensions/types.d.ts): { name, label, description,
 * parameters: a TypeBox schema, execute(toolCallId, params, signal,
 * onUpdate, ctx) => Promise<AgentToolResult<TDetails>> }.
 */
import type { TSchema } from 'typebox';
import type { AgentToolResult, ToolDefinition } from '@earendil-works/pi-coding-agent';
import type { PermissionBridge } from '../permissions/bridge';
import type { ToolResultStatus } from '../harness/types';
import type { BudgetView } from '../budget';
import { runSafeExec } from '../exec/safeExec';

/** The argv prefix token every tool's command starts with; the logical
 *  prefix safe_exec checks its allowlist against. */
export const CLI_PREFIX = 'astra-cli';

/** Structured tool outcome; the harness translates it into the wire
 *  tool_result envelope. Shared by the mail and key tools. */
export interface MailToolDetails {
  status: ToolResultStatus;
  output?: string;
}

/** Identity helper mirroring pi's defineTool: preserves TParams inference
 *  through the array literal WITHOUT a runtime import of the pi SDK in the
 *  tests (type-only imports keep the esbuild test bundles small); the
 *  intersection is what lets heterogeneous tools share one array. */
export function defineMailTool<TParams extends TSchema>(
  tool: ToolDefinition<TParams, MailToolDetails>,
): ToolDefinition<TParams, MailToolDetails> & ToolDefinition<any, any, any> {
  return tool as ToolDefinition<TParams, MailToolDetails> & ToolDefinition<any, any, any>;
}

export interface MailToolDeps {
  bridge: PermissionBridge;
  /** Live view of the init-delivered astra-cli path. */
  cliPath: () => string | null;
  /** Live view of the init-delivered data dir. */
  dataDir?: () => string | null;
  /** Live view of the current turn's correlation id, set by the stdio loop
   *  before a prompt runs; it rides the argv flag and the spawn env. */
  correlationId: () => string | null;
  /** Live view of the current turn's tool budget. The deadline reaches the
   *  gate as this view handed over at construction, never a clock threaded
   *  through every call: the tools are built once and shared by every
   *  session, so the live view is the one seam that stays per-turn. A null
   *  view (between turns) or an absent dep (unit constructions) means no
   *  clock stands and the gate never skips for budget. */
  budget?: () => BudgetView | null;
}

function textResult(text: string, details: MailToolDetails): AgentToolResult<MailToolDetails> {
  return { content: [{ type: 'text', text }], details };
}

/** Gate plus outcome mapping shared by every CLI-backed tool. The tool
 *  budget is the first gate: past the bound the model is told no (the
 *  skipped outcome the harness already reports, its audit row landing
 *  exactly as a refused tool's does through the same tool_result
 *  envelope), never a silent drop, and no permission card opens for a
 *  call that cannot run. */
export async function runGated(
  deps: MailToolDeps,
  toolCallId: string,
  tool: string,
  args: Record<string, unknown>,
  exec: () => Promise<string>,
): Promise<AgentToolResult<MailToolDetails>> {
  const budget = deps.budget?.() ?? null;
  if (budget !== null && budget.spent()) {
    const msg = "the turn's tool budget is spent";
    return textResult(`${tool}: skipped (${msg}; finish in text)`, {
      status: 'skipped',
      output: msg,
    });
  }
  if (deps.bridge.check(tool) === 'needs-approval') {
    let outcome: { approved: boolean };
    try {
      outcome = await deps.bridge.requestApproval({
        toolCallId,
        tool,
        level: deps.bridge.levelOf(tool),
        args,
      });
    } catch (e) {
      // rejectAll fired while suspended (prompt abort): no decision exists
      // and nothing executed; skipped, not error.
      const msg = e instanceof Error ? e.message : String(e);
      return textResult(`${tool}: skipped (${msg})`, { status: 'skipped', output: msg });
    }
    if (!outcome.approved) {
      return textResult(`${tool}: rejected by the user; nothing was executed`, {
        status: 'rejected',
      });
    }
  }
  // auto / notify / approved: run. notify's tell-the-panel leg is the
  // tool_result envelope itself.
  try {
    const output = await exec();
    return textResult(output, { status: 'ok', output });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    return textResult(`${tool} failed: ${msg}`, { status: 'error', output: msg });
  }
}

/** Assemble the CLI argv: prefix token, the two positionals, then --json,
 *  then the correlation flag pair when a turn id is live, then the verb's
 *  flags. docs/cli.md pins the grammar and the free flag order within. */
export function buildCliArgv(
  correlationId: string | null,
  positional: [string, string],
  flags: string[],
): string[] {
  const argv = [CLI_PREFIX, positional[0], positional[1], '--json'];
  if (correlationId) argv.push('--correlation-id', correlationId);
  argv.push(...flags);
  return argv;
}

/** Run one CLI invocation through safe_exec and shape the tool output: the
 *  CLI's JSON envelope document verbatim on success, a readable one-liner
 *  when the CLI failed without printing one. */
export async function execCli(deps: MailToolDeps, argv: string[]): Promise<string> {
  const r = await runSafeExec(argv, {
    cliPath: deps.cliPath(),
    allowlist: [CLI_PREFIX],
    dataDir: deps.dataDir?.() ?? null,
    correlationId: deps.correlationId(),
  });
  if (r.exitCode !== 0 && r.stdout.trim().length === 0) {
    return `astra-cli exited ${r.exitCode}: ${r.stderr.trim()}`;
  }
  return r.stdout.trim();
}
