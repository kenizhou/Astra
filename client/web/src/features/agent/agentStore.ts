// SPDX-License-Identifier: GPL-3.0-only
// The agent store: the panel's chat log with its streaming assistant turn,
// the approvals awaiting the user's decision, the relay's lifecycle word,
// and the chat threads the log persists into. Stream events arrive from
// the host's `agent:event` emissions and fold in here; the user's own
// gestures (send, abort, answer, select, rename, delete) ride straight
// out through the IPC wrappers.
//
// ONE source of truth for the visible log: the store's rows. A loaded
// thread paints only what `chat_load` answered; the runtime's own session
// files never load. The visible log is the painted base (rows, plus what
// each settled turn leaves) with the current session's live buffer on
// top; the host folds that buffer into the row at the turn's done, and
// the buffer lets go then.

import { create } from 'zustand';

import {
  agentAbort,
  agentPrompt,
  agentStatus,
  chatDelete,
  chatList,
  chatLoad,
  chatRename,
  permissionResponse,
  type AgentStreamEvent,
  type ChatMessage,
  type ChatSummary,
  type PendingApproval,
} from '@/services/agent';

/** One chat log entry: who said it and what. */
export interface AgentMessage {
  id: string;
  role: 'user' | 'assistant' | 'note';
  text: string;
}

/** The session id a chat's turns ride, the host derivation mirrored. */
export function sessionOf(chatId: string | null): string {
  return chatId === null ? 'agent:main' : `agent:chat:${chatId}`;
}

/** The store's message id counter, so entries key stably across renders. */
let nextMessageId = 0;

/** The prompt ids whose settle event already crossed, in any order. A
 *  settle can beat its own receipt across the IPC boundary; adopting that
 *  receipt's turn afterwards would wedge the input on a prompt no future
 *  event ever names again. */
const settledPrompts = new Set<string>();

/** The next message id, module-scoped so every turn's entries stay unique. */
function messageId(): string {
  nextMessageId += 1;
  return `m-${nextMessageId}`;
}

/** One message row as log entries: the row's parts in order, the same
 *  shapes the live fold produces, so a reloaded turn paints what the
 *  live turn rendered. */
function rowEntries(row: ChatMessage): AgentMessage[] {
  const entries: AgentMessage[] = [];
  for (const [at, part] of row.parts.entries()) {
    if (part.kind === 'text') {
      entries.push({
        id: `row-${row.id}-${at}`,
        role: row.role === 'user' ? 'user' : 'assistant',
        text: part.text ?? '',
      });
    } else if (part.kind === 'tool') {
      entries.push({ id: `row-${row.id}-${at}`, role: 'note', text: `${part.name}: ${part.status}` });
    } else if (part.kind === 'note') {
      entries.push({ id: `row-${row.id}-${at}`, role: 'note', text: part.text ?? '' });
    }
  }
  return entries;
}

interface AgentStoreState {
  /** The relay's lifecycle word: down, starting, ready, restarting, lost. */
  status: string;
  /** The live turn's prompt id, or null between turns. */
  turn: string | null;
  /** The session the live turn runs in; null between turns. */
  turnSession: string | null;
  /** Whether the live turn's tool budget is spent: the bound has bitten,
   *  the assistant keeps streaming but its tool calls answer skipped. */
  budgetSpent: boolean;
  /** The chat threads, the most recently touched first. */
  chats: ChatSummary[];
  /** The thread whose log is visible, or null for a fresh thread the next
   *  send mints. */
  currentChatId: string | null;
  /** The painted base of the visible log: the loaded rows plus what each
   *  settled turn left behind. */
  painted: AgentMessage[];
  /** The live entries each session's in-flight turn has rendered, keyed by
   *  session id: the buffer a background turn streams into while another
   *  thread is visible, merged back on return, folded into the row and
   *  dropped at the turn's done. */
  liveBySession: Record<string, AgentMessage[]>;
  /** The visible log: the painted base with the current session's live
   *  buffer on top. */
  messages: AgentMessage[];
  /** The approvals awaiting a decision, oldest first. */
  approvals: PendingApproval[];
  /** Applies the relay lifecycle word. */
  setStatus: (status: string) => void;
  /** Pulls the relay lifecycle word from the host, so the panel shows the
   *  state the events have not announced (restarting, most often). */
  refreshStatus: () => Promise<void>;
  /** Pulls the thread list from the host. */
  refreshChats: () => Promise<void>;
  /** Selects a thread (its rows paint) or null (a fresh thread). */
  selectChat: (chatId: string | null) => Promise<void>;
  /** Renames a thread and refreshes the list. */
  renameChat: (chatId: string, title: string) => Promise<void>;
  /** Deletes a thread; the view clears when it was the selected one. */
  deleteChat: (chatId: string) => Promise<void>;
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
  turnSession: null,
  budgetSpent: false,
  chats: [],
  currentChatId: null,
  painted: [],
  liveBySession: {},
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

