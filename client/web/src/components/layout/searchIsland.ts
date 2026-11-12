// SPDX-License-Identifier: GPL-3.0-only
// The search island's opener seam: the island's expanded state is local to the
// component, so it registers its open verb here on mount and clears it on
// unmount. The palette's Search mail command calls through it; one island
// stands at a time, so a registration replaces the last.

type IslandOpener = () => void;

let opener: IslandOpener | null = null;

/** The island hands over its open verb on mount and takes it back on unmount. */
export function registerSearchIslandOpener(open: IslandOpener | null): void {
  opener = open;
}

/** Opens the island through the standing registration; false when none stands. */
export function openSearchIsland(): boolean {
  if (opener === null) {
    return false;
  }
  opener();
  return true;
}
