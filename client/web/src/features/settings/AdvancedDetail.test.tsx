// SPDX-License-Identifier: GPL-3.0-only
// The Advanced detail: the AI assistant access panel. The toggle drives
// the host's enable seam and reflects the pulled truth, never local
// optimism; the allowlist picker names accounts from the settings
// snapshot's rows and sets the gate with the whole set; the reach
// sentence states what the gate allows per state; the failed state
// renders the bind diagnostic; the client-config snippet carries the
// relay command; and the restart rule says itself.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
  mcpStatus: vi.fn(),
  mcpSetEnabled: vi.fn(),
  mcpSetAllowed: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  mcpStatus: ipc.mcpStatus,
  mcpSetEnabled: ipc.mcpSetEnabled,
  mcpSetAllowed: ipc.mcpSetAllowed,
}));

import type { McpStatusView } from '@/services/ipc';
import {
  AdvancedDetail,
  REACH_EMPTY_SENTENCE,
  RESTART_SENTENCE,
  reachSentence,
} from './AdvancedDetail';
import { useSettingsStore } from './settingsStore';

/** A pulled view, every field overridable. */
function view(over: Partial<McpStatusView> = {}): McpStatusView {
  return {
    status: 'off',
    endpoint: '\\\\.\\pipe\\com.kenizhou.astra.mcp',
    allowed: [],
    relay_command: [
      'C:\\Apps\\Astra Mail\\astra-mcp-relay.exe',
      '--endpoint',
      '\\\\.\\pipe\\com.kenizhou.astra.mcp',
    ],
    ...over,
  };
}

/** The snapshot the picker reads: two account rows. */
function applyAccounts(): void {
  useSettingsStore.getState().applySnapshot({
    settings: {
      theme: 'follow_host',
      twenty_four_clock: false,
      zone: 'Etc/UTC',
      first_day_of_week: 'mo',
      horizon_days: 42,
      quote_below: true,
      view_mode: 'flat',
    },
    accounts: [
      {
        account: 'work',
        address: 'ada@example.test',
        kind: 'imap_smtp',
        legs: { calendar: false, contacts: false },
      },
      {
        account: 'home',
        address: 'me@home.test',
        kind: 'imap_smtp',
        legs: { calendar: false, contacts: false },
      },
    ],
    about: { version: '0.1.0' },
  });
}

beforeEach(() => {
  ipc.mcpStatus.mockReset();
  ipc.mcpStatus.mockResolvedValue(view());
  ipc.mcpSetEnabled.mockReset();
  ipc.mcpSetEnabled.mockResolvedValue(undefined);
  ipc.mcpSetAllowed.mockReset();
  ipc.mcpSetAllowed.mockResolvedValue(undefined);
  applyAccounts();
});

afterEach(cleanup);

describe('the Advanced detail', () => {
  it('pulls the host status on mount and reflects it, not local optimism', async () => {
    ipc.mcpStatus.mockResolvedValue(view({ status: 'listening' }));
    render(<AdvancedDetail />);

    await waitFor(() =>
      expect((screen.getByLabelText('AI assistant access') as HTMLInputElement).checked).toBe(
        true,
      ),
    );
    expect(screen.getByText('The server is running.')).toBeTruthy();
  });

  it('dispatches the host enable from the toggle and waits for the pull', async () => {
    render(<AdvancedDetail />);
    await waitFor(() => expect(ipc.mcpStatus).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByLabelText('AI assistant access'));

    expect(ipc.mcpSetEnabled).toHaveBeenCalledWith(true);
    // The pull answers off again, so the control keeps showing the
    // host's truth rather than the click that started the write.
    await waitFor(() => expect(ipc.mcpStatus).toHaveBeenCalledTimes(2));
    expect((screen.getByLabelText('AI assistant access') as HTMLInputElement).checked).toBe(false);
  });

  it('sets the gate with the whole set from the picker', async () => {
    ipc.mcpStatus.mockResolvedValue(view({ allowed: ['work'] }));
    render(<AdvancedDetail />);
    await waitFor(() =>
      expect((screen.getByLabelText('Expose ada@example.test') as HTMLInputElement).checked).toBe(
        true,
      ),
    );

    fireEvent.click(screen.getByLabelText('Expose me@home.test'));

    expect(ipc.mcpSetAllowed).toHaveBeenCalledWith(['work', 'home']);
  });

  it('removes an account from the gate when its row is unticked', async () => {
    ipc.mcpStatus.mockResolvedValue(view({ allowed: ['work', 'home'] }));
    render(<AdvancedDetail />);
    await waitFor(() =>
      expect((screen.getByLabelText('Expose ada@example.test') as HTMLInputElement).checked).toBe(
        true,
      ),
    );

    fireEvent.click(screen.getByLabelText('Expose ada@example.test'));

    expect(ipc.mcpSetAllowed).toHaveBeenCalledWith(['home']);
  });

  it('renders the reach sentence per gate state', async () => {
    const first = render(<AdvancedDetail />);
    await waitFor(() => expect(screen.getByText(REACH_EMPTY_SENTENCE)).toBeTruthy());
    first.unmount();

    ipc.mcpStatus.mockResolvedValue(view({ allowed: ['work', 'home'] }));
    render(<AdvancedDetail />);
    await waitFor(() => expect(screen.getByText(reachSentence(2))).toBeTruthy());
  });

  it('renders the bind diagnostic when the state is failed', async () => {
    ipc.mcpStatus.mockResolvedValue(
      view({ status: 'failed', reason: 'the endpoint name is taken' }),
    );
    render(<AdvancedDetail />);

    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('taken'));
  });

  it('carries the relay command as the client-config snippet', async () => {
    render(<AdvancedDetail />);

    await waitFor(() => {
      const snippet = screen.getByText(/astra-mcp-relay/);
      expect(snippet.textContent).toContain('astra-mcp-relay.exe');
      expect(snippet.textContent).toContain('--endpoint');
      expect(snippet.textContent).toContain('\\\\.\\pipe\\com.kenizhou.astra.mcp');
    });
  });

  it('states the restart rule in the panel itself', async () => {
    render(<AdvancedDetail />);

    await waitFor(() => expect(screen.getByText(RESTART_SENTENCE)).toBeTruthy());
  });

  it('says so when no account is configured at all', async () => {
    useSettingsStore.getState().applySnapshot({
      settings: useSettingsStore.getState().settings,
      accounts: [],
      about: { version: '0.1.0' },
    });
    render(<AdvancedDetail />);

    await waitFor(() =>
      expect(screen.getByText('No account is configured.')).toBeTruthy(),
    );
  });

  it('stands the picker still while a write is in flight', async () => {
    ipc.mcpStatus.mockResolvedValue(view({ status: 'listening' }));
    let release: (() => void) | undefined;
    ipc.mcpSetAllowed.mockReturnValue(
      new Promise<void>((resolve) => {
        release = resolve;
      }),
    );
    render(<AdvancedDetail />);

    const first = await screen.findByRole('checkbox', {
      name: 'Expose ada@example.test',
    });
    fireEvent.click(first);
    expect(ipc.mcpSetAllowed).toHaveBeenCalledWith(['work']);
    // The in-flight write holds the picker: a second click inside the
    // round trip cannot compute from the stale set and drop the first.
    expect(
      screen.getByRole('checkbox', { name: 'Expose ada@example.test' }),
    ).toHaveProperty('disabled', true);
    release?.();
    await waitFor(() =>
      expect(
        (screen.getByRole('checkbox', {
          name: 'Expose ada@example.test',
        }) as HTMLInputElement).disabled,
      ).toBe(false),
    );
  });
});

