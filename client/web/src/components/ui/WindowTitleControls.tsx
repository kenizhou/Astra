// SPDX-License-Identifier: GPL-3.0-only
// The frameless window's controls: minimise, maximise or restore, close. The
// maximise button tracks the window's real state through onResized so the
// glyph it draws is the state it acts on. Every call is guarded: outside a
// Tauri window (a plain browser, a test render) the buttons quietly do
// nothing rather than throw.

import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

function useMaximizedState(): boolean {
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    async function init() {
      try {
        const appWindow = getCurrentWindow();
        setIsMaximized(await appWindow.isMaximized());
        unlisten = await appWindow.onResized(async () => {
          setIsMaximized(await appWindow.isMaximized());
        });
      } catch {
        // Not in a Tauri window; the glyph stays at its rest state.
      }
    }
    void init();

    return () => {
      unlisten?.();
    };
  }, []);

  return isMaximized;
}

export function WindowTitleControls() {
  const isMaximized = useMaximizedState();

  const minimise = async () => {
    try {
      await getCurrentWindow().minimize();
    } catch {
      // Not in a Tauri window.
    }
  };

  const toggleMaximise = async () => {
    try {
      await getCurrentWindow().toggleMaximize();
    } catch {
      // Not in a Tauri window.
    }
  };

  const closeWindow = async () => {
    try {
      await getCurrentWindow().close();
    } catch {
      // Not in a Tauri window.
    }
  };

  const button =
    'inline-flex h-8 w-10 items-center justify-center rounded-[var(--radius-xs)] text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';

  return (
    <div className="flex items-center gap-0.5">
      <button type="button" onClick={minimise} aria-label="Minimise" className={button}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path d="M0 5h10" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>
      <button
        type="button"
        onClick={toggleMaximise}
        aria-label={isMaximized ? 'Restore' : 'Maximise'}
        className={button}
      >
        {isMaximized ? (
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <rect x="0.5" y="2.5" width="7" height="7" fill="none" stroke="currentColor" />
            <path d="M2.5 2.5V0.5h7v7h-2" fill="none" stroke="currentColor" />
          </svg>
        ) : (
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <rect
              x="0.5"
              y="0.5"
              width="9"
              height="9"
              fill="none"
              stroke="currentColor"
            />
          </svg>
        )}
      </button>
      <button
        type="button"
        onClick={closeWindow}
        aria-label="Close window"
        className={`${button} hover:bg-destructive hover:text-destructive-foreground`}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path d="M0 0l10 10M10 0L0 10" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>
    </div>
  );
}
