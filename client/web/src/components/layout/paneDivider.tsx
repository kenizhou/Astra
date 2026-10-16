// SPDX-License-Identifier: GPL-3.0-only
// The one pane divider, shared by every view's panel row: a slim handle
// between two cards, a neutral lift on hover rather than the accent. The
// horizontal twin serves the bottom-docked reading layout's stack.

import { Separator } from 'react-resizable-panels';

export function PaneDivider() {
  return (
    <Separator className="mx-0.5 w-1 shrink-0 cursor-col-resize rounded-full bg-border transition-colors hover:bg-muted-foreground/50" />
  );
}

/** The horizontal twin: the drag handle between vertically stacked panes. */
export function PaneDividerHorizontal() {
  return (
    <Separator className="my-0.5 h-1 shrink-0 cursor-row-resize rounded-full bg-border transition-colors hover:bg-muted-foreground/50" />
  );
}