  refreshChats: async () => {
    try {
      const chats = await chatList();
      set({ chats });
    } catch {
      // Outside the desktop host there is no store to ask; the list stands.
    }
  },

  selectChat: async (chatId) => {
    if (chatId === null) {
      set({ currentChatId: null, painted: [], messages: [] });
      return;
    }
    try {
      const loaded = await chatLoad(chatId);
      const painted = loaded.messages.flatMap(rowEntries);
      const live = get().liveBySession[sessionOf(chatId)] ?? [];
      // A turn still live in this thread shows its buffered tail beside
      // the rows; a settled one is already among them.
      set({ currentChatId: chatId, painted, messages: [...painted, ...live] });
    } catch (err) {
      set((state) => ({
        painted: [
          ...state.painted,
          { id: messageId(), role: 'note' as const, text: `the thread failed to load: ${String(err)}` },
        ],
      }));
      set((state) => ({ messages: [...state.painted, ...(state.liveBySession[sessionOf(state.currentChatId)] ?? [])] }));
    }
  },

  renameChat: async (chatId, title) => {
    try {
      await chatRename(chatId, title);
      await get().refreshChats();
    } catch (err) {
      note(set, `the rename failed: ${String(err)}`);
    }
  },

  deleteChat: async (chatId) => {
    try {
      await chatDelete(chatId);
      if (get().currentChatId === chatId) {
        set({ currentChatId: null, painted: [], messages: [] });
      }
      await get().refreshChats();
    } catch (err) {
      note(set, `the delete failed: ${String(err)}`);
    }
  },

  sendPrompt: async (text) => {
    const trimmed = text.trim();
    if (trimmed === '' || get().turn !== null) {
      return;
    }
    const userMessage: AgentMessage = { id: messageId(), role: 'user', text: trimmed };
    const chatId = get().currentChatId;
    // The user's bubble paints at once and stays painted: a failed send
    // never produces a live buffer to carry it, so the next fold's
    // recompute would erase an unpainted bubble.
    set((state) => ({
      painted: [...state.painted, userMessage],
      messages: [...state.messages, userMessage],
    }));
    try {
      const receipt = await agentPrompt(trimmed, undefined, chatId ?? undefined);
      const session = sessionOf(receipt.chat_id);
      set((state) => {
        const sameChat = receipt.chat_id === state.currentChatId;
        if (settledPrompts.has(receipt.prompt_id)) {
          // The settle crossed before the receipt resolved: the live
          // buffer is already folded away and no future event names this
          // prompt again, so the turn is never adopted (the input never
          // wedges on it) and the chat keeps the painted bubble alone.
          const settledPainted = sameChat ? state.painted : [userMessage];
          return {
            currentChatId: receipt.chat_id,
            painted: settledPainted,
            messages: settledPainted,
          };
        }
        // A minted thread adopts the receipt's id. Events that crossed
        // before the receipt sit in the session's live buffer above the
        // painted base, so the visible log's base-plus-live shape holds
        // from the turn's first entry.
        const early = state.liveBySession[session] ?? [];
        const painted = sameChat ? state.painted : [userMessage];
        return {
          currentChatId: receipt.chat_id,
          painted,
          turn: receipt.prompt_id,
          turnSession: session,
          budgetSpent: false,
          liveBySession: { ...state.liveBySession, [session]: early },
          messages: [...painted, ...early],
        };
      });
      void get().refreshChats();
    } catch (err) {
      note(set, `the prompt failed: ${String(err)}`);
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
      note(set, `the answer failed: ${String(err)}`);
    }
  },

