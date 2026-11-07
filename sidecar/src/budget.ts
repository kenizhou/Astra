// SPDX-License-Identifier: GPL-3.0-only
/**
 * The assistant chat's tool budget: the wall clock one prompt's tool calls
 * run under. The clock starts at prompt receipt (the PromptRunner arms one
 * TurnBudget per turn); past the deadline the tool gate answers skipped and
 * a watchdog aborts the turn at the cap (the budget plus a grace), so the
 * done-envelope guarantee holds on every path. The behaviour is ported
 * from the inbox-zero assistant chat (apps/web/utils/ai/assistant/chat.ts,
 * ASSISTANT_CHAT_TOOL_BUDGET_MS): the contract and semantics with
 * attribution, re-expressed in Astra's own idiom under GPL-3.0-only, per
 * the licence posture the SP2y spec records; where the reference gates at
 * prepareStep, the agent runtime has no per-step hook, so Astra's bound
 * bites at tool execute plus the watchdog.
 */

/** The state word the budget-spent event carries on the wire. */
export const BUDGET_SPENT_STATE = 'budget-spent';

/** The tool budget one chat turn runs under, in milliseconds: the
 *  reference gives its web chat 720 seconds, and Astra has one chat
 *  class, so one constant. */
export const DEFAULT_TOOL_BUDGET_MS = 720_000;

/** The grace the watchdog adds to the budget before aborting the turn:
 *  long enough for a text-only finish, short enough that a stuck turn
 *  cannot hang the panel. The reference's own route cap leaves 80 seconds
 *  of grace beside its 720-second budget; Astra takes a clean minute. */
export const DEFAULT_TOOL_BUDGET_GRACE_MS = 60_000;

/** The milliseconds the named environment variable carries, or the
 *  fallback. The harness and the tests arm short budgets this way (the
 *  reference keys its per-surface values in code; Astra's one surface
 *  reads one env override). A malformed or negative value warns on stderr
 *  and stands down to the fallback: a debug knob never breaks the sidecar. */
function resolveMs(env: NodeJS.ProcessEnv, name: string, fallback: number): number {
  const raw = env[name];
  if (raw === undefined || raw === '') return fallback;
  const value = Number(raw);
  if (!Number.isFinite(value) || value < 0) {
    process.stderr.write(`budget: ignoring malformed ${name}=${raw}; using ${fallback}\n`);
    return fallback;
  }
  return value;
}

/** The tool budget every prompt runs under, from the environment. */
export function resolveToolBudgetMs(env: NodeJS.ProcessEnv): number {
  return resolveMs(env, 'ASTRA_AGENT_TOOL_BUDGET_MS', DEFAULT_TOOL_BUDGET_MS);
}

/** The watchdog grace past the budget, from the environment. */
export function resolveToolBudgetGraceMs(env: NodeJS.ProcessEnv): number {
  return resolveMs(env, 'ASTRA_AGENT_TOOL_BUDGET_GRACE_MS', DEFAULT_TOOL_BUDGET_GRACE_MS);
}

/** The deadline view the tool gate consults before every execute. One
 *  TurnBudget per live turn reaches the tools as a live view handed over
 *  at construction (the tools are built once and shared by every session,
 *  so the view is the one seam that stays per-turn); the gate's whole
 *  question is whether the budget stands. */
export interface BudgetView {
  /** True once the turn's wall clock has run past the budget: the gate
   *  answers skipped and the model is told no. */
  spent(): boolean;
}

/** One turn's budget, armed at prompt receipt. */
export class TurnBudget implements BudgetView {
  private readonly startedAt: number;
  private readonly budgetMs: number;

  constructor(budgetMs: number) {
    this.startedAt = Date.now();
    this.budgetMs = budgetMs;
  }

  spent(): boolean {
    return Date.now() - this.startedAt >= this.budgetMs;
  }

  /** Milliseconds until the deadline; 0 once spent, so an announcement
   *  timer armed on it fires at the next tick. */
  remainingMs(): number {
    return Math.max(0, this.budgetMs - (Date.now() - this.startedAt));
  }
}
