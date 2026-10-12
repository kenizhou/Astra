// SPDX-License-Identifier: GPL-3.0-only
// The agent panel: the chat log with its streaming assistant turn, the
// input that sends prompts, one approval card per asked permission with
// the answer dispatched straight back, and the abort button for the live
// turn. Streamed events arrive through the agent store (the host's
// `agent:event` emissions, folded in by the app root's listener); the
// status word beside the title says where the relay stands.

import { useEffect, useRef, useState } from 'react';

import { useAgentStore, type AgentMessage } from './agentStore';
import { AgentIcon } from '@/components/icons';
import { RAIL_BUTTON_CLASS, railButtonColours } from '@/components/layout/railButton';
import { useUiStore } from '@/stores/uiStore';
import type { PendingApproval } from '@/services/agent';

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

/** The agent panel over the shell's corner; renders nothing when shut. */
export function AgentPanel() {
  const open = useUiStore((s) => s.agentOpen);
  const status = useAgentStore((s) => s.status);
  const turn = useAgentStore((s) => s.turn);
  const messages = useAgentStore((s) => s.messages);
  const approvals = useAgentStore((s) => s.approvals);
  const sendPrompt = useAgentStore((s) => s.sendPrompt);
  const abort = useAgentStore((s) => s.abort);
  const [draft, setDraft] = useState('');
  const logEnd = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // The log follows the newest entry, so a streaming turn stays visible.
    // jsdom implements no scrolling; the optional call keeps tests honest.
    logEnd.current?.scrollIntoView?.({ block: 'end' });
  }, [messages]);

  useEffect(() => {
    // Opening the panel asks the host where the relay stands: the events
    // announce ready and lost, but the words between them (restarting,
    // starting) only the status pull knows.
    if (open) {
      void useAgentStore.getState().refreshStatus();
    }
  }, [open]);

  if (!open) {
    return null;
  }

  const send = (): void => {
    void sendPrompt(draft);
    setDraft('');
  };

  return (
    <section
      aria-label="Agent"
      className="absolute bottom-[calc(var(--status-h)+12px)] right-3 z-[var(--z-popover)] flex max-h-[70vh] w-[380px] flex-col overflow-hidden rounded-[var(--radius)] border border-border bg-chrome shadow-lg"
    >
      <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-between border-b border-border px-3">
        <span className="flex items-baseline gap-2 text-xs font-medium text-foreground">
          Agent
          <span className="text-[11px] text-muted-foreground">{status}</span>
        </span>
        <button
          type="button"
          aria-label="Close agent"
          onClick={() => useUiStore.getState().setAgentOpen(false)}
          className={actionClass}
        >
          Close
        </button>
      </div>
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
        {turn !== null && (
          <p className="mt-1 px-1 text-[11px] text-muted-foreground" aria-live="polite">
            thinking
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
        className="flex shrink-0 items-center gap-1 border-t border-border px-2 py-2"
        onSubmit={(event) => {
          event.preventDefault();
          send();
        }}
      >
        <input
          type="text"
          aria-label="Message the agent"
          value={draft}
          disabled={turn !== null}
          onChange={(event) => setDraft(event.target.value)}
          placeholder={turn !== null ? 'The agent is answering' : 'Ask the agent'}
          className="min-w-0 flex-1 rounded-[var(--radius-xs)] border border-border bg-surface px-2 py-1 text-xs text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
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
