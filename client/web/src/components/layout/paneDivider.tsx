// SPDX-License-Identifier: GPL-3.0-only
// The one pane divider, shared by every view's panel row: a pill handle
// between two cards, the accent lifting it on hover.

import { Separator } from 'react-resizable-panels';

export function PaneDivider() {
  return (
    <Separator className="mx-1 w-1.5 shrink-0 cursor-col-resize rounded-full bg-border transition-colors hover:bg-primary" />
  );
}
