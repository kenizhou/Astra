// SPDX-License-Identifier: GPL-3.0-only
// Tests for the plugin settings section: every field kind renders in its own
// control, and every change forwards the key and value to the worker's
// set_setting custom method.

import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { PluginRow } from '@/services/plugins';

const call = vi.hoisted(() => vi.fn());

vi.mock('@/services/plugins', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  pluginCall: call,
}));

import { PluginSettingsSection, PluginSettingsSections } from './PluginSettingsSection';
import { usePluginStore } from '@/stores/pluginStore';

const plugin: PluginRow = {
  id: 'dev.astra.crypto',
  name: 'Crypto',
  version: '0.1.0',
  lifecycle: 'activated',
  grants: [],
  program: 'crypto.exe',
  ui: {
    composer_actions: [],
    settings_sections: [
      {
        id: 'crypto',
        title: 'Crypto',
        fields: [
          { key: 'sign', kind: 'toggle', label: 'Sign by default', options: [] },
          { key: 'method', kind: 'select', label: 'Method', options: ['pgp', 'smime'] },
          { key: 'token', kind: 'password', label: 'Token PIN', options: [] },
          { key: 'note', kind: 'text', label: 'Note', options: [] },
          { key: 'import', kind: 'action', label: 'Import keys', options: [] },
        ],
      },
    ],
    pages: [],
  },
};

beforeEach(() => {
  call.mockReset();
  call.mockResolvedValue(null);
});

afterEach(cleanup);

describe('the plugin settings section', () => {
  it('renders every field in its kind and forwards each change', () => {
    const section = plugin.ui!.settings_sections[0]!;
    render(<PluginSettingsSection plugin={plugin} section={section} />);
    const toggle = screen.getByRole('checkbox', { name: 'Sign by default' });
    toggle.click();
    expect(call).toHaveBeenCalledWith(
      'dev.astra.crypto',
      'plugin.dev.astra.crypto.set_setting',
      { key: 'sign', value: true },
    );

    const select = screen.getByRole('combobox', { name: 'Method' });
    expect(select.textContent).toContain('pgp');
    expect(select.textContent).toContain('smime');
    expect(screen.getByLabelText('Token PIN')).toHaveProperty('type', 'password');
    expect(screen.getByLabelText('Note')).toHaveProperty('type', 'text');

    screen.getByRole('button', { name: 'Import keys' }).click();
    expect(call).toHaveBeenCalledWith(
      'dev.astra.crypto',
      'plugin.dev.astra.crypto.set_setting',
      { key: 'import', value: null },
    );
  });

  it('renders the sections of activated plugins only', () => {
    usePluginStore.setState({
      rows: [
        plugin,
        { ...plugin, id: 'dev.astra.dormant', lifecycle: 'discovered' },
      ],
    });
    render(<PluginSettingsSections />);
    expect(screen.getAllByRole('heading', { name: 'Crypto' })).toHaveLength(1);
  });
});
