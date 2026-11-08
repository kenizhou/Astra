// SPDX-License-Identifier: GPL-3.0-only
// The platform half of the IPC wire: the default-mail registration verb
// and its ask-once latch. The latch lives host-side in the app data dir
// (the ask-once discipline is the host's, not a core preference), and the
// asking surface reads it here and marks it spent on either answer. The
// outcome crosses as the snake_case family the host's platform tests pin.

import { invoke } from '@tauri-apps/api/core';

/** The typed answer to the default-mail ask, as the surface reads it. */
export type RegisterDefaultMailOutcome =
  /** The registration landed: this app is the OS's mailto handler. */
  | 'registered'
  /** The registration already stood: no OS write was made. */
  | 'already_default'
  /** The registration refused; the sentence names why, ready to show. */
  | { failed: { reason: string } };

/**
 * Registers the app as the OS's default mail handler, idempotently: a
 * registration that already stands answers `already_default` without
 * touching the OS.
 */
export function registerDefaultMail(): Promise<RegisterDefaultMailOutcome> {
  return invoke('register_default_mail');
}

/** Whether the default-mail offer has ever been asked, the latch's read. */
export function defaultMailAsked(): Promise<boolean> {
  return invoke('default_mail_asked');
}

/** Marks the default-mail offer asked, the once-latch's write. */
export function markDefaultMailAsked(): Promise<void> {
  return invoke('default_mail_mark_asked');
}
