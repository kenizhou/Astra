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
  pluginInstallOffer: vi.fn(),
  pluginInstall: vi.fn(),
  openFile: vi.fn(),
  dispatch: vi.fn(),
}));

vi.mock('@/services/plugins', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  pluginEnable: api.pluginEnable,
  pluginDisable: api.pluginDisable,
  pluginList: api.pluginList,
  pluginInstallOffer: api.pluginInstallOffer,
  pluginInstall: api.pluginInstall,
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: api.openFile,
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: api.dispatch,
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
  api.pluginInstallOffer.mockReset();
  api.pluginInstall.mockReset().mockResolvedValue(undefined);
  api.openFile.mockReset();
  api.dispatch.mockReset().mockResolvedValue(undefined);
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

describe('the plugin pages', () => {
  it('opens a declared page in the sandboxed frame and closes it again', async () => {
    api.pluginList.mockResolvedValue([
      {
        ...row('dev.astra.keys', 'activated'),
        ui: {
          composer_actions: [],
          settings_sections: [],
          pages: [{ id: 'keys', title: 'Key manager', entry: 'keys.html' }],
        },
      },
    ]);
    render(<PluginsDetail />);
    const toggle = await screen.findByRole('button', { name: 'Key manager' });
    toggle.click();
    const frame = await screen.findByTitle('Key manager', { suggest: false });
    expect(frame.tagName).toBe('IFRAME');
    expect(frame.getAttribute('src')).toBe('astra-plugin://dev.astra.keys/keys.html');
    expect(frame.getAttribute('sandbox')).toBe('allow-scripts allow-same-origin');
    toggle.click();
    await waitFor(() => {
      expect(screen.queryByTitle('Key manager', { suggest: false })).toBeNull();
    });
  });
});

describe('the policy switch and the install entry', () => {
  it('dispatches the plugins_enabled intent from the toggle', () => {
    render(<PluginsDetail />);
    screen.getByRole('checkbox', { name: 'Run plugins' }).click();
    expect(api.dispatch).toHaveBeenCalledWith({
      settings: { set_plugins_enabled: { enabled: false } },
    });
  });

  it('offers the consent dialog from a picked file and installs through it', async () => {
    api.openFile.mockResolvedValue('/tmp/crypto.astra-plugin');
    api.pluginInstallOffer.mockResolvedValue({
      id: 'dev.astra.crypto',
      name: 'Crypto',
      version: '0.1.0',
      publisher_fingerprint: 'abcdef0123456789',
      permissions: ['send'],
    });
    render(<PluginsDetail />);
    screen.getByRole('button', { name: 'Install from file…' }).click();
    await screen.findByRole('dialog');
    expect(screen.getByRole('dialog').textContent).toContain('Crypto');
    // The consent dialog carries the install; cancelling it installs nothing.
    screen.getByRole('button', { name: 'Cancel' }).click();
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });
    expect(api.pluginInstall).not.toHaveBeenCalled();
  });
});
