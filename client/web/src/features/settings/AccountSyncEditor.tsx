// SPDX-License-Identifier: GPL-3.0-only
// The per-account sync editor on the account's page: how far back the
// account syncs (the same depth the search depth note reads), the
// per-message offline body cap, and the poll cadence. Each control writes
// through the one sync settings arm, stating only the field it changed, and
// reflects the row's effective values, never local optimism. The cap's
// words state the per-message rule the engine enforces: the figure bounds
// each message body, not the account's offline total.

import { useState } from 'react';

import { dispatch, intents, type AccountSettingsRow, type SyncBodyCap } from '@/services/ipc';

const selectClass =
  'h-7 w-full rounded-full border border-input bg-background px-3 text-xs text-foreground outline-none transition-colors focus:border-primary';

/** The per-message body cap an account keeps when the user has set none: the
 *  product default, 256 MiB, in octets. */
const DEFAULT_CAP_OCTETS = 256 * 1024 * 1024;

/** The depth an account syncs when the user has set none, in months. */
const DEFAULT_DEPTH_MONTHS = 3;

/** The cadence an account polls at when the user has set none, in minutes. */
const DEFAULT_POLL_MINUTES = 30;

/** The depth choices, in whole months; zero is the whole mailbox. */
const DEPTHS = [0, 1, 3, 6, 12, 24];

/** The cadence choices, in minutes, inside the 15 to 120 bounds. */
const CADENCES = [15, 30, 45, 60, 90, 120];

/** One select option row. */
interface Option {
  value: string;
  label: string;
}

/** The depth words: the whole mailbox, or the months back. */
function depthLabel(months: number): string {
  if (months === 0) {
    return 'Everything';
  }
  return months === 1 ? 'The last month' : `The last ${months} months`;
}

/** The depth options, the standing value present when the list misses it. */
function depthOptions(depth: number): Option[] {
  const months = DEPTHS.includes(depth)
    ? DEPTHS
    : [...DEPTHS, depth].sort((left, right) => left - right);
  return months.map((months) => ({ value: String(months), label: depthLabel(months) }));
}

/** The cadence options, the standing value present when the list misses it. */
function cadenceOptions(cadence: number): Option[] {
  const minutes = CADENCES.includes(cadence)
    ? CADENCES
    : [...CADENCES, cadence].sort((left, right) => left - right);
  return minutes.map((minutes) => ({ value: String(minutes), label: `Every ${minutes} minutes` }));
}

/** The cap picker's value for the standing cap. */
function capValue(cap: SyncBodyCap): 'off' | 'everything' | 'default' | 'custom' {
  if (cap === 'off' || cap === 'everything') {
    return cap;
  }
  return cap.bytes.octets === DEFAULT_CAP_OCTETS ? 'default' : 'custom';
}

/** The figure the custom input seeds from: the standing custom cap in
 *  mebibytes, or the product default's figure when none stands. */
function customMiB(cap: SyncBodyCap): number {
  if (cap !== 'off' && cap !== 'everything') {
    return Math.max(1, Math.round(cap.bytes.octets / (1024 * 1024)));
  }
  return 256;
}

/** The cap options' words, honest to the per-message rule. */
const CAP_OPTIONS: ReadonlyArray<Option> = [
  { value: 'off', label: 'Headers only' },
  { value: 'default', label: 'Up to 256 MiB per message' },
  { value: 'everything', label: 'Every message, any size' },
  { value: 'custom', label: 'Custom per-message cap' },
];

/** The per-account sync editor: the depth, the cap, and the cadence. */
export function AccountSyncEditor({ row }: { row: AccountSettingsRow }) {
  const depth = row.sync?.depth_months ?? DEFAULT_DEPTH_MONTHS;
  const cap = row.sync?.body_cap ?? { bytes: { octets: DEFAULT_CAP_OCTETS } };
  const cadence = row.sync?.poll_minutes ?? DEFAULT_POLL_MINUTES;
  // The custom figure's editor: open while the standing cap is custom or the
  // picker has just moved to it, holding the typed figure until it commits.
  const [customOpen, setCustomOpen] = useState(false);
  const [figure, setFigure] = useState<string | null>(null);

  const capSelection = customOpen || capValue(cap) === 'custom' ? 'custom' : capValue(cap);

  const writeCap = (value: string): void => {
    if (value === 'custom') {
      setCustomOpen(true);
      return;
    }
    setCustomOpen(false);
    if (value === 'off' || value === 'everything') {
      void dispatch(intents.setSyncSettings(row.account, { body_cap: value }));
      return;
    }
    void dispatch(
      intents.setSyncSettings(row.account, {
        body_cap: { bytes: { octets: DEFAULT_CAP_OCTETS } },
      }),
    );
  };

  // The commit hands the figure over and falls back to the standing truth,
  // the same never-local-optimism rule every control here follows.
  const commitFigure = (): void => {
    const mib = Number(figure);
    setFigure(null);
    if (Number.isInteger(mib) && mib > 0) {
      void dispatch(
        intents.setSyncSettings(row.account, {
          body_cap: { bytes: { octets: mib * 1024 * 1024 } },
        }),
      );
    }
  };

  return (
    <div className="flex max-w-[280px] flex-col gap-3">
      <h3 className="pt-2 text-xs font-semibold text-muted-foreground">Sync</h3>
      <label className="flex flex-col gap-1 text-xs text-muted-foreground">
        How far back mail syncs
        <select
          aria-label="How far back mail syncs"
          value={depth}
          onChange={(event) =>
            void dispatch(
              intents.setSyncSettings(row.account, {
                depth_months: Number(event.target.value),
              }),
            )
          }
          className={selectClass}
        >
          {depthOptions(depth).map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
        <span>Narrowing the depth keeps what an earlier depth already stored.</span>
      </label>
      <label className="flex flex-col gap-1 text-xs text-muted-foreground">
        Offline message bodies
        <select
          aria-label="Offline message bodies"
          value={capSelection}
          onChange={(event) => writeCap(event.target.value)}
          className={selectClass}
        >
          {CAP_OPTIONS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
        <span>The figure bounds each message body, never the whole account.</span>
      </label>
      {capSelection === 'custom' && (
        <label className="flex flex-col gap-1 text-xs text-muted-foreground">
          Maximum per message (MiB)
          <input
            aria-label="Maximum per message (MiB)"
            type="number"
            min={1}
            value={figure ?? customMiB(cap)}
            onChange={(event) => setFigure(event.target.value)}
            onBlur={commitFigure}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                commitFigure();
              }
            }}
            className={selectClass}
          />
        </label>
      )}
      <label className="flex flex-col gap-1 text-xs text-muted-foreground">
        Check for new mail
        <select
          aria-label="Check for new mail"
          value={cadence}
          onChange={(event) =>
            void dispatch(
              intents.setSyncSettings(row.account, { poll_minutes: Number(event.target.value) }),
            )
          }
          className={selectClass}
        >
          {cadenceOptions(cadence).map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}
