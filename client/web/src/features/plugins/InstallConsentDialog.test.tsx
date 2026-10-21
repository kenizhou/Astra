// SPDX-License-Identifier: GPL-3.0-only
// Tests for the install consent dialog: every permission starts unchecked,
// Install carries exactly the checked set, Cancel installs nothing, and a
// refused install shows the reason and keeps the dialog standing.

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { InstallOffer } from '@/services/plugins';

const install = vi.hoisted(() => vi.fn());

vi.mock('@/services/plugins', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  pluginInstall: install,
}));

import { InstallConsentDialog } from './InstallConsentDialog';

const offer: InstallOffer = {
  id: 'dev.astra.crypto',
  name: 'Crypto',
  version: '0.1.0',
  publisher_fingerprint: 'abcdef0123456789',
  permissions: ['read_item', 'send'],
  privacy_policy_url: 'https://example.test/privacy',
};

// NOTE: the beforeEach must not return the mock (an unbraced arrow would):
// vitest registers a beforeEach's return value as a cleanup hook and would
// call the mock at teardown, whose rejection then reads as unhandled.
beforeEach(() => {
  install.mockReset().mockResolvedValue(undefined);
});

afterEach(cleanup);

describe('the install consent dialog', () => {
  it('names the plugin, the publisher, the policy, and the asks', () => {
    render(<InstallConsentDialog offer={offer} path="/tmp/crypto.astra-plugin" onDone={() => {}} />);
    expect(screen.getByRole('dialog').textContent).toContain('Crypto');
    expect(screen.getByRole('dialog').textContent).toContain('publisher abcdef0123456789');
    expect(screen.getByRole('dialog').textContent).toContain('https://example.test/privacy');
    expect(screen.getByText('Read the message you are looking at')).toBeTruthy();
    expect(screen.getByText('Gate or send your outgoing mail')).toBeTruthy();
  });

  it('starts every permission unchecked and installs exactly the checked set', async () => {
    const done = vi.fn();
    render(<InstallConsentDialog offer={offer} path="/tmp/crypto.astra-plugin" onDone={done} />);
    const checks = screen.getAllByRole('checkbox');
    expect(checks.every((box) => !(box as HTMLInputElement).checked)).toBe(true);

    checks[1]!.click(); // send only
    fireEvent.click(screen.getByRole('button', { name: 'Install' }));
    await waitFor(() => {
      expect(install).toHaveBeenCalledWith('/tmp/crypto.astra-plugin', ['send']);
    });
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith(true);
    });
  });

  it('cancels into nothing installed', () => {
    const done = vi.fn();
    render(<InstallConsentDialog offer={offer} path="/tmp/crypto.astra-plugin" onDone={done} />);
    screen.getByRole('button', { name: 'Cancel' }).click();
    expect(install).not.toHaveBeenCalled();
    expect(done).toHaveBeenCalledWith(false);
  });

  it('shows the refusal and stands when the install fails', async () => {
    install.mockRejectedValue('the bundle is unsigned');
    const done = vi.fn();
    render(<InstallConsentDialog offer={offer} path="/tmp/crypto.astra-plugin" onDone={done} />);
    fireEvent.click(screen.getByRole('button', { name: 'Install' }));
    await waitFor(() => {
      expect(screen.getByRole('alert').textContent).toBe('the bundle is unsigned');
    });
    expect(done).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toBeTruthy();
  });
});
