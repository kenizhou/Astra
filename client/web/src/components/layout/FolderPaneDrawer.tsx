// SPDX-License-Identifier: GPL-3.0-only
// The compact shell's folder drawer: the folder pane riding over the content
// as an overlay card, sliding in from the left. A backdrop click or Escape
// closes it. Above the compact breakpoint the pane stands in the panel row
// and the drawer never mounts.

import { useEffect } from 'react';

import { useUiStore } from '@/stores/uiStore';

export function FolderPaneDrawer({ children }: { children: React.ReactNode }) {
  const open = useUiStore((s) => s.folderDrawerOpen);

  useEffect(() => {
    if (!open) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        useUiStore.getState().setFolderDrawerOpen(false);
      }
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [open]);

  if (!open) {
    return null;
  }

  return (
    <div
      className="absolute inset-0 z-[var(--z-modal)]"
      onClick={(event) => {
        // The backdrop answers, the drawer card never does.
        if (event.target === event.currentTarget) {
          useUiStore.getState().setFolderDrawerOpen(false);
        }
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Folder drawer"
        className="h-full w-[var(--drawer-w)] animate-[drawer-slide-in_160ms_ease-out] overflow-hidden rounded-r-xl border border-border-subtle bg-card shadow-lg"
      >
        {children}
      </div>
    </div>
  );
}
