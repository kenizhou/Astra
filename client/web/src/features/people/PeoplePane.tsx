// SPDX-License-Identifier: GPL-3.0-only
// The people page: the search box debouncing its query into the Search
// intent, then a resizable two-card row: the list card (the write statuses
// above the unified list, never inside the detail card) and the detail
// card, which shows the mail-calendar placeholder while nobody is open.
// The create affordance stands only where a writable book exists, the
// contact editor the create and edit affordances open, and the refresh on
// entry and on focus regain.

import { useEffect, useRef, useState } from 'react';

import { Group, Panel, Separator } from 'react-resizable-panels';

import { dispatch, intents } from '@/services/ipc';
import { PeopleIcon, PlusIcon, SearchIcon } from '@/components/icons';
import { SEARCH_DEBOUNCE_MS } from '@/components/layout/TitleBar';
import { pct } from '@/components/layout/panelSizes';

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
          : 'border-border-subtle bg-surface text-muted-foreground'
      }`}
    >
      {failed ? (
        <>
          <span>The last contact change did not settle. Refresh to see what stood.</span>
          <button
            type="button"
            aria-label="Retry"
            onClick={() => void dispatch(intents.refreshPeople())}
            className="rounded-[var(--radius-xs)] border border-border-subtle px-2 py-0.5 text-xs text-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
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
    <div className="relative flex w-full items-center">
      <span className="pointer-events-none absolute left-2.5 flex items-center text-muted-text">
        <SearchIcon size={13} />
      </span>
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
        className="h-7 w-full rounded-full border border-border-subtle bg-card pl-7 pr-2 text-xs text-foreground outline-none transition-colors placeholder:text-muted-foreground focus:border-primary focus:ring-2 focus:ring-ring"
      />
    </div>
  );
}

/** One pane card: a rounded, subtly bordered surface filling its panel. */
function PaneCard({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full flex-col overflow-hidden rounded-xl border border-border-subtle bg-card">
      {children}
    </div>
  );
}

/** The detail card before a person is picked: never a blank half-window. */
function PersonPlaceholder() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 text-muted-foreground">
      <PeopleIcon size={36} />
      <p className="text-sm">People</p>
    </div>
  );
}

/** The people page the shell's view switch mounts. */
export function PeoplePane() {
  const rows = usePeopleStore((s) => s.rows);
  const detail = usePeopleStore((s) => s.detail);
  const createTargets = usePeopleStore((s) => s.createTargets);
  const writeStates = usePeopleStore((s) => s.writeStates);
  const pendingCreate = usePeopleStore((s) => s.pendingContactCreate);
  const [editing, setEditing] = useState<'create' | 'edit' | null>(null);

  // Another surface's create request (the reading pane's sender chip)
  // opens the editor prefilled once a writable book stands.
  useEffect(() => {
    if (pendingCreate !== null && editing === null && createTargets.length > 0) {
      setEditing('create');
    }
  }, [pendingCreate, editing, createTargets.length]);

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
      className="relative flex min-w-0 flex-1 flex-col overflow-hidden"
    >
      <div className="flex h-[var(--pane-header-h)] shrink-0 items-center gap-2 border-b border-border-subtle bg-surface px-3">
        <div className="w-[220px]">
          <PeopleSearch />
        </div>
        <div className="flex-1" />
        {createTargets.length > 0 && (
          <button
            type="button"
            onClick={() => setEditing('create')}
            className="flex items-center gap-1 rounded-[var(--radius-xs)] bg-primary px-2 py-1 text-xs font-medium text-primary-foreground shadow-[var(--shadow-sm)] transition-colors hover:bg-primary-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <PlusIcon size={12} />
            New contact
          </button>
        )}
      </div>
      <div className="min-h-0 flex-1 p-2">
        <Group orientation="horizontal" className="h-full">
          <Panel id="people-list" defaultSize={pct(32)} minSize={pct(24)}>
            <PaneCard>
              <aside aria-label="People list" className="flex h-full min-w-0 flex-col">
                <WriteStatuses />
                <PeopleList
                  rows={rows}
                  selectedPerson={detail?.person ?? null}
                  onOpen={(person) => void dispatch(intents.openPerson(person))}
                />
              </aside>
            </PaneCard>
          </Panel>
          <Separator className="mx-1 w-1.5 rounded-full bg-border transition-colors hover:bg-primary" />
          <Panel id="person-detail" defaultSize={pct(68)} minSize={pct(40)}>
            <PaneCard>
              {detail === null ? (
                <PersonPlaceholder />
              ) : (
                <aside aria-label="Person detail" className="flex h-full min-w-0 flex-col">
                  <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-end border-b border-border-subtle px-2">
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
            </PaneCard>
          </Panel>
        </Group>
      </div>
      {editing !== null && (
        <ContactEditor
          mode={editing}
          createTargets={createTargets}
          editableCards={detail?.editable_cards ?? []}
          writeStates={writeStates}
          prefill={editing === 'create' ? pendingCreate : null}
          onClose={() => {
            setEditing(null);
            usePeopleStore.getState().clearContactCreate();
          }}
        />
      )}
    </section>
  );
}
