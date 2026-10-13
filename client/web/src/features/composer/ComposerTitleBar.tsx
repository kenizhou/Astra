// SPDX-License-Identifier: GPL-3.0-only
// The pop-out composer window's frameless title bar, kylins' ComposerTitleBar
// idiom on Astra's chrome: the strip drags, a double click on the drag region
// toggles maximise, the subject sits centred and pointer-transparent so the
// drag region under it still works, and the window's own controls pin right.

import { getCurrentWindow } from '@tauri-apps/api/window';

import { WindowTitleControls } from '@/components/ui/WindowTitleControls';

interface ComposerTitleBarProps {
  /** The centred title: the composition's subject, or the new-message word. */
  title: string;
}

export function ComposerTitleBar({ title }: ComposerTitleBarProps) {
  const toggleMaximise = async (): Promise<void> => {
    try {
      await getCurrentWindow().toggleMaximize();
    } catch {
      // Not in a Tauri window.
    }
  };

  return (
    <header
      data-tauri-drag-region
      className="glass relative flex h-[var(--header-h)] shrink-0 select-none items-center pl-4 pr-2 text-chrome-foreground"
    >
      <span className="pointer-events-none absolute inset-y-0 left-16 right-28 flex items-center justify-center truncate text-sm font-medium">
        {title}
      </span>
      <div
        data-tauri-drag-region
        role="button"
        aria-label="Toggle maximise"
        onDoubleClick={toggleMaximise}
        className="h-full min-w-[40px] flex-1 cursor-default"
      />
      <WindowTitleControls />
      <span aria-hidden="true" className="absolute inset-x-0 bottom-0 h-px bg-primary opacity-40" />
    </header>
  );
}
