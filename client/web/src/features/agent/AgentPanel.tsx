// SPDX-License-Identifier: GPL-3.0-only
// The agent panel: the chat log with its streaming assistant turn, the
// input that sends prompts, one approval card per asked permission with
// the answer dispatched straight back, and the abort button for the live
// turn. The thread strip above the log keeps the assistant's chats: it
// paints the store's rows, selects, renames, and deletes them, and the
// new-chat gesture starts a fresh thread. Streamed events arrive through
// the agent store (the host's `agent:event` emissions, folded in by the
// app root's listener); the status word beside the title says where the
// relay stands. The panel docks as the mail row's resizable right column;
// the tool rail's toggle opens and shuts it.

import { useEffect, useRef, useState } from 'react';

import { sessionOf, useAgentStore, type AgentMessage } from './agentStore';
import { AgentIcon, CloseIcon } from '@/components/icons';
import { RAIL_BUTTON_CLASS, railButtonColours } from '@/components/layout/railButton';
import { useUiStore } from '@/stores/uiStore';
import type { ChatSummary, PendingApproval } from '@/services/agent';

const actionClass =
  'rounded-[var(--radius-xs)] px-1.5 py-0.5 text-[11px] text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';

/** One chat log entry: a user bubble, an assistant bubble, or a note line. */
function LogEntry({ message }: { message: AgentMessage }) {
  if (message.role === 'note') {
    return <p className="px-1 text-[11px] italic text-muted-foreground">{message.text}</p>;
  }
  const own = message.role === 'user';
  return (
    <p
      className={
        own
          ? 'ml-6 rounded-[var(--radius-xs)] bg-hover px-2 py-1 text-xs text-foreground'
          : 'mr-6 rounded-[var(--radius-xs)] bg-surface px-2 py-1 text-xs text-foreground'
      }
    >
      {message.text}
    </p>
  );
}

/** The tool's arguments as the card shows them: one compact JSON line. */
function argsPreview(args: Record<string, unknown>): string {
  return JSON.stringify(args);
}

/** One asked permission, with the answer the host routes back. */
function ApprovalCard({ approval }: { approval: PendingApproval }) {
  const answer = useAgentStore((s) => s.answer);
  return (
    <article
      aria-label={`Approval ${approval.tool}`}
      className="rounded-[var(--radius-xs)] border border-border bg-surface px-2 py-1.5"
    >
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-xs font-medium text-foreground">{approval.tool}</span>
        <span className="text-[11px] text-muted-foreground">{approval.level}</span>
      </div>
      {/* The request id, so a support conversation can name the exact ask
       *  the ledger's audit rows carry. */}
      <p className="mt-0.5 truncate text-[10px] text-muted-foreground" title={approval.id}>
        {approval.id}
      </p>
      <p className="mt-0.5 truncate text-[11px] text-muted-foreground" title={argsPreview(approval.args)}>
        {argsPreview(approval.args)}
      </p>
      <div className="mt-1 flex items-center justify-end gap-1">
        <button
          type="button"
          className={`${actionClass} text-success`}
          onClick={() => void answer(approval.id, true)}
        >
          Approve
        </button>
        <button type="button" className={actionClass} onClick={() => void answer(approval.id, false)}>
          Reject
        </button>
      </div>
    </article>
  );
}

/** One thread row of the strip: the title selects, the affordances rename
 *  and delete. A rename in flight swaps the title for a one-line input. */
