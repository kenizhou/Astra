// SPDX-License-Identifier: GPL-3.0-only
// The one avatar every surface shares: the monogram the snapshot rows carry
// (the core's one derivation) drawn as initials on its palette colour, the
// person glyph when the initials are empty, and the cached photo the avatar
// command resolves replacing the monogram when one stands. A caller with no
// row of its own (the reading header) passes the address alone and the same
// command supplies the monogram.

import { useEffect, useState } from 'react';

import { peopleAvatar, type Monogram } from '@/services/ipc';

interface AvatarProps {
  /** The snapshot's own monogram, when the caller carries a row. */
  monogram?: Monogram;
  /** The address the photo resolution keys on; absent draws the monogram alone. */
  address?: string;
  /** The circle's pixel size. */
  size?: number;
}

/** A webview-safe source for a host file path: the asset protocol when the
 * Tauri bridge stands, an inert placeholder otherwise. */
function assetUrl(path: string): string {
  const internals = (
    window as { __TAURI_INTERNALS__?: { convertFileSrc: (path: string) => string } }
  ).__TAURI_INTERNALS__;
  if (internals !== undefined) {
    return internals.convertFileSrc(path);
  }
  return `asset://localhost/${encodeURIComponent(path)}`;
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
      .then((view) => {
        if (standing) {
          setResolved({ photo: view.photo, monogram: { initials: view.initials, colour: view.colour } });
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
      className="flex shrink-0 select-none items-center justify-center overflow-hidden rounded-full text-xs font-semibold text-white"
      style={{
        width: size,
        height: size,
        backgroundColor: drawn?.colour ?? 'var(--muted)',
      }}
    >
      {photo !== undefined ? (
        <img src={assetUrl(photo)} alt="" className="h-full w-full object-cover" />
      ) : initials === '' ? (
        <PersonGlyph />
      ) : (
        initials
      )}
    </span>
  );
}
