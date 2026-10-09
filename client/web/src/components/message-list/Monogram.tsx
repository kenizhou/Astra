// SPDX-License-Identifier: GPL-3.0-only
// The row's sender monogram: one letter in a muted circle, drawn from the
// bare address's first character so the avatar is of the sender's identity,
// never of the display name the header happened to carry.

/** The monogram letter for one address: its first character, uppercased. */
export function monogramLetter(address: string): string {
  const first = Array.from(address.trim())[0];
  return first === undefined ? '?' : first.toUpperCase();
}

interface MonogramProps {
  /** The sender's bare address; the identity the avatar is of. */
  address: string;
}

export function Monogram({ address }: MonogramProps) {
  return (
    <span
      aria-hidden="true"
      className="flex size-7 shrink-0 select-none items-center justify-center rounded-full bg-muted text-xs font-semibold text-muted-foreground"
    >
      {monogramLetter(address)}
    </span>
  );
}
