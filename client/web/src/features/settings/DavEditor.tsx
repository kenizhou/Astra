// SPDX-License-Identifier: GPL-3.0-only
// The dav section editor: the form the account page holds for one
// account's calendar and contacts section. The masked row's values seed
// it (the credential never crosses back, so the password field starts
// blank and says blank keeps), the save states only the fields the form
// changed plus the password only when one is typed, and a refused save is
// what keeps the form open: the close waits for the publish the dispatch
// started, and the failure sentence renders under the form the user is
// still looking at.

import { useEffect, useState } from 'react';

import { dispatch, intents } from '@/services/ipc';
import type { AccountSettingsRow, DavSectionInput } from '@/services/ipc';

import { useSettingsStore } from './settingsStore';

/** The form's fields, flat text. */
interface DavForm {
  caldav: string;
  carddav: string;
  username: string;
  password: string;
}

/** The form the masked row seeds: every echo it carried, never a password. */
function formOfRow(row: AccountSettingsRow): DavForm {
  return {
    caldav: row.dav?.caldav_url ?? '',
    carddav: row.dav?.carddav_url ?? '',
    username: row.dav?.username ?? '',
    password: '',
  };
}

/**
 * The section the save states: each field the form changed against its
 * seed, and the password only when one was typed. A field the form did
 * not move crosses no key at all, exactly as the wire's omission
 * discipline spells.
 */
function sectionOf(form: DavForm, baseline: DavForm): DavSectionInput | null {
  const section: DavSectionInput = {};
  if (form.caldav !== baseline.caldav) {
    section.caldav_url = form.caldav;
  }
  if (form.carddav !== baseline.carddav) {
    section.carddav_url = form.carddav;
  }
  if (form.username !== baseline.username) {
    section.username = form.username;
  }
  if (form.password !== '') {
    section.password = form.password;
  }
  return Object.keys(section).length > 0 ? section : null;
}

const fieldClass =
  'h-7 w-full rounded-full border border-input bg-background px-3 text-xs text-foreground outline-none transition-colors focus:border-primary';

interface DavEditorProps {
  /** The account whose section the editor writes. */
  account: string;
  /** Closes the editor: the account page's own back to the list. */
  onClose: () => void;
}

/** The dav section editor for one account. */
export function DavEditor({ account, onClose }: DavEditorProps) {
  const rows = useSettingsStore((s) => s.accounts);
  const revision = useSettingsStore((s) => s.revision);
  const row = rows.find((candidate) => candidate.account === account);
  const [form, setForm] = useState<DavForm>(() => formOfRow(row ?? {
    account,
    address: '',
    kind: 'imap_smtp',
    legs: { calendar: false, contacts: false },
  }));
  /** The revision the save dispatched against; null while none is pending. */
  const [watch, setWatch] = useState<number | null>(null);

  // The close waits for the publish the save started: the revision moves
  // on every applied snapshot, and the row the fresh publish carries
  // decides, the failure sentence standing under the form until a later
  // save clears it.
  useEffect(() => {
    if (watch === null || revision === watch) {
      return;
    }
    setWatch(null);
    const fresh = useSettingsStore
      .getState()
      .accounts.find((candidate) => candidate.account === account);
    if (fresh?.dav_error !== undefined) {
      return;
    }
    onClose();
  }, [revision, watch, account, onClose]);

  if (row === undefined) {
    return null;
  }

  const baseline = formOfRow(row);
  const section = sectionOf(form, baseline);
  const set = (field: keyof DavForm, value: string): void =>
    setForm((current) => ({ ...current, [field]: value }));

  const save = (): void => {
    if (section === null) {
      return;
    }
    void dispatch(intents.setDav(account, section));
    setWatch(useSettingsStore.getState().revision);
  };

  const clear = (): void => {
    void dispatch(intents.setDav(account, null));
    setWatch(useSettingsStore.getState().revision);
  };

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        save();
      }}
      className="flex flex-col gap-3"
    >
      <h3 className="text-xs font-medium text-foreground">Calendar and contacts</h3>
      <label className="flex flex-col gap-1 text-xs text-muted-foreground">
        CalDAV URL
        <input
          aria-label="CalDAV URL"
          value={form.caldav}
          onChange={(event) => set('caldav', event.target.value)}
          placeholder="https://"
          className={fieldClass}
        />
      </label>
      <label className="flex flex-col gap-1 text-xs text-muted-foreground">
        CardDAV URL
        <input
          aria-label="CardDAV URL"
          value={form.carddav}
          onChange={(event) => set('carddav', event.target.value)}
          placeholder="Leave empty to use the CalDAV origin"
          className={fieldClass}
        />
      </label>
      <label className="flex flex-col gap-1 text-xs text-muted-foreground">
        Username
        <input
          aria-label="Username"
          value={form.username}
          onChange={(event) => set('username', event.target.value)}
          className={fieldClass}
        />
      </label>
      <label className="flex flex-col gap-1 text-xs text-muted-foreground">
        Password
        <input
          aria-label="Password"
          type="password"
          value={form.password}
          onChange={(event) => set('password', event.target.value)}
          placeholder={
            row.dav?.dav_password_set
              ? 'Leave blank to keep the stored password'
              : 'Password'
          }
          className={fieldClass}
        />
      </label>

      {row.dav_error !== undefined && (
        <p role="alert" className="text-xs text-[var(--warning)]">
          {row.dav_error}
        </p>
      )}

      <div className="flex items-center justify-between gap-2 border-t border-border pt-3">
        <button
          type="button"
          aria-label="Clear dav section"
          onClick={clear}
          className="rounded-[var(--radius)] border border-[var(--destructive)] px-2.5 py-1 text-xs text-destructive transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
        >
          Clear section
        </button>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-[var(--radius)] border border-border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
          >
            Cancel
          </button>
          <button
            type="submit"
            aria-label="Save dav section"
            disabled={section === null}
            className="rounded-[var(--radius)] bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground transition-colors hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
          >
            Save
          </button>
        </div>
      </div>
    </form>
  );
}
