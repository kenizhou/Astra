// SPDX-License-Identifier: GPL-3.0-only
// The report wire, pinned against the app protocol's serde: both spam-report
// verbs cross under their own tags carrying the row, message and thread
// shapes alike, exactly the bytes astra-app's protocol_report_tests.rs
// asserts.
import { describe, expect, it } from 'vitest';

import { intents } from '@/services/ipc';

describe('the report intent family', () => {
  it('carries the row under the mark_junk tag', () => {
    expect(
      JSON.stringify(intents.markJunk({ message: { account: 'work', key: 'imap:v1:u1' } })),
    ).toBe('{"mark_junk":{"row":{"message":{"account":"work","key":"imap:v1:u1"}}}}');
    expect(
      JSON.stringify(intents.markJunk({ thread: { account: 'work', thread_id: 'th-7' } })),
    ).toBe('{"mark_junk":{"row":{"thread":{"account":"work","thread_id":"th-7"}}}}');
  });

  it('carries the row under the mark_not_junk tag', () => {
    expect(
      JSON.stringify(intents.markNotJunk({ message: { account: 'work', key: 'jmap:e2' } })),
    ).toBe('{"mark_not_junk":{"row":{"message":{"account":"work","key":"jmap:e2"}}}}');
    expect(
      JSON.stringify(intents.markNotJunk({ thread: { account: 'work', thread_id: 'th-7' } })),
    ).toBe('{"mark_not_junk":{"row":{"thread":{"account":"work","thread_id":"th-7"}}}}');
  });
});
