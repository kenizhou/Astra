// SPDX-License-Identifier: GPL-3.0-only
// Tests for the plugin composer actions: a click calls the worker's custom
// method with the composition id, the answer's pressed state lands on the
// button, and a refused call leaves the button standing.

import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { PluginRow } from '@/services/plugins';
import { usePluginStore } from '@/stores/pluginStore';
import { useComposerStore } from '@/features/composer/composerStore';

const call = vi.hoisted(() => vi.fn());

vi.mock('@/services/plugins', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  pluginCall: call,
}));

import { PluginComposerActions } from './PluginComposerActions';

function row(actions: Array<{ id: string; label: string }>, lifecycle = 'activated'): PluginRow {
  return {
    id: 'dev.astra.crypto',
    name: 'Crypto',
    version: '0.1.0',
    lifecycle,
    grants: [],
    program: 'crypto.exe',
    ui: { composer_actions: actions, settings_sections: [], pages: [] },
  };
}

beforeEach(() => {
  call.mockReset();
  useComposerStore.setState({ composition: 'compose-1' });
});

afterEach(cleanup);

describe('the plugin composer actions', () => {
  it('renders nothing when no activated plugin declares an action', () => {
    usePluginStore.setState({ rows: [row([{ id: 'encrypt', label: 'Encrypt' }], 'discovered')] });
    const { container } = render(<PluginComposerActions />);
    expect(container.innerHTML).toBe('');
  });

  it('calls the worker on click and lands the pressed answer', async () => {
    call.mockResolvedValue({ pressed: true });
    usePluginStore.setState({
      rows: [row([{ id: 'encrypt', label: 'Encrypt' }, { id: 'sign', label: 'Sign' }])],
    });
    render(<PluginComposerActions />);
    screen.getByRole('button', { name: 'Encrypt' }).click();

    await waitFor(() => {
      expect(call).toHaveBeenCalledWith('dev.astra.crypto', 'plugin.dev.astra.crypto.encrypt', {
        composition: 'compose-1',
      });
    });
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Encrypt' }).getAttribute('aria-pressed')).toBe(
        'true',
      );
    });
    expect(screen.getByRole('button', { name: 'Sign' }).getAttribute('aria-pressed')).toBe(
      'false',
    );
  });

  it('leaves the button standing when the call is refused', async () => {
    call.mockRejectedValue(new Error('plugin dev.astra.crypto is not activated'));
    usePluginStore.setState({ rows: [row([{ id: 'encrypt', label: 'Encrypt' }])] });
    render(<PluginComposerActions />);
    screen.getByRole('button', { name: 'Encrypt' }).click();
    await waitFor(() => {
      expect(call).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Encrypt' }).hasAttribute('disabled')).toBe(
        false,
      );
    });
  });
});
