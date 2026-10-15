// SPDX-License-Identifier: GPL-3.0-only
// One recipient field of the composer's header: the finished recipients as
// pills with per-recipient removal by position (a duplicated address goes
// one occurrence at a time), over the one comma-separated string the DOM
// input carries whole (pills are a view of it, never a second source of
// truth), the completion on the trailing token with its floating list the
// keyboard drives (the arrows move and wrap the active suggestion, Enter
// accepts it, Escape closes the list) beside the mouse, and the settled
// flag that states a seeded field has nothing in progress.

import { useEffect, useId, useRef, useState } from 'react';

import { peopleSuggestions, type RecipientSuggestion } from '@/services/ipc';

import {
  finishedRecipients,
  insertBareAddress,
  removeRecipientAt,
  trailingToken,
} from './recipientTokens';

/** How long the token sits quiet before the suggestions pull crosses. */
export const SUGGESTIONS_DEBOUNCE_MS = 150;

interface RecipientFieldProps {
  /** The field's name, as its input's label and the pills' group name. */
  label: string;
  /** The one comma-separated string: the field's whole value. */
  value: string;
  /** Carries the field's new whole value, named as the wire names it. */
  onField: (value: string) => void;
  /** The host ref to the field's input, for the pane's caret placement. */
  inputRef?: React.RefObject<HTMLInputElement | null>;
}

export function RecipientField({ label, value, onField, inputRef }: RecipientFieldProps) {
  // A seeded field has nothing in progress; the first user edit moves the
  // field into the typing split, where the trailing token is the work.
  const [settled, setSettled] = useState(true);
  const [focused, setFocused] = useState(false);
  const [suggestions, setSuggestions] = useState<RecipientSuggestion[]>([]);
  // The list's active option, by position; -1 states none stands.
  const [active, setActive] = useState(-1);
  const ownRef = useRef<HTMLInputElement | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Every ask and every close moves the epoch, so a resolve that lands
  // after a newer token's ask or after the blur is dropped, never stored.
  const epoch = useRef(0);
  const listId = useId();

  useEffect(() => {
    return () => {
      if (timer.current !== null) {
        clearTimeout(timer.current);
      }
    };
  }, []);

  // A list that lands starts with nothing active.
  useEffect(() => {
    setActive(-1);
  }, [suggestions]);

  const pills = finishedRecipients(value, settled);

  /** Disarms the standing ask and outdates any resolve still in flight. */
  const retire = (): void => {
    epoch.current += 1;
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
  };

  const armAsk = (next: string): void => {
    retire();
    // The ask carries the token the completion works on: the separator's
    // spacing is not the user's typing, so it drops off here and nowhere
    // else.
    const ask = trailingToken(next).trim();
    if (ask === '') {
      setSuggestions([]);
      return;
    }
    const at = epoch.current;
    timer.current = setTimeout(() => {
      timer.current = null;
      void peopleSuggestions(ask).then((rows) => {
        if (at === epoch.current) {
          setSuggestions(rows);
        }
      }).catch(() => {
        if (at === epoch.current) {
          setSuggestions([]);
        }
      });
    }, SUGGESTIONS_DEBOUNCE_MS);
  };

  /** Shuts the floating list: the ask disarms and the rows drop. */
  const closeList = (): void => {
    retire();
    setSuggestions([]);
  };

  const onChange = (event: React.ChangeEvent<HTMLInputElement>): void => {
    setSettled(false);
    onField(event.target.value);
    armAsk(event.target.value);
  };

  const onBlur = (): void => {
    setFocused(false);
    closeList();
  };

  const accept = (address: string): void => {
    const next = insertBareAddress(value, address);
    setSettled(false);
    onField(next);
    closeList();
    // The caret goes to the end after the programmatic change, once the
    // value has landed in the input.
    requestAnimationFrame(() => {
      const input = ownRef.current;
      if (input !== null) {
        input.focus();
        input.setSelectionRange(input.value.length, input.value.length);
      }
    });
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>): void => {
    if (suggestions.length === 0) {
      return;
    }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const down = event.key === 'ArrowDown';
      setActive((current) => {
        if (current < 0) {
          return down ? 0 : suggestions.length - 1;
        }
        return (current + (down ? 1 : -1) + suggestions.length) % suggestions.length;
      });
      return;
    }
    if (event.key === 'Enter' && active >= 0) {
      event.preventDefault();
      accept(suggestions[active]!.email);
      return;
    }
    if (event.key === 'Escape') {
      closeList();
    }
  };

  const listOpen = focused && suggestions.length > 0;

  return (
    <div className="relative min-w-0 flex-1">
      <div className="rounded-xl border border-border bg-background transition-colors focus-within:border-primary">
        {pills.length > 0 && (
          <div
            role="group"
            aria-label={`${label} recipients`}
            className="flex flex-wrap gap-1 px-1.5 pt-1.5"
          >
            {pills.map((address, at) => (
              <span
                key={`${address}:${at}`}
                className="flex items-center gap-1 rounded-[var(--radius-xs)] bg-muted pl-1.5 text-[11px] text-foreground"
              >
                <span aria-hidden="true" className="max-w-[180px] truncate">
                  {address}
                </span>
                <button
                  type="button"
                  aria-label={`Remove ${address}`}
                  onClick={() => onField(removeRecipientAt(value, at, settled))}
                  className="rounded-[var(--radius-xs)] px-1 text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        )}
        <input
          ref={(node) => {
            ownRef.current = node;
            if (inputRef !== undefined) {
              inputRef.current = node;
            }
          }}
          type="text"
          aria-label={label}
          aria-autocomplete="list"
          aria-controls={listOpen ? listId : undefined}
          aria-activedescendant={listOpen && active >= 0 ? `${listId}-option-${active}` : undefined}
          value={value}
          onChange={onChange}
          onFocus={() => {
            setFocused(true);
            // A refocus never resurrects rows a blurred resolve left: the
            // list waits for the next keystroke's own ask.
            closeList();
          }}
          onBlur={onBlur}
          onKeyDown={onKeyDown}
          className="w-full bg-transparent px-2 py-1 text-[13px] text-foreground outline-none placeholder:text-muted-foreground"
        />
      </div>
      {listOpen && (
        <ul
          id={listId}
          role="listbox"
          aria-label={`${label} suggestions`}
          className="absolute left-0 right-0 top-full z-[var(--z-modal)] mt-1 max-h-56 overflow-y-auto rounded-[var(--radius)] border border-border bg-surface-floating p-1 shadow-[var(--shadow-lg)]"
        >
          {suggestions.map((row, at) => (
            // The option never takes focus: the press is a mouse-down the
            // input keeps, so the keystrokes the list completes do not stop.
            <li
              key={row.email}
              id={`${listId}-option-${at}`}
              role="option"
              aria-selected={active === at}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => accept(row.email)}
              className={`flex cursor-pointer items-baseline justify-between gap-2 rounded-[var(--radius-xs)] px-2 py-1 text-[13px] text-foreground transition-colors hover:bg-hover${
                active === at ? ' bg-hover' : ''
              }`}
            >
              <span className="flex min-w-0 flex-col">
                <span className="truncate">{row.display_name}</span>
                <span className="truncate text-xs text-muted-foreground">{row.email}</span>
              </span>
              {!row.is_saved && (
                <span className="shrink-0 rounded-[var(--radius-xs)] bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                  From sent mail
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
