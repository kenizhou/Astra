// SPDX-License-Identifier: GPL-3.0-only
// The agent store: the panel's chat log with its streaming assistant turn,
// the approvals awaiting the user's decision, and the relay's lifecycle
// word. Stream events arrive from the host's `agent:event` emissions and
// are folded in here; the user's own gestures (send, abort, answer) ride
// straight out through the IPC wrappers.

import { create } from 'zustand';

import {
  agentAbort,
  agentPrompt,
  agentStatus,
  permissionResponse,
  type AgentStreamEvent,
  type PendingApproval,
} from '@/services/agent';

/** One chat log entry: who said it and what. */
export interface AgentMessage {
  id: string;
  role: 'user' | 'assistant' | 'note';
  text: string;
}

/** The store's message id counter, so entries key stably across renders. */
let nextMessageId = 0;

/** The next message id, module-scoped so every turn's entries stay unique. */
function messageId(): string {
  nextMessageId += 1;
  return `m-${nextMessageId}`;
}

interface AgentStoreState {
  /** The relay's lifecycle word: down, starting, ready, restarting, lost. */
  status: string;
  /** The live turn's prompt id, or null between turns. */
  turn: string | null;
  /** The chat log, oldest first. */
  messages: AgentMessage[];
  /** The approvals awaiting a decision, oldest first. */
  approvals: PendingApproval[];
  /** Applies the relay lifecycle word. */
  setStatus: (status: string) => void;
  /** Pulls the relay lifecycle word from the host, so the panel shows the
   *  state the events have not announced (restarting, most often). */
  refreshStatus: () => Promise<void>;
  /** Sends a prompt and records the user's side of the turn. */
  sendPrompt: (text: string) => Promise<void>;
  /** Aborts the live turn. */
  abort: () => Promise<void>;
  /** Answers one pending approval. */
  answer: (id: string, approved: boolean) => Promise<void>;
  /** Folds one streamed turn event into the log. */
  applyStreamEvent: (event: AgentStreamEvent) => void;
  /** Records a freshly asked approval. */
  addApproval: (approval: PendingApproval) => void;
  /** Takes a settled approval's card away. */
  resolveApproval: (id: string) => void;
  /** Empties the log and the approvals; the test seam. */
  reset: () => void;
}

export const useAgentStore = create<AgentStoreState>((set, get) => ({
  status: 'down',
  turn: null,
  messages: [],
  approvals: [],

  setStatus: (status) => set({ status }),

  refreshStatus: async () => {
    try {
      const status = await agentStatus();
      set({ status: status.state });
    } catch {
      // Outside the desktop host (jsdom, a plain browser tab) there is no
      // relay to ask; the last announced word stands.
    }
  },

  sendPrompt: async (text) => {
    const trimmed = text.trim();
    if (trimmed === '' || get().turn !== null) {
      return;
    }
    const userMessage: AgentMessage = { id: messageId(), role: 'user', text: trimmed };
    set((state) => ({ messages: [...state.messages, userMessage] }));
    try {
      const turn = await agentPrompt(trimmed);
      set({ turn });
    } catch (err) {
      set((state) => ({
        messages: [
          ...state.messages,
          { id: messageId(), role: 'note', text: `the prompt failed: ${String(err)}` },
        ],
      }));
    }
  },

  abort: async () => {
    const turn = get().turn;
    if (turn === null) {
      return;
    }
    try {
      await agentAbort(turn);
    } catch {
      // The done event (or the host's drain) settles the turn regardless;
      // a refused abort leaves the card exactly as it was.
    }
  },

  answer: async (id, approved) => {
    try {
      await permissionResponse(id, approved, false);
      // The card comes off on the resolved event, not here: the host's
      // fail-close paths settle approvals this surface never sent.
    } catch (err) {
      set((state) => ({
        messages: [
          ...state.messages,
          { id: messageId(), role: 'note', text: `the answer failed: ${String(err)}` },
        ],
      }));
    }
  },

  applyStreamEvent: (event) => {
    set((state) => {
      const last = state.messages[state.messages.length - 1];
      switch (event.event.kind) {
        case 'text_delta': {
          // Streaming appends to the trailing assistant bubble of the turn,
          // opening one when the turn has not been heard from yet.
          if (last !== undefined && last.role === 'assistant') {
            const messages = state.messages.slice(0, -1);
            messages.push({ ...last, text: last.text + event.event.text });
            return { messages };
          }
          return {
            messages: [
              ...state.messages,
              { id: messageId(), role: 'assistant' as const, text: event.event.text },
            ],
          };
        }
        case 'tool_call':
          return {
            messages: [
              ...state.messages,
              { id: messageId(), role: 'note' as const, text: `called ${event.event.name}` },
            ],
          };
        case 'tool_result':
          return {
            messages: [
              ...state.messages,
              {
                id: messageId(),
                role: 'note' as const,
                text: `${event.event.name}: ${event.event.status}`,
              },
            ],
          };
        case 'done':
          return { turn: null };
        case 'error':
          return {
            turn: null,
            messages: [
              ...state.messages,
              { id: messageId(), role: 'note' as const, text: event.event.message },
            ],
          };
        case 'state':
          return state;
        default:
          return state;
      }
    });
  },

  addApproval: (approval) =>
    set((state) => ({
      approvals: state.approvals.some((existing) => existing.id === approval.id)
        ? state.approvals
        : [...state.approvals, approval],
    })),

  resolveApproval: (id) =>
    set((state) => ({
      approvals: state.approvals.filter((existing) => existing.id !== id),
    })),

  reset: () => set({ status: 'down', turn: null, messages: [], approvals: [] }),
}));
