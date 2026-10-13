// SPDX-License-Identifier: GPL-3.0-only
// The ambient island: a centred pill on the title bar that is the search
// entry at rest and the search box once it opens. A click or Ctrl+K opens
// it into the debounced query field with its scope dropdown; Escape on a
// typed query clears it at once, Escape on an empty field folds the island
// back to a pill, and Enter submits without waiting out the pause, keeping
// the query in the session's history. While the field stands empty the
// history drops down below, and picking one runs that search again. The
// field echoes the query the snapshot's search state carries, so a search
// that landed or exited elsewhere shows here truthfully; the echo never
// arms the debounce itself. The island opens with the field already out
// when a search stands. The trailing dot is the agent's presence: muted at
// rest, primary with a pulse while a turn streams, warning while an
// approval waits, and clicking it opens the agent panel without touching
// the search.

import { useEffect, useRef, useState } from 'react';

import { SearchIcon } from '@/components/icons';
import { useAgentStore } from '@/features/agent/agentStore';
import { dispatch, intents } from '@/services/ipc';
import type { SearchScope } from '@/services/surfaces';
import { useListStore } from '@/stores/listStore';
import { useSearchHistoryStore } from '@/stores/searchHistoryStore';
import { useUiStore } from '@/stores/uiStore';

/** How long the search field sits quiet before its query crosses. */
export const SEARCH_DEBOUNCE_MS = 500;

/** The scopes the dropdown offers, in the wire's own order. */
const SCOPES: ReadonlyArray<{ id: SearchScope; label: string }> = [
  { id: 'all_except_trash', label: 'All except trash' },
  { id: 'current_folder', label: 'Current folder' },
  { id: 'current_account', label: 'Current account' },
];

/** The agent presence words, in rising order of attention. */
type AgentPresence = 'idle' | 'working' | 'approval';

/** Reads the agent store down to the one presence the dot draws. */
function agentPresence(status: { turn: string | null; approvals: unknown[] }): AgentPresence {
  if (status.approvals.length > 0) {
    return 'approval';
  }
  if (status.turn !== null) {
    return 'working';
  }
  return 'idle';
}

const PRESENCE_DOT_CLASS: Record<AgentPresence, string> = {
  idle: 'bg-muted-foreground/40',
  working: 'animate-pulse bg-primary',
  approval: 'bg-warning',
};

const PRESENCE_LABEL: Record<AgentPresence, string> = {
  idle: 'Open agent panel',
  working: 'Agent working; open agent panel',
  approval: 'Approval pending; open agent panel',
};

/** The trailing presence dot; opens the agent panel, never the search. */
function AgentPresenceDot() {
  const turn = useAgentStore((s) => s.turn);
  const approvals = useAgentStore((s) => s.approvals);
  const presence = agentPresence({ turn, approvals });

  return (
    <button
      type="button"
      aria-label={PRESENCE_LABEL[presence]}
      title={PRESENCE_LABEL[presence]}
      data-agent-presence={presence}
      onClick={(event) => {
        event.stopPropagation();
        useUiStore.getState().setAgentOpen(true);
      }}
      className="mr-1 grid h-5 w-5 shrink-0 place-items-center rounded-full transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <span
        aria-hidden="true"
        className={`h-2 w-2 rounded-full ${PRESENCE_DOT_CLASS[presence]}`}
      />
    </button>
  );
}

