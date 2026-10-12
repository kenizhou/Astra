// SPDX-License-Identifier: GPL-3.0-only
// The recipient field's token math, as pure functions: the field holds one
// comma-separated string (the wire's contract, the single source of truth)
// and everything on screen splits out of it here, so pills, completion, and
// the string itself can never disagree. The trailing token is what the user
// is typing; a field the composer was opened with has nothing in progress,
// which the settled flag states and the seed normalisation sets.

/** The text after the last comma, verbatim: the token completion works on. */
export function trailingToken(value: string): string {
  const at = value.lastIndexOf(',');
  return at === -1 ? value : value.slice(at + 1);
}

/** The non-blank comma-separated parts of the field. */
function parts(value: string): string[] {
  return value
    .split(',')
    .map((part) => part.trim())
    .filter((part) => part !== '');
}

/**
 * The finished recipients the pills draw: every part of a settled field (a
 * seeded or normalised field has nothing in progress), or the parts before
 * the trailing token while the user types.
 */
export function finishedRecipients(value: string, settled: boolean): string[] {
  if (settled) {
    return parts(value);
  }
  return parts(value.slice(0, Math.max(value.lastIndexOf(','), 0)));
}

/** Rebuilds the one string from the finished recipients and the token: the
 * token carries its own separator spacing, so a raw split round-trips. */
export function rebuildRecipients(finished: string[], token: string): string {
  if (token === '') {
    return finished.join(', ');
  }
  if (finished.length === 0) {
    return token;
  }
  return `${finished.join(', ')},${token}`;
}

/**
 * Inserts one bare address in place of the trailing token: never
 * `Name <address>`, because the core parses addresses and a name carrying a
 * comma would split into two invalid recipients. The trailing separator
 * empties the token and leaves the next keystroke a fresh one to grow.
 */
export function insertBareAddress(value: string, address: string): string {
  return `${[...finishedRecipients(value, false), address].join(', ')}, `;
}

/** Removes one finished recipient; a token in progress stays untouched. */
export function removeRecipient(value: string, address: string, settled: boolean): string {
  const token = settled ? '' : trailingToken(value);
  const kept = finishedRecipients(value, settled).filter((kept) => kept !== address);
  return rebuildRecipients(kept, token);
}

/**
 * Normalises a field the composer was opened with: every supplied address a
 * finished recipient, blanks and stray spacing dropped, nothing in progress.
 */
export function normaliseRecipients(seed: string): string {
  return parts(seed).join(', ');
}
