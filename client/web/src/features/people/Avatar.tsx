// SPDX-License-Identifier: GPL-3.0-only
// The one avatar every surface shares: the monogram the snapshot rows carry
// (the core's one derivation) drawn as initials on its palette colour, the
// person glyph when the initials are empty, and the cached photo the avatar
// command resolves replacing the monogram when one stands. A caller with no
// row of its own (the reading header) passes the address alone and the same
// command supplies the monogram.

import { useEffect, useState } from 'react';

import { peopleAvatar, peoplePhoto, type Monogram } from '@/services/ipc';

interface AvatarProps {
  /** The snapshot's own monogram, when the caller carries a row. */
  monogram?: Monogram;
  /** The address the photo resolution keys on; absent draws the monogram alone. */
  address?: string;
  /** The circle's pixel size. */
  size?: number;
}

/** The monogram's readable foreground: dark ink over a light palette
 *  colour, white over a dark one, derived from the colour's luminance.
 *  The two literals are the contrast answer, not theme tokens: the circle
 *  wears the palette in both themes. */
export function monogramForeground(colour: string | undefined): string {
  if (colour === undefined) {
    // No palette colour: the circle wears --muted, so the muted ink reads.
    return 'var(--muted-foreground)';
  }
  const match = colour.match(/^#?([0-9a-f]{6})$/i);
  if (match === null) {
    // An unparseable colour cannot derive its ink; white reads on the
    // saturated values the palette actually sends.
    return '#ffffff';
  }
  const packed = parseInt(match[1]!, 16);
  const r = (packed >> 16) & 255;
  const g = (packed >> 8) & 255;
  const b = packed & 255;
  const luminance = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
  return luminance > 0.6 ? '#1c1917' : '#ffffff';
}

/** A stroke-drawn person glyph for the monogram with no initials. */
function PersonGlyph() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="16"
      height="16"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      role="img"
      aria-label="Person"
    >
      <circle cx="12" cy="8" r="4" />
      <path d="M4 21c0-4 3.6-7 8-7s8 3 8 7" />
    </svg>
  );
}

export function Avatar({ monogram, address, size = 32 }: AvatarProps) {
  const [resolved, setResolved] = useState<{ photo?: string; monogram?: Monogram }>({});

  useEffect(() => {
    if (address === undefined) {
      return;
    }
    let standing = true;
    peopleAvatar(address)
      .then(async (view) => {
        // The photo crosses as bytes through the validated command, never
        // through a protocol the config would have to widen for: the data
        // URI the CSP already admits.
        const fetched =
          view.photo === undefined ? undefined : await peoplePhoto(view.photo);
        const photo = fetched ?? undefined;
        if (standing) {
          setResolved({
            photo,
            monogram: { initials: view.initials, colour: view.colour },
          });
        }
      })
      .catch(() => {
        // An unresolvable pull leaves the monogram standing.
      });
    return () => {
      standing = false;
    };
  }, [address]);

  const photo = resolved.photo;
  const drawn = monogram ?? resolved.monogram;
  const initials = drawn?.initials ?? '';

  return (
    <span
      role="img"
      aria-label="Avatar"
      className="flex shrink-0 select-none items-center justify-center overflow-hidden rounded-full text-xs font-semibold"
      style={{
        width: size,
        height: size,
        backgroundColor: drawn?.colour ?? 'var(--muted)',
        color: monogramForeground(drawn?.colour),
      }}
    >
      {photo !== undefined ? (
        <img src={photo} alt="" className="h-full w-full object-cover" />
      ) : initials === '' ? (
        <PersonGlyph />
      ) : (
        initials
      )}
    </span>
  );
}