export function AmbientIsland() {
  const echo = useListStore((s) => s.search?.query ?? null);
  const scope = useListStore((s) => s.search?.scope ?? 'all_except_trash');
  const history = useSearchHistoryStore((s) => s.queries);
  const [expanded, setExpanded] = useState(echo !== null);
  const [text, setText] = useState(echo ?? '');
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const query = useRef(text);
  const inputRef = useRef<HTMLInputElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setText(echo ?? '');
  }, [echo]);

  // The opening click hands the field the focus once it has mounted.
  useEffect(() => {
    if (expanded) {
      inputRef.current?.focus();
    }
  }, [expanded]);

  // Ctrl+K opens the island from anywhere; a handler that claimed the key
  // first (a preventDefault upstream) keeps it.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== 'k') {
        return;
      }
      if (event.defaultPrevented) {
        return;
      }
      event.preventDefault();
      setExpanded(true);
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, []);

  // A click outside folds the island back to its pill.
  useEffect(() => {
    if (!expanded) {
      return;
    }
    const onPointerDown = (event: MouseEvent): void => {
      if (wrapRef.current?.contains(event.target as Node) !== true) {
        collapse();
      }
    };
    document.addEventListener('mousedown', onPointerDown);
    return () => document.removeEventListener('mousedown', onPointerDown);
  }, [expanded]);

  // A pause the field armed must never outlive the island.
  useEffect(() => {
    return () => {
      if (timer.current !== null) {
        clearTimeout(timer.current);
        timer.current = null;
      }
    };
  }, []);

  const disarm = (): void => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
  };

  /** Folds back to the pill, the field settling on the search that stands. */
  const collapse = (): void => {
    disarm();
    const standing = useListStore.getState().search?.query ?? '';
    query.current = standing;
    setText(standing);
    setExpanded(false);
  };

  const onQueryChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    query.current = event.target.value;
    setText(event.target.value);
    disarm();
    timer.current = setTimeout(() => {
      timer.current = null;
      void dispatch(intents.search(query.current));
    }, SEARCH_DEBOUNCE_MS);
  };

  /** Enter submits without the pause and keeps the query for the session. */
  const submit = (value: string): void => {
    disarm();
    query.current = value;
    setText(value);
    const trimmed = value.trim();
    if (trimmed !== '') {
      useSearchHistoryStore.getState().record(trimmed);
    }
    void dispatch(intents.search(value === '' ? null : value));
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      submit(query.current);
      return;
    }
    if (event.key !== 'Escape') {
      return;
    }
    // A typed query clears at once; an empty field folds the island away.
    if (query.current !== '') {
      disarm();
      query.current = '';
      setText('');
      void dispatch(intents.search(null));
      return;
    }
    collapse();
  };

  const onScopeChange = (event: React.ChangeEvent<HTMLSelectElement>) => {
    void dispatch(intents.setSearchScope(event.target.value as SearchScope));
  };

  /** A history pick runs that search again and keeps the field's focus. */
  const onHistoryPick = (value: string): void => {
    submit(value);
    inputRef.current?.focus();
  };

  const showHistory = expanded && text === '' && history.length > 0;

  return (
    <div
      ref={wrapRef}
      className={`relative flex h-7 shrink-0 items-center gap-1 rounded-full border border-border-subtle transition-[width] duration-200 focus-within:ring-2 focus-within:ring-ring ${
        expanded ? 'w-[min(420px,100%)] bg-card pl-1' : 'w-[190px] bg-card'
      }`}
    >
      {expanded ? (
        <>
          <select
            aria-label="Search scope"
            title="Search scope"
            value={scope}
            onChange={onScopeChange}
            className="max-w-[7.5rem] shrink-0 cursor-pointer truncate bg-transparent text-xs text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:text-foreground"
          >
            {SCOPES.map(({ id, label }) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
          <span aria-hidden="true" className="h-4 w-px shrink-0 bg-border" />
          <input
            ref={inputRef}
            type="text"
            aria-label="Search mail"
            placeholder="Search mail"
            value={text}
            onChange={onQueryChange}
            onKeyDown={onKeyDown}
            className="h-full min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground"
          />
        </>
      ) : (
        <button
          type="button"
          aria-label="Search mail"
          title="Search mail (Ctrl+K)"
          onClick={() => setExpanded(true)}
          className="flex h-full min-w-0 flex-1 items-center gap-1.5 rounded-full pl-2.5 text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none"
        >
          <SearchIcon size={13} className="shrink-0" />
          <span className="min-w-0 flex-1 truncate text-left text-xs">Search mail</span>
        </button>
      )}
      <AgentPresenceDot />
      {showHistory && (
        <div
          role="listbox"
          aria-label="Recent searches"
          className="absolute left-0 top-full z-[var(--z-dropdown)] mt-1 w-full overflow-hidden rounded-[var(--radius-sm)] border border-border-subtle bg-surface-floating py-1 shadow-[var(--shadow-lg)]"
        >
          {history.map((entry) => (
            <button
              key={entry}
              type="button"
              role="option"
              aria-selected={false}
              onClick={() => onHistoryPick(entry)}
              className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <SearchIcon size={12} className="shrink-0 text-muted-foreground" />
              <span className="truncate">{entry}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
