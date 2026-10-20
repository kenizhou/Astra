// SPDX-License-Identifier: GPL-3.0-only
// The Composing detail: the quote-style picker (the quoted original below
// the reply or above it), the app-level default send account (the
// preference the new-message button and the mailto: entry open from), and
// the signature editor: rich text over the same marks the composer widened,
// written once and assigned per account, one slot a new mail opens with
// and one its replies open with. Every control dispatches its intent and
// reflects the persisted block, never local optimism.

import { useState } from 'react';
import { generateHTML, generateJSON, generateText, type Editor as CoreEditor } from '@tiptap/core';
import type { JSONContent } from '@tiptap/react';

import { dispatch, intents, type Signature } from '@/services/ipc';
import { Editor, trimmedExtensions } from '@/features/composer/Editor';
import { EditorToolbar } from '@/features/composer/EditorToolbar';

import { PERSIST_SENTENCE } from './GeneralDetail';
import { useSettingsStore } from './settingsStore';

/** The slot buttons, in the pane's own order. */
const SLOTS: ReadonlyArray<{ id: 'new_mail' | 'reply'; label: string }> = [
  { id: 'new_mail', label: 'New mail' },
  { id: 'reply', label: 'Reply' },
];

const selectClass =
  'h-7 w-full rounded-full border border-input bg-background px-3 text-xs text-foreground outline-none transition-colors focus:border-primary';

/** The editor content one signature's html leg stands for. */
function contentOf(signature: Signature | undefined): JSONContent[] {
  if (signature === undefined) {
    return [{ type: 'paragraph' }];
  }
  return generateJSON(signature.body_html, trimmedExtensions()).content ?? [];
}

/** The signature the live document carries, both legs. */
function signatureOf(doc: JSONContent): Signature {
  const extensions = trimmedExtensions();
  return {
    body_html: generateHTML(doc, extensions),
    body_plain: generateText(doc, extensions),
  };
}

/** The Composing page of the settings screen. */
export function ComposingDetail() {
  const settings = useSettingsStore((s) => s.settings);
  const accounts = useSettingsStore((s) => s.accounts);
  const error = useSettingsStore((s) => s.error);
  const [account, setAccount] = useState(accounts[0]?.account ?? '');
  const [slot, setSlot] = useState<'new_mail' | 'reply'>('new_mail');
  const [doc, setDoc] = useState<JSONContent | null>(null);
  const [live, setLive] = useState<CoreEditor | null>(null);

  const chosen = accounts.find((row) => row.account === account) ?? accounts[0];
  const signature = slot === 'reply' ? chosen?.signatures?.reply : chosen?.signatures?.new_mail;
  // The words to save: the last change the editor reported, or the editor's
  // own standing document when no change has landed yet.
  const current = doc ?? live?.getJSON();

  return (
    <section aria-label="Composing settings" className="flex flex-col gap-4">
      <h2 className="text-sm font-semibold text-foreground">Composing</h2>
      <label className="flex max-w-[280px] flex-col gap-1 text-xs text-muted-foreground">
        Quote style
        <select
          aria-label="Quote style"
          value={settings.quote_below ? 'below' : 'above'}
          onChange={(event) =>
            void dispatch(intents.setQuoteStyle(event.target.value === 'below'))
          }
          className={selectClass}
        >
          <option value="below">Quote below the reply</option>
          <option value="above">Quote above the reply</option>
        </select>
      </label>

      <label className="flex max-w-[280px] flex-col gap-1 text-xs text-muted-foreground">
        Default send account
        <select
          aria-label="Default send account"
          value={settings.default_send_account ?? ''}
          onChange={(event) =>
            void dispatch(
              intents.setSendAccount(event.target.value === '' ? null : event.target.value),
            )
          }
          className={selectClass}
        >
          <option value="">First account</option>
          {accounts.map((row) => (
            <option key={row.account} value={row.account}>
              {row.address}
            </option>
          ))}
        </select>
      </label>

      <div className="flex max-w-[420px] flex-col gap-2">
        <h3 className="pt-2 text-xs font-semibold text-muted-foreground">Signatures</h3>
        <div className="flex gap-2">
          <label className="flex max-w-[280px] flex-1 flex-col gap-1 text-xs text-muted-foreground">
            Account
            <select
              aria-label="Signature account"
              value={chosen?.account ?? ''}
              onChange={(event) => {
                setAccount(event.target.value);
                setDoc(null);
              }}
              className={selectClass}
            >
              {accounts.map((row) => (
                <option key={row.account} value={row.account}>
                  {row.address}
                </option>
              ))}
            </select>
          </label>
          <div className="flex flex-col gap-1">
            <span className="text-xs text-muted-foreground">Slot</span>
            <div role="group" aria-label="Signature slot" className="flex gap-1">
              {SLOTS.map((entry) => (
                <button
                  key={entry.id}
                  type="button"
                  aria-pressed={slot === entry.id}
                  onClick={() => {
                    setSlot(entry.id);
                    setDoc(null);
                  }}
                  className={`rounded-[var(--radius-xs)] px-3 py-1.5 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                    slot === entry.id
                      ? 'bg-primary-muted font-medium text-primary'
                      : 'border border-border text-muted-foreground hover:bg-hover hover:text-foreground'
                  }`}
                >
                  {entry.label}
                </button>
              ))}
            </div>
          </div>
        </div>
        {chosen !== undefined && (
          <div
            key={`${chosen.account}:${slot}`}
            className="flex min-h-[120px] flex-col overflow-hidden rounded-[var(--radius-sm)] border border-border"
          >
            <EditorToolbar editor={live} />
            <Editor
              initialContent={contentOf(signature)}
              onChange={setDoc}
              onReady={(instance) => setLive(instance)}
            />
          </div>
        )}
        <div className="flex gap-1.5">
          <button
            type="button"
            onClick={() => {
              if (chosen !== undefined && current !== undefined) {
                void dispatch(
                  intents.setSignature(chosen.account, slot === 'reply', signatureOf(current)),
                );
              }
            }}
            className="rounded-[var(--radius-xs)] bg-primary px-2.5 py-1.5 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Save signature
          </button>
          <button
            type="button"
            onClick={() => {
              if (chosen !== undefined) {
                void dispatch(intents.setSignature(chosen.account, slot === 'reply', null));
              }
            }}
            className="rounded-[var(--radius-xs)] px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Clear signature
          </button>
          <button
            type="button"
            onClick={() => {
              if (chosen !== undefined && current !== undefined) {
                // Both slots take the editor's words: the button's name is
                // the contract, and unsaved edits reach the current slot
                // through it too, not only the other one.
                void dispatch(
                  intents.setSignature(chosen.account, slot === 'reply', signatureOf(current)),
                );
                void dispatch(
                  intents.setSignature(chosen.account, slot !== 'reply', signatureOf(current)),
                );
              }
            }}
            className="rounded-[var(--radius-xs)] px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Use for both
          </button>
        </div>
      </div>

      {error === 'persist_failed' && (
        <p role="alert" className="text-xs text-[var(--warning)]">
          {PERSIST_SENTENCE}
        </p>
      )}
    </section>
  );
}
