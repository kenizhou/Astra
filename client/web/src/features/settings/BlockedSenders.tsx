// SPDX-License-Identifier: GPL-3.0-only
// The blocked-senders management section: lists the blocklist the settings
// block carries, each entry with an Unblock that dispatches the unblock
// intent. The list reads the settings wire (the persisted preference), never
// a local copy, so a write's republished snapshot repaints it. Suppression
// only: the section says plainly that blocked mail still arrives in the Inbox.

import { dispatch, intents } from '@/services/ipc';

import { useSettingsStore } from './settingsStore';

/** The blocked-senders section of the General settings page. */
export function BlockedSenders() {
  // The selector returns the stored reference (or undefined); the empty
  // default lives outside it so a absent field never mints a fresh array
  // per snapshot read.
  const blocked = useSettingsStore((s) => s.settings.blocked_senders) ?? [];

  return (
    <section aria-label="Blocked senders" className="flex flex-col gap-1">
      <h3 className="pt-2 text-xs font-semibold text-muted-foreground">Blocked senders</h3>
      <p className="text-xs text-muted-foreground">
        Their mail no longer raises new-mail notifications. It still arrives in your Inbox.
      </p>
      {blocked.length === 0 ? (
        <p className="text-xs text-muted-text">No blocked senders.</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {blocked.map((address) => (
            <li
              key={address}
              className="flex items-center justify-between gap-2 rounded-[var(--radius-xs)] border border-border-subtle px-2 py-1"
            >
              <span className="truncate text-xs text-foreground" title={address}>
                {address}
              </span>
              <button
                type="button"
                aria-label={`Unblock ${address}`}
                onClick={() => void dispatch(intents.unblockSender(address))}
                className="shrink-0 rounded-[var(--radius-xs)] border border-border px-2 py-0.5 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                Unblock
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}