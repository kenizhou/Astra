// SPDX-License-Identifier: GPL-3.0-only
// The mcp wire, pinned against the desktop command's serde: the shared
// fixture parses as the panel's view with every block (the state word,
// the bind diagnostic, the endpoint, the gate, the relay argv), an off
// view crosses with no reason key at all, and the three commands ask by
// name with the payload keys the Rust side reads.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import type { McpStatusView } from './mcp';
import { mcpSetAllowed, mcpSetEnabled, mcpStatus } from './mcp';
import fixtureJson from './__fixtures__/mcp_status.json';

// The shared fixture: the failed state carrying every block, so each
// key's shape is pinned in one place both sides of the wire read.
describe('the mcp status wire', () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockResolvedValue({});
  });

  it('carries the state word, the diagnostic, the endpoint, the gate, and the relay argv', () => {
    const view = fixtureJson as McpStatusView;

    expect(view.status).toBe('failed');
    expect(view.reason).toBe('the endpoint name is held by another listener');
    expect(view.endpoint).toBe('\\\\.\\pipe\\com.kenizhou.astra.mcp');
    expect(view.allowed).toEqual(['work']);
    expect(view.relay_command).toEqual([
      'C:\\Apps\\Astra Mail\\astra-mcp-relay.exe',
      '--endpoint',
      '\\\\.\\pipe\\com.kenizhou.astra.mcp',
    ]);
  });

  it('crosses an off view with no reason key at all', () => {
    const view = JSON.parse(
      '{"status":"off","endpoint":"x","allowed":[],"relay_command":[]}',
    ) as McpStatusView;

    expect(view.reason).toBeUndefined();
  });

  it('pulls the status by its command name', async () => {
    await mcpStatus();

    expect(invoke).toHaveBeenCalledWith('mcp_status');
  });

  it('sets the enable flag and the allowlist by their command names', async () => {
    await mcpSetEnabled(true);
    await mcpSetAllowed(['work', 'home']);

    expect(invoke).toHaveBeenNthCalledWith(1, 'mcp_set_enabled', { on: true });
    expect(invoke).toHaveBeenNthCalledWith(2, 'mcp_set_allowed', {
      accounts: ['work', 'home'],
    });
  });
});
