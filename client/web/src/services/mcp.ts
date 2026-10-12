// SPDX-License-Identifier: GPL-3.0-only
// The MCP half of the IPC wire: the advanced panel's status view (the
// listener's state word, the bind diagnostic, the endpoint, the gate's
// allowed account ids, and the relay argv a client config pastes) and
// the three host commands over it: the status pull and the enable and
// allowlist sets. Host-side state, never intents: the host owns the
// listener, and nothing persists across a restart. The literals are
// pinned in mcp.test.ts against the desktop command's serde.

import { invoke } from '@tauri-apps/api/core';

/** The listener's state as the panel reads it; crosses as a snake_case word. */
export type McpStateWord = 'off' | 'listening' | 'failed';

/** The advanced panel's pull: the whole panel's truth in one view. */
export interface McpStatusView {
  /** The listener's state word. */
  status: McpStateWord;
  /** Why the bind failed, when it did; absent otherwise. */
  reason?: string;
  /** The endpoint as the string the relay argv carries. */
  endpoint: string;
  /** The account ids the gate allows, in stable order. */
  allowed: string[];
  /** The relay command: the binary beside the app, the endpoint as its argument. */
  relay_command: string[];
}

/** Pulls the MCP panel's status view. */
export function mcpStatus(): Promise<McpStatusView> {
  return invoke('mcp_status');
}

/** Turns the MCP listener on or off. */
export function mcpSetEnabled(on: boolean): Promise<void> {
  return invoke('mcp_set_enabled', { on });
}

/** Replaces the MCP account allowlist. */
export function mcpSetAllowed(accounts: string[]): Promise<void> {
  return invoke('mcp_set_allowed', { accounts });
}