function ThreadRow({
  chat,
  selected,
  renaming,
  onRename,
}: {
  chat: ChatSummary;
  selected: boolean;
  renaming: boolean;
  onRename: (id: string | null) => void;
}) {
  const selectChat = useAgentStore((s) => s.selectChat);
  const renameChat = useAgentStore((s) => s.renameChat);
  const deleteChat = useAgentStore((s) => s.deleteChat);
  const [draft, setDraft] = useState(chat.title);

  if (renaming) {
    return (
      <form
        className="flex shrink-0 items-center gap-1"
        onSubmit={(event) => {
          event.preventDefault();
          const title = draft.trim();
          if (title !== '' && title !== chat.title) {
            void renameChat(chat.id, title);
          }
          onRename(null);
        }}
      >
        <input
          aria-label="Rename thread"
          className="w-28 rounded-[var(--radius-xs)] border border-border bg-surface px-1.5 py-0.5 text-[11px] text-foreground focus-visible:outline-none focus-visible:border-primary"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              onRename(null);
            }
          }}
          autoFocus
        />
        <button type="submit" className={actionClass}>
          Save
        </button>
      </form>
    );
  }
  return (
    <span className="flex shrink-0 items-center gap-0.5">
      <button
        type="button"
        aria-current={selected ? 'true' : undefined}
        title={chat.title}
        className={`max-w-36 truncate rounded-[var(--radius-xs)] px-1.5 py-0.5 text-[11px] transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
          selected ? 'bg-hover text-foreground' : 'text-muted-foreground'
        }`}
        onClick={() => void selectChat(chat.id)}
      >
        {chat.title}
      </button>
      <button
        type="button"
        aria-label={`Rename ${chat.title}`}
        className={actionClass}
        onClick={() => {
          setDraft(chat.title);
          onRename(chat.id);
        }}
      >
        Rename
      </button>
      <button
        type="button"
        aria-label={`Delete ${chat.title}`}
        className={actionClass}
        onClick={() => void deleteChat(chat.id)}
      >
        Delete
      </button>
    </span>
  );
}

