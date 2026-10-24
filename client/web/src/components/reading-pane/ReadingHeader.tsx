// SPDX-License-Identifier: GPL-3.0-only
// The reading header: the subject at the reading type scale (19px, weight
// 640, tightened tracking; the A3 voice), then the sender block with the
// avatar beside a semibold name over the bare address, and the full UTC
// stamp right-aligned on that row. The pane's controls ride the header's
// top-right corner: the reading zoom's three-step menu, the position
// picker, and the overflow (the own window, print, and the .eml save, each
// surfacing its outcome the way the attachment save does, beside the
// placeholders for what has not shipped). The avatar resolves through the
// people command on the sender's address, the one resolution every surface
// shares. The stamp derives from the UTC parts the same way the list's
// does, with the year always shown and the time of day appended in the
// form the clock setting names.

import { useState } from 'react';

import {
  ForwardIcon,
  JunkIcon,
  MoreIcon,
  PanelLeftIcon,
  ReplyAllIcon,
  ReplyIcon,
} from '@/components/icons';
import { ContextMenu, placeholderItem, type ContextMenuItem } from '@/components/ui/ContextMenu';
import { useContextMenu } from '@/components/ui/useContextMenu';
import { useWindowStage } from '@/hooks/useWindowSize';
import { useAppearanceStore, type ReadingPosition, type ReadingZoomStep } from '@/services/appearance';
import { saveEml } from '@/services/attachments';
import { dispatch, intents, type MessageRef } from '@/services/ipc';
import { openMessageWindow, openPrintWindow } from '@/features/reader/readerWindows';
import { Avatar } from '@/features/people/Avatar';
import { useSettingsStore } from '@/features/settings/settingsStore';
import { carriedAttachmentIds } from '@/stores/readingStore';

/** The month abbreviations the stamp uses, in UTC order. */
const MONTHS = [
  'Jan',
  'Feb',
  'Mar',
  'Apr',
  'May',
  'Jun',
  'Jul',
  'Aug',
  'Sep',
  'Oct',
  'Nov',
  'Dec',
] as const;

/**
 * Formats the open message's sent_at as `9 Oct 2026, 08:30`, or the 12-hour
 * form with its meridiem when the clock setting says so; empty when unknown.
 */
export function formatReadingDate(sentAt: string, twentyFour = true): string {
  if (sentAt === '') {
    return '';
  }
  const stamp = new Date(sentAt);
  if (Number.isNaN(stamp.getTime())) {
    return '';
  }
  const two = (value: number): string => String(value).padStart(2, '0');
  const month = MONTHS[stamp.getUTCMonth()] ?? '';
  if (twentyFour) {
    return `${stamp.getUTCDate()} ${month} ${stamp.getUTCFullYear()}, ${two(stamp.getUTCHours())}:${two(stamp.getUTCMinutes())}`;
  }
  const hour = stamp.getUTCHours();
  const meridiem = hour < 12 ? 'AM' : 'PM';
  const twelve = hour % 12 === 0 ? 12 : hour % 12;
  return `${stamp.getUTCDate()} ${month} ${stamp.getUTCFullYear()}, ${twelve}:${two(stamp.getUTCMinutes())} ${meridiem}`;
}

interface ReadingHeaderProps {
  /** The subject, empty if none. */
  subject: string;
  /** The sender's display name, empty when the header carried none. */
  fromName: string;
  /** The sender's bare email address, empty when there is no sender. */
  fromAddress: string;
  /** When the message was sent, as a whole-second UTC instant string. */
  sentAt: string;
  /** The open message's ref; the compose verbs and the overflow ride it. */
  message?: MessageRef;
  /** When set and a sender stands, the header offers an add-contact chip. */
  onAddContact?: () => void;
  /** The folder the message was opened from, carried onto the own window. */
  openedFromFolder?: { account: string; key: string } | null;
  /** When set, the verb row offers Not spam: the message sits in the
   *  account's junk folder and the account can report. */
  notJunk?: boolean;
}

/** The overflow's items: the three working verbs beside the named
 *  placeholders, the save surfacing its outcome like the attachment save's. */
function overflowItems(
  message: MessageRef,
  subject: string,
  openedFromFolder: { account: string; key: string } | null | undefined,
  onOutcome: (note: string) => void,
): ContextMenuItem[] {
  const saveEmlOne = (): void => {
    void saveEml(message).then((outcome) => {
      if (outcome === 'cancelled') {
        return;
      }
      onOutcome('saved' in outcome ? `Saved to ${outcome.saved.path}` : outcome.failed.reason);
    });
  };
  return [
    placeholderItem('Recipients', 'Recipient details arrive with a future slice'),
    { separator: true },
    {
      label: 'Open in New Window',
      onSelect: () =>
        void openMessageWindow(
          message,
          subject === '' ? '(no subject)' : subject,
          openedFromFolder?.key ?? null,
        ),
    },
    { label: 'Print', onSelect: () => void openPrintWindow(message) },
    { label: 'Save as .eml', onSelect: saveEmlOne },
    { separator: true },
    placeholderItem('View source', 'Viewing the message source arrives with a future slice'),
  ];
}

const POSITIONS: ReadonlyArray<{ id: ReadingPosition; label: string }> = [
  { id: 'right', label: 'Right' },
  { id: 'bottom', label: 'Bottom' },
  { id: 'off', label: 'Off' },
];

/** The position picker: kylins' three-way dock as a checked menu. At the
 *  narrow stage the pane is the whole row, so the menu offers the way back
 *  to the list instead. */
