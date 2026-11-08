// SPDX-License-Identifier: GPL-3.0-only
/**
 * budget.test.ts: the tool budget's own unit seams, the module bundled
 *  with esbuild like the other sidecar tests. Pins the constants (the
 *  reference's 720-second chat budget, the one-minute watchdog grace), the
 *  env overrides the harness and tests arm, and the deadline view's
 *  spent transition (the budget zero case is spent from the first
 *  instant, which is how the evals arm a bound that bites at once).
 */
import type { BudgetView } from './budget.ts';
const { test, before, after }: typeof import('node:test') = require('node:test');
const assert: typeof import('node:assert/strict') = require('node:assert/strict');
const esbuild: typeof import('esbuild') = require('esbuild');
const os: typeof import('node:os') = require('node:os');
const path: typeof import('node:path') = require('node:path');
const fs: typeof import('node:fs') = require('node:fs');

interface BudgetModule {
  BUDGET_SPENT_STATE: string;
  DEFAULT_TOOL_BUDGET_MS: number;
  DEFAULT_TOOL_BUDGET_GRACE_MS: number;
  resolveToolBudgetMs: (env: NodeJS.ProcessEnv) => number;
  resolveToolBudgetGraceMs: (env: NodeJS.ProcessEnv) => number;
  TurnBudget: new (budgetMs: number) => { spent(): boolean; remainingMs(): number };
}

let budget: BudgetModule;
let bundlePath: string;

before(async () => {
  bundlePath = path.join(os.tmpdir(), `astra-budget-test-${process.pid}.cjs`);
  await esbuild.build({
    entryPoints: [path.resolve(__dirname, 'budget.ts')],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    outfile: bundlePath,
    logLevel: 'silent',
  });
  budget = require(bundlePath) as BudgetModule;
});

after(() => {
  try {
    fs.rmSync(bundlePath, { force: true });
  } catch (e) {
    process.stderr.write(`budget.test: failed to remove temp bundle: ${String(e)}\n`);
  }
});

test('the constants pin the reference budget and a one-minute grace', () => {
  assert.equal(budget.DEFAULT_TOOL_BUDGET_MS, 720_000, 'the web chat budget, 720 seconds');
  assert.equal(budget.DEFAULT_TOOL_BUDGET_GRACE_MS, 60_000, 'a minute to finish in text');
  assert.equal(budget.BUDGET_SPENT_STATE, 'budget-spent');
});

test('the env overrides arm short budgets and ignore malformed values', () => {
  assert.equal(budget.resolveToolBudgetMs({}), 720_000, 'no override means the default');
  assert.equal(budget.resolveToolBudgetMs({ ASTRA_AGENT_TOOL_BUDGET_MS: '40' }), 40);
  assert.equal(budget.resolveToolBudgetMs({ ASTRA_AGENT_TOOL_BUDGET_MS: '0' }), 0, 'zero arms a bound that bites at once');
  assert.equal(budget.resolveToolBudgetMs({ ASTRA_AGENT_TOOL_BUDGET_MS: 'soon' }), 720_000, 'malformed stands down to the default');
  assert.equal(budget.resolveToolBudgetMs({ ASTRA_AGENT_TOOL_BUDGET_MS: '-5' }), 720_000, 'negative stands down too');
  assert.equal(budget.resolveToolBudgetGraceMs({}), 60_000);
  assert.equal(budget.resolveToolBudgetGraceMs({ ASTRA_AGENT_TOOL_BUDGET_GRACE_MS: '30' }), 30);
  assert.equal(budget.resolveToolBudgetGraceMs({ ASTRA_AGENT_TOOL_BUDGET_GRACE_MS: '' }), 60_000, 'empty means unset');
});

test('a zero budget is spent from the first instant; a long one stands', () => {
  const spent = new budget.TurnBudget(0);
  const view: BudgetView = spent;
  assert.equal(view.spent(), true, 'elapsed is never below zero');
  assert.equal(spent.remainingMs(), 0);
  const standing = new budget.TurnBudget(600_000);
  assert.equal(standing.spent(), false, 'a fresh long budget stands');
  assert.ok(standing.remainingMs() > 0 && standing.remainingMs() <= 600_000);
});
