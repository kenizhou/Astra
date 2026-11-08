// SPDX-License-Identifier: GPL-3.0-only
// The platform command wire: the default-mail registration verb and the
// offer latch's read and mark, each asking by its command name, with the
// registration outcome crossing as the snake_case family the host pins.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import { defaultMailAsked, markDefaultMailAsked, registerDefaultMail } from './platform';

beforeEach(() => {
  invoke.mockReset();
});

describe('the default-mail platform commands', () => {
  it('asks the registration by name', async () => {
    invoke.mockResolvedValue('registered');
    await registerDefaultMail();
    expect(invoke).toHaveBeenCalledWith('register_default_mail');
  });

  it('carries the outcome family as the host spells it', async () => {
    invoke.mockResolvedValueOnce('registered');
    invoke.mockResolvedValueOnce('already_default');
    invoke.mockResolvedValueOnce({ failed: { reason: 'the registry refused' } });
    expect(await registerDefaultMail()).toBe('registered');
    expect(await registerDefaultMail()).toBe('already_default');
    expect(await registerDefaultMail()).toEqual({ failed: { reason: 'the registry refused' } });
  });

  it('reads the offer latch by name', async () => {
    invoke.mockResolvedValue(false);
    await defaultMailAsked();
    expect(invoke).toHaveBeenCalledWith('default_mail_asked');
  });

  it('marks the offer latch by name', async () => {
    invoke.mockResolvedValue(undefined);
    await markDefaultMailAsked();
    expect(invoke).toHaveBeenCalledWith('default_mail_mark_asked');
  });
});
