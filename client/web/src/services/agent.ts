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

/** What one accepted prompt answers: the id its events carry and the chat
 *  its turn records into. A prompt sent without a chat id mints one. */
export interface AgentPromptReceipt {
  prompt_id: string;
  chat_id: string;
}

/** One chat thread as the list holds it (the store's row). */
export interface ChatSummary {
  id: string;
  title: string;
  created_at: number;
  last_touched_at: number;
}

/** One rendered part of a message row: a text run, a completed tool call,
 *  or a note. The recorder's vocabulary; unknown kinds render as nothing. */
export interface ChatPart {
  kind: string;
  text?: string;
  name?: string;
  status?: string;
  args?: unknown;
  output?: string;
}

/** One message row of a loaded thread (the store's row). */
export interface ChatMessage {
  id: number;
  chat_id: string;
  role: string;
  parts: ChatPart[];
  metadata?: Record<string, unknown> | null;
  created_at: number;
}

/** One loaded thread: the chat's row beside its messages, oldest first. */
export interface ChatLoad {
  chat: ChatSummary;
  messages: ChatMessage[];
}

/** Sends one prompt; resolves to the receipt its events and its chat carry. */
export function agentPrompt(prompt: string, correlation?: string, chatId?: string): Promise<AgentPromptReceipt> {
  return invoke('agent_prompt', { prompt, correlation, chatId });
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

/** Lists the chat threads, the most recently touched first. */
export function chatList(): Promise<ChatSummary[]> {
  return invoke('chat_list');
}

/** Loads one thread's messages, oldest first: the rows the visible log
 *  paints from, the one source of truth. */
export function chatLoad(chatId: string): Promise<ChatLoad> {
  return invoke('chat_load', { chatId });
}

/** Renames one thread. */
export function chatRename(chatId: string, title: string): Promise<boolean> {
  return invoke('chat_rename', { chatId, title });
}

/** Deletes one thread with its messages. */
export function chatDelete(chatId: string): Promise<boolean> {
  return invoke('chat_delete', { chatId });
}
