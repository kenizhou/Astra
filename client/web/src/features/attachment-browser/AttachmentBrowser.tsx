// SPDX-License-Identifier: GPL-3.0-only
// The attachment browser: a transient modal listing attachments across mail,
// grouped by sender (newest first, the count and total size beside each),
// filtered by mime chips derived from a mime-unfiltered snapshot so an active
// filter never collapses them. Save As reuses the reading pane's attachment
// service; opening the source message dispatches the existing open intent and
// closes the modal; Preview stands as a placeholder until its slice arrives.
// The coverage footer states the scan window honestly and Refresh re-pulls.
// Escape and the backdrop close, the focus moves in on open and back on close,
// and the surface mounts fresh on every open. Not a fourth view: the shell's
// rail and ShellView are untouched.

import { useCallback, useEffect, useState } from 'react';

import { useDialogFocus } from '@/components/ui/useDialogFocus';
import {
  browseAttachments,
  saveAttachment,
  type AttachmentBrowserEntry,
  type AttachmentBrowserView,
} from '@/services/attachments';
import { dispatch, intents } from '@/services/ipc';

import { closeAttachmentBrowser, useAttachmentBrowserStore } from './attachmentBrowserStore';
import { formatSize, groupBySender, mimeChips } from './grouping';

const CHIP_CLASS =
  'shrink-0 rounded-full border border-border-subtle px-2.5 py-0.5 text-[11px] text-muted-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';
const CHIP_ACTIVE_CLASS =
  'shrink-0 rounded-full border border-border-subtle bg-hover px-2.5 py-0.5 text-[11px] font-medium text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';
const ROW_BUTTON_CLASS =
  'shrink-0 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-40';

/** The overlay, mounted in the shell's tail beside the palette; null while closed. */
export function AttachmentBrowser() {
  const isOpen = useAttachmentBrowserStore((state) => state.isOpen);
  if (!isOpen) {
    return null;
  }
  return <BrowserSurface />;
}