  applyStreamEvent: (event) => {
    set((state) => {
      const session = event.session_id;
      const live = state.liveBySession[session] ?? [];
      const visible = session === sessionOf(state.currentChatId);
      switch (event.event.kind) {
        case 'text_delta': {
          // Streaming appends to the trailing assistant bubble of the turn,
          // opening one when the turn has not been heard from yet.
          const last = live[live.length - 1];
          const next =
            last !== undefined && last.role === 'assistant'
              ? [...live.slice(0, -1), { ...last, text: last.text + event.event.text }]
              : [...live, { id: messageId(), role: 'assistant' as const, text: event.event.text }];
          return foldLive(state, session, next, visible);
        }
        case 'tool_call':
          return foldLive(state, session, [
            ...live,
            { id: messageId(), role: 'note' as const, text: `called ${event.event.name}` },
          ], visible);
        case 'tool_result':
          return foldLive(state, session, [
            ...live,
            {
              id: messageId(),
              role: 'note' as const,
              text: `${event.event.name}: ${event.event.status}`,
            },
          ], visible);
        case 'done': {
          // The fold point: the host has recorded the row this buffer
          // became. The buffer lets go; a visible thread keeps its entries
          // as the painted base (they mirror the row), a hidden one drops
          // them (its next load paints the row). The id is remembered even
          // when no turn was adopted yet: its receipt may still be in
          // flight.
          settledPrompts.add(event.event.prompt_id);
          const { [session]: settledLive, ...rest } = state.liveBySession;
          const settledTurn =
            event.event.prompt_id === state.turn
              ? { turn: null, turnSession: null, budgetSpent: false }
              : {};
          const painted = visible ? [...state.painted, ...(settledLive ?? [])] : state.painted;
          return {
            ...state,
            ...settledTurn,
            painted,
            messages: visible ? painted : state.messages,
            liveBySession: rest,
          };
        }
        case 'error': {
          if (event.event.prompt_id !== undefined) {
            // A named failure settles its prompt the way a done does, so
            // the receipt that is still in flight never adopts it.
            settledPrompts.add(event.event.prompt_id);
          }
          const { [session]: failedLive, ...rest } = state.liveBySession;
          // An error naming the live prompt settles it; an error naming no
          // prompt (the sidecar at large) settles it too, the old contract.
          const settlesTurn =
            event.event.prompt_id === state.turn || event.event.prompt_id === undefined;
          const settledTurn = settlesTurn
            ? { turn: null, turnSession: null, budgetSpent: false }
            : {};
          const painted = visible
            ? [
                ...state.painted,
                ...(failedLive ?? []),
                { id: messageId(), role: 'note' as const, text: event.event.message },
              ]
            : state.painted;
          return {
            ...state,
            ...settledTurn,
            painted,
            messages: visible ? painted : state.messages,
            liveBySession: rest,
          };
        }
        case 'state':
          // Only the budget's word surfaces: the harness's own states
          // (thinking, done-streaming) stay noise the log does not need.
          if (event.event.state === 'budget-spent') {
            return foldLive(
              state,
              session,
              [
                ...live,
                {
                  id: messageId(),
                  role: 'note' as const,
                  text: 'the tool budget is spent; the assistant finishes in text',
                },
              ],
              visible,
              { budgetSpent: true },
            );
          }
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

  reset: () => {
    settledPrompts.clear();
    set({
      status: 'down',
      turn: null,
      turnSession: null,
      budgetSpent: false,
      chats: [],
      currentChatId: null,
      painted: [],
      liveBySession: {},
      messages: [],
      approvals: [],
    });
  },
}));

/** Writes one folded live list back: the session's buffer always, the
 *  visible log (the painted base with the buffer on top) when the session
 *  is the shown thread's. */
function foldLive(
  state: AgentStoreState,
  session: string,
  next: AgentMessage[],
  visible: boolean,
  extra?: Partial<AgentStoreState>,
): AgentStoreState {
  return {
    ...state,
    ...extra,
    liveBySession: { ...state.liveBySession, [session]: next },
    messages: visible ? [...state.painted, ...next] : state.messages,
  };
}

/** Lands a transient failure note in the painted base, so it survives the
 *  next fold's recompute (it is UI state; no row ever records it). */
function note(
  set: (partial: (state: AgentStoreState) => Partial<AgentStoreState>) => void,
  text: string,
): void {
  set((state) => {
    const entry: AgentMessage = { id: messageId(), role: 'note', text };
    return {
      painted: [...state.painted, entry],
      messages: [...state.messages, entry],
    };
  });
}