/** The agent panel, docked in the mail row; renders nothing when shut. */
export function AgentPanel() {
  const open = useUiStore((s) => s.agentOpen);
  const status = useAgentStore((s) => s.status);
  const turn = useAgentStore((s) => s.turn);
  const turnSession = useAgentStore((s) => s.turnSession);
  const currentChatId = useAgentStore((s) => s.currentChatId);
  const budgetSpent = useAgentStore((s) => s.budgetSpent);
  const messages = useAgentStore((s) => s.messages);
  const chats = useAgentStore((s) => s.chats);
  const approvals = useAgentStore((s) => s.approvals);
  const sendPrompt = useAgentStore((s) => s.sendPrompt);
  const abort = useAgentStore((s) => s.abort);
  const [draft, setDraft] = useState('');
  const [renaming, setRenaming] = useState<string | null>(null);
  const logEnd = useRef<HTMLDivElement>(null);
  const draftRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    // The log follows the newest entry, so a streaming turn stays visible.
    // jsdom implements no scrolling; the optional call keeps tests honest.
    logEnd.current?.scrollIntoView?.({ block: 'end' });
  }, [messages]);

  useEffect(() => {
    // Opening the panel asks the host where the relay stands and which
    // threads exist: the events announce ready and lost, but the words
    // between them and the store's rows only the pulls know.
    if (open) {
      void useAgentStore.getState().refreshStatus();
      void useAgentStore.getState().refreshChats();
    }
  }, [open]);

  if (!open) {
    return null;
  }

  const send = (): void => {
    void sendPrompt(draft);
    setDraft('');
    // The cleared draft leaves the grown box tall; bring it back to one row.
    if (draftRef.current !== null) {
      draftRef.current.style.height = 'auto';
    }
  };

  const turnHere = turn !== null && turnSession === sessionOf(currentChatId);

  return (
    <section
      aria-label="Agent"
      className="flex h-full w-full flex-col overflow-hidden bg-background"
    >
      <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-between border-b border-border px-3">
        <span className="flex items-baseline gap-2 text-xs font-medium text-foreground">
          Agent
          <span className="text-[11px] text-muted-foreground">{status}</span>
        </span>
        <span className="flex items-center gap-1">
          <button
            type="button"
            aria-label="New chat"
            className={actionClass}
            onClick={() => void useAgentStore.getState().selectChat(null)}
          >
            New chat
          </button>
          <button
            type="button"
            aria-label="Close agent"
            onClick={() => useUiStore.getState().setAgentOpen(false)}
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <CloseIcon size={14} />
          </button>
        </span>
      </div>
      {chats.length > 0 && (
        <div
          role="list"
          aria-label="Agent threads"
          className="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-border px-2 py-1"
        >
          {chats.map((chat) => (
            <div role="listitem" key={chat.id} className="flex shrink-0">
              <ThreadRow
                chat={chat}
                selected={chat.id === currentChatId}
                renaming={renaming === chat.id}
                onRename={setRenaming}
              />
            </div>
          ))}
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-2">
        <ol
          role="log"
          aria-label="Agent conversation"
          className="flex flex-col gap-1.5"
        >
          {messages.map((message) => (
            <li key={message.id}>
              <LogEntry message={message} />
            </li>
          ))}
        </ol>
        {turnHere && (
          <p className="mt-1 px-1 text-[11px] text-muted-foreground" aria-live="polite">
            {budgetSpent ? 'finishing in text' : 'thinking'}
          </p>
        )}
        <div ref={logEnd} />
      </div>
      {approvals.length > 0 && (
        <div className="flex shrink-0 flex-col gap-1.5 border-t border-border px-2 py-2">
          {approvals.map((approval) => (
            <ApprovalCard key={approval.id} approval={approval} />
          ))}
        </div>
      )}
      <form
        className="flex shrink-0 items-end gap-1 border-t border-border px-2 py-2"
        onSubmit={(event) => {
          event.preventDefault();
          send();
        }}
      >
        <textarea
          ref={draftRef}
          aria-label="Message the agent"
          value={draft}
          disabled={turn !== null}
          rows={1}
          onChange={(event) => {
            setDraft(event.target.value);
            // Auto-grow to the content, capped so a long paste scrolls
            // instead of swallowing the log.
            const el = event.target;
            el.style.height = 'auto';
            el.style.height = `${Math.min(el.scrollHeight, 120)}px`;
          }}
          onKeyDown={(event) => {
            // Enter sends, Shift+Enter breaks the line: the chat idiom.
            if (event.key === 'Enter' && !event.shiftKey) {
              event.preventDefault();
              if (turn === null && draft.trim() !== '') {
                send();
              }
            }
          }}
          placeholder={turn !== null ? 'The agent is answering' : 'Ask the agent'}
          className="scrollbar-none max-h-[120px] min-w-0 flex-1 resize-none rounded-xl border border-border bg-surface px-3 py-1.5 text-xs text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:border-primary"
        />
        {turn === null ? (
          <button type="submit" className={actionClass}>
            Send
          </button>
        ) : (
          <button type="button" className={actionClass} onClick={() => void abort()}>
            Abort
          </button>
        )}
      </form>
    </section>
  );
}

/** The tool rail's agent toggle, carrying the approvals awaiting a decision. */
export function AgentStatus() {
  const open = useUiStore((s) => s.agentOpen);
  const count = useAgentStore((s) => s.approvals.length);

  return (
    <button
      type="button"
      aria-label={count > 0 ? `Agent ${count}` : 'Agent'}
      aria-pressed={open}
      title={count > 0 ? `Agent, ${count} awaiting a decision` : 'Agent'}
      onClick={() => useUiStore.getState().setAgentOpen(!open)}
      className={`${RAIL_BUTTON_CLASS} ${railButtonColours(open)}`}
    >
      <AgentIcon size={22} />
      {count > 0 && (
        <span
          aria-hidden="true"
          className="absolute right-1 top-1 grid h-4 min-w-4 place-items-center rounded-full bg-primary px-1 text-[10px] font-medium text-primary-foreground"
        >
          {count}
        </span>
      )}
    </button>
  );
}
