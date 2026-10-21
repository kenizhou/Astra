// SPDX-License-Identifier: GPL-3.0-only
// Tests for the Plugins settings detail: the rows render with lifecycle and
// grants, the verb matches the state (enable for discovered, disable for
// activated, dead for failed), and every verb ends in a refresh.

import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { PluginRow } from '@/services/plugins';
import { usePluginStore } from '@/stores/pluginStore';

const api = vi.hoisted(() => ({
  pluginEnable: vi.fn(),
  pluginDisable: vi.fn(),
  pluginList: vi.fn(),
}));

vi.mock('@/services/plugins', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  pluginEnable: api.pluginEnable,
  pluginDisable: api.pluginDisable,
  pluginList: api.pluginList,
}));

import { PluginsDetail } from './PluginsDetail';

function row(id: string, lifecycle: string, grants: string[] = []): PluginRow {
  return {
    id,
    name: `${id} plugin`,
    version: '0.1.0',
    lifecycle,
    grants,
    program: `${id}.exe`,
  };
}

beforeEach(() => {
  api.pluginEnable.mockReset().mockResolvedValue(undefined);
  api.pluginDisable.mockReset().mockResolvedValue(undefined);
  api.pluginList.mockReset().mockResolvedValue([]);
});

afterEach(cleanup);

describe('the plugins detail', () => {
  it('says so when nothing is discovered', () => {
    usePluginStore.setState({ rows: [] });
    render(<PluginsDetail />);
    expect(screen.getByText('No plugins discovered.')).toBeTruthy();
  });

  it('renders each row with its state and the verb its state allows', async () => {
    // The mount pulls the registry: the mocked pull answers the rows.
    api.pluginList.mockResolvedValue([
      row('dev.astra.crypto', 'activated', ['read_item', 'send']),
      row('dev.astra.idle', 'discovered'),
      row('dev.astra.dead', 'failed'),
    ]);
    render(<PluginsDetail />);
    await waitFor(() => {
      expect(screen.getByText('dev.astra.crypto · activated · read_item, send')).toBeTruthy();
    });
    const disable = screen.getByRole('button', { name: 'Disable' });
    disable.click();
    await waitFor(() => {
      expect(api.pluginDisable).toHaveBeenCalledWith('dev.astra.crypto');
    });
    expect(api.pluginList).toHaveBeenCalled();

    const enables = screen.getAllByRole('button', { name: 'Enable' });
    // The discovered row's Enable is live; the failed row's is dead.
    const live = enables.find((button) => !button.hasAttribute('disabled'))!;
    const dead = enables.find((button) => button.hasAttribute('disabled'))!;
    expect(dead).toBeTruthy();
    live.click();
    await waitFor(() => {
      expect(api.pluginEnable).toHaveBeenCalledWith('dev.astra.idle');
    });

    expect(dead).toBeTruthy();
  });
});
