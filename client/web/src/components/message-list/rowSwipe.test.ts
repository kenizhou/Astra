// SPDX-License-Identifier: GPL-3.0-only
// The swipe action table: the direction-to-action mapping the settings
// configure, the intent each action dispatches on a row, the reverse the
// undo toast dispatches (expressible only where the sitting folder is known
// and the row names a message), and the release threshold's decision.
import { describe, expect, it } from 'vitest';

import {
  SWIPE_ACTIONS,
  SWIPE_FIRE_PX,
  swipeActionFor,
  swipeFires,
  swipeIntent,
  swipeReverse,
  swipeToastSentence,
  type SwipeAction,
} from './rowSwipe';
import { intents } from '@/services/ipc';
import type { MessageRowView, RowKey } from '@/services/surfaces';

function row(key: RowKey, over: Partial<MessageRowView> = {}): MessageRowView {
  return {
    key,
    account_id: 'work',
    sender_name: 'Ada',
    sender_address: 'ada@example.test',
    subject: 'Quarterly report',
    sent_at: '2026-10-09T08:30:00Z',
    unread: true,
    flagged: false,
    has_attachments: false,
    ...over,
  };
}

const messageRow = row({ message: { account: 'work', key: 'k1' } });
const threadRow = row({ thread: { account: 'work', thread_id: 'th-1' } });
const sittingFolder = { account: 'work', key: 'INBOX' };

/** The message half of the fixture row, narrowed for the reverse's asserts. */
function messageRef(view: MessageRowView) {
  if (!('message' in view.key)) {
    throw new Error('the fixture is not a message row');
  }
  return view.key.message;
}

describe('the swipe action vocabulary', () => {
  it('catalogues the four acts the intent surface serves', () => {
    expect(SWIPE_ACTIONS).toEqual([
      { id: 'archive', label: 'Archive' },
      { id: 'trash', label: 'Trash' },
      { id: 'read', label: 'Read' },
      { id: 'flag', label: 'Flag' },
    ]);
  });

  it('maps the directions onto the configured actions', () => {
    expect(swipeActionFor('right', 'archive', 'trash')).toBe('archive');
    expect(swipeActionFor('left', 'archive', 'trash')).toBe('trash');
    expect(swipeActionFor('right', 'flag', 'read')).toBe('flag');
    expect(swipeActionFor('left', 'flag', 'read')).toBe('read');
  });
});

describe('the swipe intents', () => {
  it('dispatches each action over the row', () => {
    expect(swipeIntent('archive', messageRow)).toEqual(intents.archiveMessage(messageRow.key));
    expect(swipeIntent('trash', messageRow)).toEqual(intents.moveToTrash(messageRow.key));
    expect(swipeIntent('read', messageRow)).toEqual(intents.markRead(messageRow.key, true));
    expect(swipeIntent('flag', messageRow)).toEqual(intents.setFlagged(messageRow.key, true));
  });

  it('flips the read and flag states off what the row carries', () => {
    const readRow = row(messageRow.key, { unread: false, flagged: true });
    expect(swipeIntent('read', readRow)).toEqual(intents.markRead(readRow.key, false));
    expect(swipeIntent('flag', readRow)).toEqual(intents.setFlagged(readRow.key, false));
  });
});

describe('the swipe reverses', () => {
  it('reverses a move by filing the message back into the sitting folder', () => {
    expect(swipeReverse('archive', messageRow, sittingFolder)).toEqual(
      intents.moveMessages([messageRef(messageRow)], sittingFolder),
    );
    expect(swipeReverse('trash', messageRow, sittingFolder)).toEqual(
      intents.moveMessages([messageRef(messageRow)], sittingFolder),
    );
  });

  it('names no reverse for a move the sitting folder cannot be named for', () => {
    expect(swipeReverse('archive', messageRow, null)).toBeNull();
    expect(swipeReverse('trash', threadRow, sittingFolder)).toBeNull();
  });

  it('reverses the state flips through their own flip', () => {
    expect(swipeReverse('read', messageRow, null)).toEqual(intents.markRead(messageRow.key, false));
    expect(swipeReverse('flag', messageRow, null)).toEqual(intents.setFlagged(messageRow.key, false));
  });
});

describe('the release decision', () => {
  it('fires only past the threshold and only horizontally dominant', () => {
    expect(swipeFires(SWIPE_FIRE_PX, 0)).toBe(true);
    expect(swipeFires(-SWIPE_FIRE_PX, 0)).toBe(true);
    expect(swipeFires(SWIPE_FIRE_PX - 1, 0)).toBe(false);
    expect(swipeFires(SWIPE_FIRE_PX, SWIPE_FIRE_PX + 10)).toBe(false);
    expect(swipeFires(SWIPE_FIRE_PX * 2, SWIPE_FIRE_PX)).toBe(true);
  });
});

describe('the toast sentences', () => {
  it('names each act in the past tense', () => {
    const sentences: Record<SwipeAction, string> = {
      archive: 'Archived',
      trash: 'Moved to Trash',
      read: 'Marked as read',
      flag: 'Flagged',
    };
    for (const action of Object.keys(sentences) as SwipeAction[]) {
      expect(swipeToastSentence(action)).toBe(sentences[action]);
    }
  });
});
