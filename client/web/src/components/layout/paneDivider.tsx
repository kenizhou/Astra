// SPDX-License-Identifier: GPL-3.0-only
// The one pane divider, shared by every view's panel row: a pill handle
// between two cards. Hover lifts a quiet wash of the ink colour, never the
// accent, so a drag reads as the structure yielding rather than a control
// lighting up.

import { Separator } from 'react-resizable-panels';

export function PaneDivider() {
  return (
    <Separator className="group relative mx-1 w-1.5 shrink-0 cursor-col-resize rounded-full bg-border">
      <span
        aria-hidden="true"
        className="absolute inset-0 rounded-full bg-[var(--muted-text)] opacity-0 transition-opacity duration-[var(--dur-row)] group-hover:opacity-30"
      />
    </Separator>
  );
}
