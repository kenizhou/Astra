// SPDX-License-Identifier: GPL-3.0-only
// The message-write wire literals, pinned the way ipc.test.ts pins the row
// verbs: the delete crosses as exactly the bytes the host's serde reads.
import { describe, expect, it } from 'vitest';

import { writeIntents } from './writes';

describe('writeIntents', () => {
  it('carries the row reference with the permanent delete', () => {
    const message = { message: { account: 'work', key: 'k1' } };
    const thread = { thread: { account: 'work', thread_id: 'th-7' } };
    expect(JSON.stringify(writeIntents.deleteMessage(message))).toBe(
      '{"delete_message":{"row":{"message":{"account":"work","key":"k1"}}}}',
    );
    expect(JSON.stringify(writeIntents.deleteMessage(thread))).toBe(
      '{"delete_message":{"row":{"thread":{"account":"work","thread_id":"th-7"}}}}',
    );
  });

  it('carries the message reference with the stored draft discard', () => {
    expect(
      JSON.stringify(writeIntents.discardStored({ account: 'work', key: 'd-1' })),
    ).toBe('{"drafts":{"discard_stored":{"message":{"account":"work","key":"d-1"}}}}');
  });
});
