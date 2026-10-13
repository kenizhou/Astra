// SPDX-License-Identifier: GPL-3.0-only
// The oauth half of the IPC wire, pinned against the literal JSON
// astra-desktop's oauth commands assert: the provider words, the
// outcome's externally tagged shape (a bare word for the unit success),
// and the event the completion arrives on.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen }));

import { oauthBegin, oauthCancel, oauthOffer, onOauthSignInResult } from './oauth';

beforeEach(() => {
  invoke.mockReset();
  listen.mockReset();
});

describe('the oauth wire', () => {
  it('pulls the offer', async () => {
    invoke.mockResolvedValue({ providers: ['microsoft'] });
    await expect(oauthOffer()).resolves.toEqual({ providers: ['microsoft'] });
    expect(invoke).toHaveBeenCalledWith('oauth_offer');
  });

  it('begins with the provider and the login hint in camel case', async () => {
    invoke.mockResolvedValue({ provider: 'google' });
    await oauthBegin('google', 'work@example.test');
    expect(invoke).toHaveBeenCalledWith('oauth_begin', {
      provider: 'google',
      loginHint: 'work@example.test',
    });
  });

  it('begins without a login hint when no address is known', async () => {
    invoke.mockResolvedValue({ provider: 'microsoft' });
    await oauthBegin('microsoft');
    expect(invoke).toHaveBeenCalledWith('oauth_begin', {
      provider: 'microsoft',
      loginHint: undefined,
    });
  });

  it('cancels the standing sign-in', async () => {
    invoke.mockResolvedValue(undefined);
    await oauthCancel();
    expect(invoke).toHaveBeenCalledWith('oauth_cancel');
  });

  it('subscribes to the sign-in result event and hands the payload over', async () => {
    const unlisten = vi.fn();
    listen.mockResolvedValue(unlisten);
    const seen: unknown[] = [];
    const teardown = await onOauthSignInResult((result) => {
      seen.push(result);
    });

    expect(listen).toHaveBeenCalledTimes(1);
    const [event, handler] = listen.mock.calls[0] as [
      string,
      (event: { payload: unknown }) => void,
    ];
    expect(event).toBe('oauth_sign_in_result');
    handler({
      payload: { provider: 'microsoft', outcome: { refused: { refusal: 'dead', detail: 'x' } } },
    });
    expect(seen).toEqual([
      { provider: 'microsoft', outcome: { refused: { refusal: 'dead', detail: 'x' } } },
    ]);
    teardown();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
