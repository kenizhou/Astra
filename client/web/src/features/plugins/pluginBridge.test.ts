// SPDX-License-Identifier: GPL-3.0-only
// Tests for the frame bridge: only the frame's own window is served, a
// well-formed call reaches the worker and its answer posts back, a refused
// call answers an error, and foreign or malformed messages are ignored.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const call = vi.hoisted(() => vi.fn());

vi.mock('@/services/plugins', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  pluginCall: call,
}));

import { asFrameCall, bridgeFrame } from './pluginBridge';

/** A detached iframe with a spyable contentWindow. */
function fakeFrame(): HTMLIFrameElement {
  const frame = document.createElement('iframe');
  document.body.appendChild(frame);
  return frame;
}

beforeEach(() => {
  call.mockReset();
  call.mockResolvedValue({ ok: true });
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('the frame bridge', () => {
  it('narrows only well-formed frame calls', () => {
    expect(asFrameCall({ type: 'frame_call', id: 'c1', method: 'plugin.x.y', args: {} }))
      .toEqual({ type: 'frame_call', id: 'c1', method: 'plugin.x.y', args: {} });
    expect(asFrameCall(null)).toBeNull();
    expect(asFrameCall({ type: 'frame_call', id: 'c1' })).toBeNull();
    expect(asFrameCall({ type: 'other', id: 'c1', method: 'm' })).toBeNull();
    expect(asFrameCall('frame_call')).toBeNull();
  });

  it('answers calls from the frame itself and ignores every other source', async () => {
    const frame = fakeFrame();
    const posted: unknown[] = [];
    vi.spyOn(frame.contentWindow!, 'postMessage').mockImplementation((message: unknown) => {
      posted.push(message);
    });
    const unbridge = bridgeFrame(frame, 'dev.astra.keys');

    // A message from another window is not served.
    window.dispatchEvent(
      new MessageEvent('message', {
        data: { type: 'frame_call', id: 'c0', method: 'plugin.dev.astra.keys.ping', args: {} },
        source: window,
      }),
    );
    expect(call).not.toHaveBeenCalled();

    window.dispatchEvent(
      new MessageEvent('message', {
        data: { type: 'frame_call', id: 'c1', method: 'plugin.dev.astra.keys.ping', args: { n: 1 } },
        source: frame.contentWindow,
      }),
    );
    await vi.waitFor(() => {
      expect(posted).toEqual([{ type: 'frame_result', id: 'c1', result: { ok: true } }]);
    });
    expect(call).toHaveBeenCalledWith('dev.astra.keys', 'plugin.dev.astra.keys.ping', { n: 1 });
    unbridge();
  });

  it('answers a refused call with the error', async () => {
    call.mockRejectedValue(new Error('not activated'));
    const frame = fakeFrame();
    const posted: unknown[] = [];
    vi.spyOn(frame.contentWindow!, 'postMessage').mockImplementation((message: unknown) => {
      posted.push(message);
    });
    const unbridge = bridgeFrame(frame, 'dev.astra.keys');
    window.dispatchEvent(
      new MessageEvent('message', {
        data: { type: 'frame_call', id: 'c2', method: 'plugin.dev.astra.keys.ping', args: {} },
        source: frame.contentWindow,
      }),
    );
    await vi.waitFor(() => {
      expect(posted).toEqual([{ type: 'frame_error', id: 'c2', message: 'not activated' }]);
    });
    unbridge();
  });

  it('ignores malformed messages from the frame itself', () => {
    const frame = fakeFrame();
    const unbridge = bridgeFrame(frame, 'dev.astra.keys');
    window.dispatchEvent(
      new MessageEvent('message', {
        data: { type: 'frame_call', method: 'plugin.dev.astra.keys.ping' },
        source: frame.contentWindow,
      }),
    );
    expect(call).not.toHaveBeenCalled();
    unbridge();
  });
});
