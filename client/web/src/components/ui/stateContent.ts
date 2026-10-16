// SPDX-License-Identifier: GPL-3.0-only
// The state family's content map: what each surface's empty state says, in
// one place, so a surface never re-derives its own copy. The demo pages
// reference this map; the components read it through EmptyState.

/** One surface's empty state: the message, and the action when one exists. */
export interface StateContent {
  message: string;
  actionLabel?: string;
}

/** The map, keyed by surface and moment. */
export const STATE_CONTENT: Record<string, StateContent> = {
  listNoFolder: { message: 'Select a folder to read its mail.' },
  listEmptyFolder: { message: 'No messages in this folder.' },
  listEmptyFilter: { message: 'Nothing matches this filter.' },
  searchEmpty: { message: 'No results.' },
  readingEmpty: { message: 'Select a message to read it.' },
};
