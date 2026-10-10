// SPDX-License-Identifier: GPL-3.0-only
// The agent IPC layer: the command wrappers and the event payload types the
// panel and its store consume. The wire shapes mirror the desktop host's
// serde exactly: stream events carry a `kind` tag with snake_case fields,
// and every event of a turn echoes the turn's correlation id.

import { invoke } from '@tauri-apps/api/core';

/** One streamed turn event's payload, as the host emits it. */
export type StreamPayload =
  | { kind: 'text_delta'; text: string }
  | { kind: 'tool_call'; name: string; args: unknown; tool_call_id: string }
  | { kind: 'state'; state: string }
  | {
      kind: 'tool_result';
      tool_call_id: string;
      name: string;
      status: string;
      output?: string;
    }
  | { kind: 'done'; prompt_id: string }
  | { kind: 'error'; message: string; prompt_id?: string };

/** One streamed turn event: the session, the correlation id, and the payload. */
export interface AgentStreamEvent {
  session_id: string;
  correlation?: string;
  event: StreamPayload;
}

/** One approval awaiting the user's decision. */
export interface PendingApproval {
  id: string;
  tool_call_id: string;
  tool: string;
  level: string;
  args: Record<string, unknown>;
  preview?: unknown;
  correlation?: string;
}

/** A pending approval settled: by the user's answer or by a fail close. */
export interface PermissionResolved {
  id: string;
  approved: boolean;
  always_allow: boolean;
}

/** The relay status pull: the lifecycle word and the configured chat model. */
export interface AgentStatus {
  state: string;
  model: string | null;
}

/** Sends one prompt; resolves to the prompt id its events carry. */
export function agentPrompt(prompt: string, correlation?: string): Promise<string> {
  return invoke('agent_prompt', { prompt, correlation });
}

/** Cancels the prompt: the turn settles with a done event. */
export function agentAbort(promptId: string): Promise<void> {
  return invoke('agent_abort', { promptId });
}

/** Pulls the agent status. */
export function agentStatus(): Promise<AgentStatus> {
  return invoke('agent_status');
}

/** Answers one pending approval. */
export function permissionResponse(
  id: string,
  approved: boolean,
  alwaysAllow: boolean,
): Promise<void> {
  return invoke('permission_response', { id, approved, alwaysAllow });
}
