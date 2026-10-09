// SPDX-License-Identifier: GPL-3.0-only
// The reading pane's compose actions: Reply, Reply all, and Forward, each
// dispatching the compose family's intent for the message the pane has open.
// With nothing open the actions stand disabled: there is no message to
// compose from, and the core would have nothing to prefill.

import { dispatch, intents, type Intent, type MessageKey, type MessageRef } from '@/services/ipc';

interface ReadingActionsProps {
  /** The open message the actions compose from; null with nothing open. */
  message: MessageKey | null;
}

/** The verbs on offer, in display order, each with the intent it dispatches. */
const ACTIONS: ReadonlyArray<{ label: string; verb: (message: MessageRef) => Intent }> = [
  { label: 'Reply', verb: intents.replyTo },
  { label: 'Reply all', verb: intents.replyAllTo },
  { label: 'Forward', verb: intents.forward },
];

export function ReadingActions({ message }: ReadingActionsProps) {
  return (
    <div className="flex items-center gap-1">
      {ACTIONS.map((action) => (
        <button
          key={action.label}
          type="button"
          disabled={message === null}
          onClick={() => {
            if (message !== null) {
              void dispatch(action.verb(message));
            }
          }}
          className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-50"
        >
          {action.label}
        </button>
      ))}
    </div>
  );
}
