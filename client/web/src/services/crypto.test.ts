// SPDX-License-Identifier: GPL-3.0-only
// The crypto receive wire, pinned against the desktop host's command surface:
// the passphrase submission invokes the command its Rust side registers with
// the account-and-key-and-passphrase payload the serde there reads, and the
// answer mirrors the prompt affordance the shared serde answers (pinned on the
// Rust side against these same literals). The passphrase crosses into host
// memory only: it is never logged or persisted.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import { submitReadingPassword } from './crypto';
import type { PasswordPromptState } from './crypto';

beforeEach(() => {
  invoke.mockReset();
});

describe('the password submission call', () => {
  it('invokes the command with the message reference and the passphrase', async () => {
    invoke.mockResolvedValue(null);
    await submitReadingPassword({ account: 'work', key: 'imap:v1:u42' }, 'hunter2');
    expect(invoke).toHaveBeenCalledWith('submit_reading_password', {
      account: 'work',
      key: 'imap:v1:u42',
      passphrase: 'hunter2',
    });
  });

  it('answers null once the passphrase decrypts', async () => {
    invoke.mockResolvedValue(null);
    const outcome = await submitReadingPassword({ account: 'work', key: 'm1' }, 'hunter2');
    expect(outcome).toBeNull();
  });

  it('answers the re-prompt signal on a wrong passphrase', async () => {
    invoke.mockResolvedValue('wrong_password');
    const outcome: PasswordPromptState | null = await submitReadingPassword(
      { account: 'work', key: 'm1' },
      'not-the-passphrase',
    );
    expect(outcome).toBe('wrong_password');
  });
});

describe('the prompt affordance wire', () => {
  it('crosses the needs-password prompt as the snake_case word', () => {
    const wire = JSON.parse('"needs_password"') as PasswordPromptState;
    expect(wire).toBe('needs_password');
  });

  it('crosses the wrong-password re-prompt as the snake_case word', () => {
    const wire = JSON.parse('"wrong_password"') as PasswordPromptState;
    expect(wire).toBe('wrong_password');
  });
});