// SPDX-License-Identifier: GPL-3.0-only
// The reminder dialog: one message reminded about at a wake instant, the
// picker offering later today / tomorrow / next week as computed instants
// (the snooze preset math reused). Choosing one fires the host's remind
// command (the core records the provider key, subject, and sender; the
// message never moves or flags) and closes; the wake timer notifies at the
// instant. The row menu's Remind me entry opens it through the reminder
// store, the snooze dialog's own shape.

import { useDialogFocus } from '@/components/ui/useDialogFocus';
import type { MessageRef } from '@/services/ipc';
import { remindMessage } from '@/services/reminders';
import { snoozeChoices } from '@/services/snooze';

import { useReminderStore } from './reminderStore';

/** The store gate: the dialog stands while a message is named, keyed by it
 *  so every open starts with its own focus move. */
export function ReminderDialog() {
  const target = useReminderStore((s) => s.target);
  if (target === null) {
    return null;
  }
  return <ReminderDialogBody key={`${target.account}/${target.key}`} target={target} />;
}

/** The modal over one message's wake choices. */
function ReminderDialogBody({ target }: { target: MessageRef }) {
  const dialogRef = useDialogFocus<HTMLDivElement>();
  const choices = snoozeChoices(new Date());

  const close = (): void => useReminderStore.getState().close();
  const confirm = (wakeAt: number): void => {
    void remindMessage(target, wakeAt);
    close();
  };

  const choice =
    'rounded-[var(--radius)] px-2.5 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)] border border-border-subtle text-muted-foreground hover:bg-hover hover:text-foreground';
  const footer =
    'rounded-[var(--radius)] px-2.5 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]';

  return (
    <div
      className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)] p-4"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          close();
        }
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Remind me"
        ref={dialogRef}
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.stopPropagation();
            close();
          }
        }}
        className="flex max-h-full w-[420px] flex-col overflow-y-auto rounded-xl border border-border-subtle bg-card p-5 shadow-[var(--shadow-lg)] focus-visible:outline-none"
      >
        <h2 className="mb-3 text-sm font-semibold text-foreground">Remind me</h2>
        <p className="text-sm text-foreground">Remind me about this message at:</p>

        <div role="group" aria-label="Wake time" className="mt-3 flex flex-col gap-1.5">
          {choices.map((entry) => (
            <button
              key={entry.label}
              type="button"
              onClick={() => confirm(entry.wakeAt)}
              className={choice}
            >
              {entry.label}
            </button>
          ))}
        </div>

        <div className="mt-4 flex justify-end gap-2 border-t border-border-subtle pt-3">
          <button
            type="button"
            onClick={close}
            className={`${footer} border border-border-subtle text-muted-foreground hover:bg-hover hover:text-foreground`}
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
