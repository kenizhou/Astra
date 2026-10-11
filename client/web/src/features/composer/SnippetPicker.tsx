// SPDX-License-Identifier: GPL-3.0-only
// The snippet picker: the toolbar affordance whose panel lists the snippet
// library in the host's recent-first order (pulled once and kept across
// reopenings), a search box that crosses its query to the ranked search
// once it settles (debounced) and returns to the plain listing when it
// clears, and rows that hand their body to the insert seam and close the
// panel. Placing the body at the editor's current selection is the
// caller's discipline, the insert seam it is handed.

import { useEffect, useRef, useState } from 'react';

import { snippetList, snippetSearch, type SnippetView } from '@/services/ai';

/** How long the search box must stay quiet before its query crosses. */
const SEARCH_DEBOUNCE_MS = 300;

interface SnippetPickerProps {
  /** Places one snippet's body at the editor's current selection. */
  onInsert: (body: string) => void;
}

export function SnippetPicker({ onInsert }: SnippetPickerProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  // The plain library the open pull fetched, kept for the next opening and
  // for a cleared query; the rows the panel shows right now.
  const [listed, setListed] = useState<SnippetView[] | null>(null);
  const [found, setFound] = useState<SnippetView[] | null>(null);
  const searchEpoch = useRef(0);

  // The library pulls once per mount, however often the panel reopens.
  useEffect(() => {
    if (!open || listed !== null) {
      return;
    }
    let live = true;
    snippetList().then(
      (rows) => {
        if (live) {
          setListed(rows);
        }
      },
      (err: unknown) => {
        console.error('compose: the snippet listing failed', err);
        if (live) {
          setListed([]);
        }
      },
    );
    return () => {
      live = false;
    };
  }, [open, listed]);

  // The query crosses once it settles; a cleared query drops back to the
  // plain listing without a call.
  useEffect(() => {
    if (!open) {
      return;
    }
    const words = query.trim();
    if (words === '') {
      searchEpoch.current += 1;
      setFound(null);
      return;
    }
    const timer = window.setTimeout(() => {
      const run = ++searchEpoch.current;
      snippetSearch(words).then(
        (rows) => {
          if (searchEpoch.current === run) {
            setFound(rows);
          }
        },
        (err: unknown) => {
          console.error('compose: the snippet search failed', err);
          if (searchEpoch.current === run) {
            setFound([]);
          }
        },
      );
    }, SEARCH_DEBOUNCE_MS);
    return () => {
      window.clearTimeout(timer);
    };
  }, [open, query]);

  const close = (): void => {
    setOpen(false);
    setQuery('');
    setFound(null);
  };

  // While a search is still in flight the plain listing keeps the panel
  // company; the note waits until the rows it describes have settled.
  const words = query.trim();
  const rows = words === '' ? (listed ?? []) : (found ?? listed ?? []);
  const settled = words === '' ? listed !== null : found !== null;
  const emptyNote = words === '' ? 'No snippets yet.' : 'No snippets matched.';

  return (
    <div className="relative">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => (open ? close() : setOpen(true))}
        className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        Snippets
      </button>
      {open && (
        <section
          aria-label="Snippet picker"
          className="absolute right-0 top-full z-10 mt-1 flex w-72 flex-col gap-1.5 rounded-[var(--radius-sm)] border border-border bg-surface p-2 shadow-lg"
        >
          <input
            type="text"
            aria-label="Search snippets"
            placeholder="Search snippets"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            className="w-full rounded-[var(--radius-xs)] border border-border bg-background px-2 py-1 text-xs text-foreground outline-none transition-colors focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring"
          />
          <div className="flex max-h-64 flex-col gap-0.5 overflow-y-auto">
            {rows.length === 0 && settled && (
              <p className="px-1 py-2 text-xs text-muted-foreground">{emptyNote}</p>
            )}
            {rows.map((snippet) => (
              <button
                key={snippet.id}
                type="button"
                onClick={() => {
                  onInsert(snippet.body);
                  close();
                }}
                className="flex flex-col items-start rounded-[var(--radius-xs)] px-1.5 py-1 text-left transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                <span className="text-xs font-medium text-foreground">{snippet.title}</span>
                <span className="line-clamp-1 text-xs text-muted-foreground">{snippet.body}</span>
              </button>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
