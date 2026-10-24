// SPDX-License-Identifier: GPL-3.0-only
// The report half of the IPC wire: the two spam-report intents' constructors.
// Split from ipc.ts to keep that file under the line cap, like the folder and
// sending halves before it, and re-exported whole through it, so ipc stays
// the one import point of the wire. The literals are pinned in reports.test.ts
// against the JSON astra-app's protocol_report_tests.rs asserts.

import type { RowRef } from './ipc';

/** The report family's constructors, spread into ipc's `intents` in the
 *  enum's own order. */
export const reportIntents = {
  /** Report one row (a message, or a whole conversation) as spam: the Junk
   *  verdict and the filing into the account's Junk folder in one report. */
  markJunk: (row: RowRef) => ({ mark_junk: { row } }),
  /** Report one row as legitimate: the NotJunk verdict, filed back into the
   *  account's Inbox. */
  markNotJunk: (row: RowRef) => ({ mark_not_junk: { row } }),
};
