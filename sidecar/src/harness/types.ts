// SPDX-License-Identifier: GPL-3.0-only
/**
 * Harness abstraction: one harness adapts one agent backend (mock, pi) to
 * the sidecar's session model. HarnessEvent is the sidecar-internal event
 * union; the top-level done and error envelopes are the stdio loop's
 * business, never a harness event.
 */
import type { LlmCallData, ToolResultStatus } from '../protocol';

export type { ToolResultStatus };

/** Usage of one LLM API call as the harness observed it. The runner
 *  translates it into the wire llm_call ledger row, adding the session,
 *  prompt, correlation, tier, and caller context from the command side. */
export type LlmCallUsage = Omit<
  LlmCallData,
  'session_id' | 'prompt_id' | 'correlation_id' | 'tier' | 'caller'
>;

export type HarnessEvent =
  | { kind: 'text_delta'; text: string }
  | { kind: 'tool_call'; name: string; args: unknown; tool_call_id: string }
  | { kind: 'state'; state: string }
  | {
      kind: 'tool_result';
      tool_call_id: string;
      name: string;
      status: ToolResultStatus;
      output?: string;
    }
  | { kind: 'llm_call'; usage: LlmCallUsage };

/** Runtime call context for one session.prompt() invocation. The
 *  correlation id is the traceability key: it rides every tool spawn of the
 *  turn and the turn's llm_call ledger rows. */
export interface PromptCallContext {
  session_id: string;
  correlation_id: string;
}

export interface HarnessSession {
  prompt(text: string, ctx: PromptCallContext): Promise<void>;
  abort(): void;
  subscribe(cb: (e: HarnessEvent) => void): void;
  dispose(): Promise<void>;
}

export interface AgentHarness {
  readonly id: string;
  createSession(opts: { harnessOptions?: unknown }): Promise<HarnessSession>;
}
