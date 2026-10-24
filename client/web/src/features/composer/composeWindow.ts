// SPDX-License-Identifier: GPL-3.0-only
// The pop-out composer window's mechanics: the URL that carries the
// composition id alone (plus, for a mailto window, the host-parsed
// prefill it opens from; the parse itself never crosses), the window
// label the capability wildcard matches, and the runtime WebviewWindow
// the main window opens per composition, kylins' composeWindow.ts idiom
// narrowed to the one thing the URL hands over. The New message route
// lives here too: a window under Tauri, the inline composer outside it,
// the same fallback kylins takes.

import { isTauri } from '@/events/surfaceEvents';
import { dispatch, intents, type CompositionId } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useSettingsStore } from '@/features/settings/settingsStore';

import {
  mintCompositionId,
  useComposerStore,
  type MailtoOpen,
} from './composerStore';
import { editorToDocument } from './documentWire';
import type { SharedFile } from './shareOpen';

/** The query parameter a composer window's URL carries its composition id in. */
const COMPOSER_PARAM = 'composer';

/** The query parameter a transfer marks the resume account in. */
const RESUME_PARAM = 'resume';

/** The query parameter a mailto window carries its prefill in, encoded. */
const PREFILL_PARAM = 'prefill';

/** The query parameter a share window carries its pending set in, encoded. */
const SHARE_PARAM = 'share';

/** The composition id a composer window's URL names; null for the main window. */
export function readComposerParam(search: string): CompositionId | null {
  const id = new URLSearchParams(search).get(COMPOSER_PARAM);
  return id === null || id === '' ? null : id;
}

/** The account a transfer window resumes the latest draft of; null for a
 *  plain pop-out. */
export function readResumeParam(search: string): string | null {
  const account = new URLSearchParams(search).get(RESUME_PARAM);
  return account === null || account === '' ? null : account;
}

/**
 * The prefill a mailto window opens from, decoded off its URL; null for a
 * plain pop-out and for anything the wire would not recognise, so a mangled
 * copy opens blank rather than half-seeded.
 */
export function readPrefillParam(search: string): MailtoOpen | null {
  const raw = new URLSearchParams(search).get(PREFILL_PARAM);
  if (raw === null || raw === '') {
    return null;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof parsed !== 'object' || parsed === null) {
    return null;
  }
  const fields = parsed as Record<string, unknown>;
  const strings = ['from', 'to', 'cc', 'bcc', 'subject', 'body'];
  if (!strings.every((field) => typeof fields[field] === 'string')) {
    return null;
  }
  return parsed as MailtoOpen;
}

/**
 * The pending share set a share window opens with, decoded off its URL;
 * null for a plain pop-out and for anything the wire would not recognise,
 * so a mangled copy opens blank rather than half-seeded.
 */
export function readShareParam(search: string): SharedFile[] | null {
  const raw = new URLSearchParams(search).get(SHARE_PARAM);
  if (raw === null || raw === '') {
    return null;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!Array.isArray(parsed)) {
    return null;
  }
  const files = parsed as unknown[];
  if (
    !files.every(
      (file) =>
        typeof file === 'object' &&
        file !== null &&
        typeof (file as Record<string, unknown>).name === 'string' &&
        typeof (file as Record<string, unknown>).path === 'string',
    )
  ) {
    return null;
  }
  return parsed as SharedFile[];
}

/**
 * The URL a composer window loads: the bundle with the composition id, the
 * resume marker when the dock's draft transfers, the prefill when a
 * mailto link opened the window, and the pending share set when a share
 * hand-off opened it.
 */
export function composerWindowUrl(
  composition: CompositionId,
  resumeAccount?: string,
  prefill?: MailtoOpen,
  shares?: SharedFile[],
): string {
  const base = `index.html?${COMPOSER_PARAM}=${encodeURIComponent(composition)}`;
  const withResume =
    resumeAccount === undefined
      ? base
      : `${base}&${RESUME_PARAM}=${encodeURIComponent(resumeAccount)}`;
  const withPrefill =
    prefill === undefined
      ? withResume
      : `${withResume}&${PREFILL_PARAM}=${encodeURIComponent(JSON.stringify(prefill))}`;
  return shares === undefined
    ? withPrefill
    : `${withPrefill}&${SHARE_PARAM}=${encodeURIComponent(JSON.stringify(shares))}`;
}

/** The window label a composition's pop-out takes, under the composer-* capability. */
export function composerWindowLabel(composition: CompositionId): string {
  return `composer-${composition}`;
}

/**
 * Opens the pop-out composer window for one minted composition: frameless
 * (the bundle draws its own title bar), centred, resizable down to the
 * composer's minimum. The window hydrates itself from the host; nothing but
 * the id crosses the URL, plus the resume marker for a dock-to-window
 * transfer and the prefill for a mailto open. A dock-to-window transfer
 * marks the resume account on the URL, so the window resumes the draft the
 * dock just saved rather than opening blank.
 */
export async function openComposerWindow(
  composition: CompositionId,
  title: string,
  resumeAccount?: string,
  prefill?: MailtoOpen,
  shares?: SharedFile[],
): Promise<void> {
  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  const webview = new WebviewWindow(composerWindowLabel(composition), {
    url: composerWindowUrl(composition, resumeAccount, prefill, shares),
    title,
    width: 720,
    height: 640,
    minWidth: 600,
    minHeight: 480,
    center: true,
    decorations: false,
    resizable: true,
  });
  await webview.once('tauri://error', (event) => {
    console.error('composer: the pop-out window failed to open', event);
  });
}

/**
 * The account a plain composition opens from: the default send account the
 * persisted preference names when it still stands, else the first account.
 * Undefined only when no account stands at all.
 */
export function defaultSendAccount(): { id: string; label: string } | undefined {
  const accounts = useAccountStore.getState().accounts;
  const preferred = useSettingsStore.getState().settings.default_send_account;
  const pinned =
    preferred !== undefined && accounts.some((account) => account.accountId === preferred)
      ? accounts.find((account) => account.accountId === preferred)
      : accounts[0];
  return pinned === undefined ? undefined : { id: pinned.accountId, label: pinned.displayName };
}

/**
 * The New message route: a pop-out window under Tauri, the inline composer
 * outside it (a plain browser holds no windows). The window hydrates blank;
 * only the minted id crosses.
 */
export function openNewMessageComposer(): Promise<void> {
  const account = defaultSendAccount();
  if (account === undefined) {
    return Promise.resolve();
  }
  if (!isTauri) {
    useComposerStore.getState().newMessage(account);
    return Promise.resolve();
  }
  const composition = mintCompositionId();
  return openComposerWindow(composition, 'New message');
}

/**
 * The dock-to-window transfer: the draft saves (and closes host-side), the
 * dock closes locally, and the composition's own window opens with the
 * account marked for its resume. The save settles before the window opens,
 * so the window's resume never races the words it reads back.
 */
export async function popOutComposer(): Promise<void> {
  const state = useComposerStore.getState();
  if (state.composition === null) {
    return;
  }
  await dispatch(
    intents.saveDraft({
      composition: state.composition,
      from: state.from,
      to: state.to,
      cc: state.cc,
      bcc: state.bcc,
      subject: state.subject,
      document: editorToDocument(state.editor),
      then_close: true,
    }),
  );
  const composition = mintCompositionId();
  const title = state.subject === '' ? 'New message' : state.subject;
  useComposerStore.getState().closePane();
  await openComposerWindow(composition, title, state.from);
}
