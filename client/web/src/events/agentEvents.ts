// SPDX-License-Identifier: GPL-3.0-only
// The agent event wiring: the host emits one event per streamed turn
// event, asked approval, settled approval, and lifecycle transition, and
// this module folds each into the agent store. Mounted once at the app
// root, so approvals land in the store whether or not the panel is up;
// the registry is the same dispose-safe shape the surface events use
// (its `listen` import is the one that talks to Tauri).

import { isTauri, createListenerRegistry } from '@/events/surfaceEvents';
import { useAgentStore } from '@/features/agent/agentStore';
import type { AgentStreamEvent, PendingApproval, PermissionResolved } from '@/services/agent';

/** The sidecar's ready handshake, as the host emits it. */
interface SidecarReady {
  models: string[];
  harnesses: string[];
}

/** The restart budget's exhaustion, as the host emits it. */
interface SidecarLost {
  attempts: number;
  message: string;
}

/**
 * Mounts the agent event listeners and returns their disposer.
 * Synchronous on purpose: the caller's effect can unmount before the
 * registrations resolve, and the registry handles that race itself.
 */
export function mountAgentEvents(): () => void {
  if (!isTauri) {
    return () => undefined;
  }
  const registry = createListenerRegistry();
  void registry.safeListen<AgentStreamEvent>('agent:event', (event) => {
    useAgentStore.getState().applyStreamEvent(event.payload);
  }, 'agent:event');
  void registry.safeListen<PendingApproval>('agent:permission-requested', (event) => {
    useAgentStore.getState().addApproval(event.payload);
  }, 'agent:permission-requested');
  void registry.safeListen<PermissionResolved>('agent:permission-resolved', (event) => {
    useAgentStore.getState().resolveApproval(event.payload.id);
  }, 'agent:permission-resolved');
  void registry.safeListen<SidecarReady>('agent:sidecar-ready', () => {
    useAgentStore.getState().setStatus('ready');
  }, 'agent:sidecar-ready');
  void registry.safeListen<SidecarLost>('agent:sidecar-lost', () => {
    useAgentStore.getState().setStatus('lost');
  }, 'agent:sidecar-lost');
  return () => registry.disposeAll();
}