/** The open surface: a dialog over the grouped, filtered rows. */
function BrowserSurface() {
  const sender = useAttachmentBrowserStore((state) => state.sender);
  const dialogRef = useDialogFocus<HTMLDivElement>();
  const [view, setView] = useState<AttachmentBrowserView | null>(null);
  const [mimePrefix, setMimePrefix] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [refresh, setRefresh] = useState(0);

  // One pull per open, per sender, and per Refresh: the mime filter narrows
  // client-side over the mime-unfiltered answer, so the chips never collapse.
  const load = useCallback(() => {
    let alive = true;
    browseAttachments({ sender, mime_prefix: null })
      .then((answer) => {
        if (alive) {
          setView(answer);
        }
      })
      .catch(() => {
        if (alive) {
          setView({ entries: [], coverage: { scanned: 0, indexed: 0, skipped: 0, failed_accounts: [], scan_limit: 200 } });
        }
      });
    return () => {
      alive = false;
    };
  }, [sender, refresh]);

  useEffect(() => load(), [load]);

  const entries = view?.entries ?? [];
  const visible =
    mimePrefix === null
      ? entries
      : entries.filter((entry) => entry.media_type.toLowerCase().startsWith(mimePrefix.toLowerCase()));
  const groups = groupBySender(visible);
  const chips = mimeChips(entries);
  const coverage = view?.coverage;

  const save = (entry: AttachmentBrowserEntry): void => {
    void saveAttachment(entry.source, entry.id).then((outcome) => {
      if (outcome === 'cancelled') {
        return;
      }
      setNote('saved' in outcome ? `Saved to ${outcome.saved.path}` : outcome.failed.reason);
    });
  };

  const openSource = (entry: AttachmentBrowserEntry): void => {
    void dispatch(intents.openMessage(entry.source));
    closeAttachmentBrowser();
  };

  return (
    <div
      className="fixed inset-0 z-[var(--z-modal)] flex items-start justify-center bg-[var(--backdrop)] p-4 pt-[10vh]"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          closeAttachmentBrowser();
        }
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label="Attachment browser"
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.preventDefault();
            closeAttachmentBrowser();
          }
        }}
        className="flex max-h-[80vh] w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-border-subtle bg-surface-floating shadow-[var(--shadow-lg)] outline-none"
      >
        <div className="border-b border-border-subtle px-5 py-3">
          <h2 className="text-base font-semibold text-foreground">Attachment browser</h2>
          {sender !== null && (
            <p className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
              <span className="min-w-0 truncate">From {sender}</span>
              <button
                type="button"
                onClick={() => useAttachmentBrowserStore.getState().clearSender()}
                className={ROW_BUTTON_CLASS}
              >
                Clear
              </button>
            </p>
          )}
          <div className="mt-2 flex flex-wrap gap-1.5" role="group" aria-label="Filter by type">
            {chips.map((chip) => (
              <button
                key={chip.label}
                type="button"
                aria-pressed={mimePrefix === chip.prefix}
                onClick={() => setMimePrefix(chip.prefix)}
                className={mimePrefix === chip.prefix ? CHIP_ACTIVE_CLASS : CHIP_CLASS}
              >
                {chip.label} ({chip.count})
              </button>
            ))}
          </div>
        </div>

        {note !== null && (
          <p role="status" className="border-b border-border-subtle px-5 py-1.5 text-xs text-muted-foreground">
            {note}
          </p>
        )}

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-3">
          {view === null ? (
            <p className="py-6 text-center text-xs text-muted-foreground">Loading attachments…</p>
          ) : visible.length === 0 ? (
            <p className="py-6 text-center text-xs text-muted-foreground">
              No attachments to show. Open messages so their sources land in the cache, then refresh.
            </p>
          ) : (
            groups.map((group) => (
              <section key={group.address} aria-label={group.name} className="mb-4 last:mb-0">
                <h3 className="mb-1.5 flex items-baseline gap-2 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
                  <span className="min-w-0 truncate">{group.name}</span>
                  <span className="shrink-0 normal-case">
                    {group.count} {group.count === 1 ? 'attachment' : 'attachments'} · {formatSize(group.totalSize)}
                  </span>
                </h3>
                <ul className="space-y-0.5">
                  {group.entries.map((entry) => (
                    <li
                      key={`${entry.source.account}:${entry.source.key}:${entry.id}`}
                      className="flex items-center gap-2 rounded-md px-2 py-1 text-[13px] text-foreground"
                    >
                      <span className="min-w-0 flex-1 truncate" title={entry.subject}>
                        {entry.file_name}
                      </span>
                      <span className="shrink-0 text-xs text-muted-foreground">{formatSize(entry.size)}</span>
                      <button
                        type="button"
                        aria-label={`Preview ${entry.file_name}`}
                        title="Attachment previews arrive with a future slice"
                        disabled
                        className={ROW_BUTTON_CLASS}
                      >
                        Preview
                      </button>
                      <button
                        type="button"
                        aria-label={`Save As ${entry.file_name}`}
                        onClick={() => save(entry)}
                        className={ROW_BUTTON_CLASS}
                      >
                        Save As…
                      </button>
                      <button
                        type="button"
                        aria-label={`Open message ${entry.subject}`}
                        onClick={() => openSource(entry)}
                        className={ROW_BUTTON_CLASS}
                      >
                        Open message
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            ))
          )}
        </div>

        <div className="flex items-center justify-between gap-3 border-t border-border-subtle px-5 py-2 text-[11px] text-muted-foreground">
          <p className="min-w-0 flex-1">
            {coverage === undefined
              ? 'Scanning…'
              : `Indexed from ${coverage.indexed} cached ${coverage.indexed === 1 ? 'message' : 'messages'}, ${coverage.skipped} not cached, up to ${coverage.scan_limit} scanned per account${coverage.failed_accounts.length > 0 ? `, ${coverage.failed_accounts.length} ${coverage.failed_accounts.length === 1 ? 'account' : 'accounts'} failed (${coverage.failed_accounts.join(', ')})` : ''}.`}
          </p>
          <button type="button" onClick={() => setRefresh((tick) => tick + 1)} className={ROW_BUTTON_CLASS}>
            Refresh
          </button>
        </div>
      </div>
    </div>
  );
}
