// SPDX-License-Identifier: GPL-3.0-only
// The plugin frame's bridge: the narrow, typed channel the sandboxed page
// talks to the host through. The page posts `{ type: 'frame_call', id,
// method, args }`; the bridge answers `{ type: 'frame_result' | 'frame_error',
// id, ... }` over the same port. Only messages from the frame's own window
// are served, and the host's plugin_call enforces the declared-method gate
// before anything reaches the worker.

import { pluginCall } from '@/services/plugins';

/** One call the frame's page posts. */
export interface FrameCall {
  /** The envelope's own tag. */
  type: 'frame_call';
  /** The call's id, echoed by the answer. */
  id: string;
  /** The plugin's declared custom method (`plugin.<id>.<name>`). */
  method: string;
  /** The call's arguments. */
  args: unknown;
}

/** One answer the bridge posts back. */
export type FrameAnswer =
  | { type: 'frame_result'; id: string; result: unknown }
  | { type: 'frame_error'; id: string; message: string };

/** Narrows an arriving message's data to a frame call. */
export function asFrameCall(data: unknown): FrameCall | null {
  if (typeof data !== 'object' || data === null) {
    return null;
  }
  const fields = data as Record<string, unknown>;
  if (
    fields.type !== 'frame_call' ||
    typeof fields.id !== 'string' ||
    typeof fields.method !== 'string'
  ) {
    return null;
  }
  return { type: 'frame_call', id: fields.id, method: fields.method, args: fields.args };
}

/**
 * Bridges one frame: serves its calls against its plugin's worker and posts
 * the answers back. Returns the teardown (the frame unmounts it).
 */
export function bridgeFrame(
  frame: HTMLIFrameElement,
  pluginId: string,
): () => void {
  const onMessage = (event: MessageEvent) => {
    if (event.source !== frame.contentWindow) {
      return;
    }
    const call = asFrameCall(event.data);
    if (call === null) {
      return;
    }
    void pluginCall(pluginId, call.method, call.args).then(
      (result) => {
        const answer: FrameAnswer = { type: 'frame_result', id: call.id, result };
        frame.contentWindow?.postMessage(answer, '*');
      },
      (error: unknown) => {
        const answer: FrameAnswer = {
          type: 'frame_error',
          id: call.id,
          message: error instanceof Error ? error.message : String(error),
        };
        frame.contentWindow?.postMessage(answer, '*');
      },
    );
  };
  window.addEventListener('message', onMessage);
  return () => window.removeEventListener('message', onMessage);
}
