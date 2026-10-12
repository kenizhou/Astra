// SPDX-License-Identifier: GPL-3.0-only
// The people page: the search box debouncing its query into the Search
// intent, the write statuses beside the list (never inside the detail
// pane), the unified list and the open person's detail beside it, the
// create affordance standing only where a writable book exists, the contact
// editor the create and edit affordances open, and the refresh on entry and
// on focus regain.

import { useEffect, useRef, useState } from 'react';

import { dispatch, intents } from '@/services/ipc';
import { SEARCH_DEBOUNCE_MS } from '@/components/layout/TitleBar';

import { ContactEditor } from './ContactEditor';
import { PeopleList } from './PeopleList';
import { PersonDetailPane } from './PersonDetail';
import { usePeopleStore } from './peopleStore';

/** The write statuses beside the list: a write settling, and one that did
 * not settle (its recovery is the refresh the Retry button asks for). */
function WriteStatuses() {
  const writeStates = usePeopleStore((s) => s.writeStates);
  const failed = writeStates.some((row) => row.state === 'failed_with_retry');
  const writing = writeStates.some((row) => row.state === 'writing');

  if (!writing && !failed) {
    return null;
  }
  return (
    <div
      role="status"
      className={`flex shrink-0 items-center justify-between gap-2 border-b px-3 py-1.5 text-xs ${
        failed
          ? 'border-[var(--warning)]/40 bg-[var(--warning)]/10 text-warning-foreground'
          : 'border-border bg-surface text-muted-foreground'
      }`}
    >
      {failed ? (
        <>
          <span>The last contact change did not settle. Refresh to see what stood.</span>
          <button
            type="button"
            aria-label="Retry"
            onClick={() => void dispatch(intents.refreshPeople())}
            className="rounded-[var(--radius)] border border-border px-2 py-0.5 text-xs text-foreground transition-colors hover:bg-hover"
          >
            Retry
          </button>
        </>
      ) : (
        <span className="inline-flex items-center gap-1.5">
          <span className="inline-block h-3 w-3 animate-spin rounded-full border-2 border-current border-t-transparent" />
          Saving contacts…
        </span>
      )}
    </div>
  );
}

/** The search box: typing debounces the query into the Search intent, one
 * ask per pause rather than one per keystroke, and Escape clears at once. */
function PeopleSearch() {
  const [text, setText] = useState('');
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timer.current !== null) {
        clearTimeout(timer.current);
      }
    };
  }, []);

  const onChange = (event: React.ChangeEvent<HTMLInputElement>): void => {
    setText(event.target.value);
    if (timer.current !== null) {
      clearTimeout(timer.current);
    }
    timer.current = setTimeout(() => {
      timer.current = null;
      void dispatch(intents.searchPeople(event.target.value));
    }, SEARCH_DEBOUNCE_MS);
  };

  return (
    <input
      type="text"
      aria-label="Search people"
      placeholder="Search people"
      value={text}
      onChange={onChange}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          if (timer.current !== null) {
            clearTimeout(timer.current);
            timer.current = null;
          }
          setText('');
          void dispatch(intents.searchPeople(''));
        }
      }}
      className="h-7 w-full rounded-[var(--radius-xs)] border border-input bg-background px-2 text-xs text-foreground outline-none transition-colors placeholder:text-muted-foreground focus:border-primary focus:ring-2 focus:ring-ring"
    />
  );
}

/** The people page the shell's view switch mounts. */
export function PeoplePane() {
  const rows = usePeopleStore((s) => s.rows);
  const detail = usePeopleStore((s) => s.detail);
  const createTargets = usePeopleStore((s) => s.createTargets);
  const writeStates = usePeopleStore((s) => s.writeStates);
  const [editing, setEditing] = useState<'create' | 'edit' | null>(null);

  // A people surface entering the view syncs: the same refresh a failed
  // write's recovery waits for, and the one a focus regain repeats.
  useEffect(() => {
    void dispatch(intents.refreshPeople());
    const onFocus = (): void => {
      void dispatch(intents.refreshPeople());
    };
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, []);

  return (
    <section
      aria-label="People"
      className="relative flex min-w-0 flex-1 flex-col overflow-hidden bg-background"
    >
      <div className="flex h-[var(--pane-header-h)] shrink-0 items-center gap-2 border-b border-border bg-surface px-3">
        <div className="w-[220px]">
          <PeopleSearch />
        </div>
        <div className="flex-1" />
        {createTargets.length > 0 && (
          <button
            type="button"
            onClick={() => setEditing('create')}
            className="rounded-[var(--radius-xs)] bg-primary px-2 py-1 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            New contact
          </button>
        )}
      </div>
      <div className="flex min-h-0 flex-1">
        <aside
          aria-label="People list"
          className="flex w-[320px] shrink-0 flex-col border-r border-border bg-surface"
        >
          <WriteStatuses />
          <PeopleList
            rows={rows}
            onOpen={(person) => void dispatch(intents.openPerson(person))}
          />
        </aside>
        {detail !== null && (
          <aside aria-label="Person detail" className="flex min-w-0 flex-1 flex-col bg-background">
            <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-end border-b border-border bg-surface px-2">
              <button
                type="button"
                aria-label="Close person"
                onClick={() => void dispatch(intents.closePerson())}
                className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                Close
              </button>
            </div>
            <PersonDetailPane
              detail={detail}
              onEdit={() => setEditing('edit')}
            />
          </aside>
        )}
      </div>
      {editing !== null && (
        <ContactEditor
          mode={editing}
          createTargets={createTargets}
          editableCards={detail?.editable_cards ?? []}
          writeStates={writeStates}
          onClose={() => setEditing(null)}
        />
      )}
    </section>
  );
}