function PositionMenu() {
  const stage = useWindowStage();
  const position = useAppearanceStore((s) => s.readingPosition);
  const setPosition = useAppearanceStore((s) => s.setReadingPosition);
  const menu = useContextMenu();

  const items: ContextMenuItem[] =
    stage === 'narrow'
      ? [
          {
            label: 'Back to list',
            onSelect: () => void dispatch(intents.closeReading()),
          },
        ]
      : POSITIONS.map(({ id, label }) => ({
          label,
          checked: position === id,
          onSelect: () => setPosition(id),
        }));

  return (
    <>
      <button
        type="button"
        aria-label="Reading pane position"
        title="Reading pane position"
        onClick={(event) => menu.open(event)}
        className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <PanelLeftIcon size={16} />
      </button>
      {menu.state !== null && (
        <ContextMenu x={menu.state.x} y={menu.state.y} items={items} onClose={menu.close} />
      )}
    </>
  );
}

const ZOOM_STEPS: ReadonlyArray<{ id: ReadingZoomStep; label: string }> = [
  { id: 'small', label: 'Smaller' },
  { id: 'default', label: 'Default' },
  { id: 'large', label: 'Larger' },
];

/** The reading zoom's three-step menu, persisted in the local appearance
 *  store beside the font steps. */
function ZoomMenu() {
  const zoom = useAppearanceStore((s) => s.readingZoom);
  const setZoom = useAppearanceStore((s) => s.setReadingZoom);
  const menu = useContextMenu();

  const items: ContextMenuItem[] = ZOOM_STEPS.map(({ id, label }) => ({
    label,
    checked: zoom === id,
    onSelect: () => setZoom(id),
  }));

  return (
    <>
      <button
        type="button"
        aria-label="Reading zoom"
        title="Reading zoom"
        onClick={(event) => menu.open(event)}
        className="inline-flex h-7 min-w-7 items-center justify-center rounded-md px-1.5 text-xs font-medium text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        Aa
      </button>
      {menu.state !== null && (
        <ContextMenu x={menu.state.x} y={menu.state.y} items={items} onClose={menu.close} />
      )}
    </>
  );
}

/** The kylins-style verb row: one icon button per compose verb, and Not spam
 *  for a message the junk-folder context and the account's report
 *  capability both stand behind. */
function VerbRow({ message, notJunk }: { message: MessageRef; notJunk?: boolean }) {
  const verbs = [
    { label: 'Reply', icon: ReplyIcon, intent: () => intents.replyTo(message) },
    { label: 'Reply all', icon: ReplyAllIcon, intent: () => intents.replyAllTo(message) },
    {
      label: 'Forward',
      icon: ForwardIcon,
      intent: () => intents.forward(message, carriedAttachmentIds()),
    },
    ...(notJunk
      ? [{ label: 'Not spam', icon: JunkIcon, intent: () => intents.markNotJunk({ message }) }]
      : []),
  ] as const;
  return (
    <div className="mt-3 flex items-center gap-1">
      {verbs.map(({ label, icon: Icon, intent }) => (
        <button
          key={label}
          type="button"
          aria-label={label}
          title={label}
          onClick={() => void dispatch(intent())}
          className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <Icon size={16} />
        </button>
      ))}
    </div>
  );
}

export function ReadingHeader({
  subject,
  fromName,
  fromAddress,
  sentAt,
  message,
  onAddContact,
  openedFromFolder,
  notJunk,
}: ReadingHeaderProps) {
  const twentyFour = useSettingsStore((s) => s.settings.twenty_four_clock);
  const hasSender = fromName !== '' || fromAddress !== '';
  const displayName = fromName === '' ? fromAddress : fromName;
  const date = formatReadingDate(sentAt, twentyFour);
  const overflow = useContextMenu();
  const [note, setNote] = useState<string | null>(null);
  return (
    <div className="relative shrink-0 border-b border-[var(--hairline)] px-7 pb-3.5 pt-5">
      <h2 className="truncate pr-24 text-[19px] font-[640] leading-[1.3] tracking-[-0.015em] text-foreground">
        {subject === '' ? '(no subject)' : subject}
      </h2>
      <div className="absolute right-4 top-4 flex items-center gap-1">
        <ZoomMenu />
        <PositionMenu />
        {message !== undefined && (
          <button
            type="button"
            aria-label="More actions"
            title="More actions"
            onClick={(event) => overflow.open(event)}
            className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <MoreIcon size={16} />
          </button>
        )}
      </div>
      {overflow.state !== null && message !== undefined && (
        <ContextMenu
          x={overflow.state.x}
          y={overflow.state.y}
          items={overflowItems(message, subject, openedFromFolder, setNote)}
          onClose={overflow.close}
        />
      )}
      {note !== null && (
        <p role="status" className="mt-2 truncate text-xs text-muted-foreground">
          {note}
        </p>
      )}
      {hasSender ? (
        <div className="mt-2.5 flex items-center gap-3">
          {fromAddress !== '' && <Avatar address={fromAddress} size={36} />}
          <div className="min-w-0 flex-1">
            <p className="truncate text-[13px] font-semibold text-foreground">{displayName}</p>
            {fromName !== '' && fromAddress !== '' && (
              <p className="truncate text-xs text-muted-text">{fromAddress}</p>
            )}
          </div>
          {onAddContact !== undefined && fromAddress !== '' && (
            <button
              type="button"
              onClick={onAddContact}
              className="shrink-0 rounded-full border border-border-subtle px-2 py-0.5 text-[11px] text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              Add to contacts
            </button>
          )}
          {date !== '' && (
            <span className="shrink-0 self-start text-xs tabular-nums text-muted-text">
              {date}
            </span>
          )}
        </div>
      ) : (
        date !== '' && <p className="mt-1 text-xs tabular-nums text-muted-text">{date}</p>
      )}
      {message !== undefined && <VerbRow message={message} notJunk={notJunk} />}
    </div>
  );
}
