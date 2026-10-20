// SPDX-License-Identifier: GPL-3.0-only
// The composition root: hydrate both surfaces once, mount the surface_changed,
// agent, draft-pool, and mailto event listeners for the session, and hand the
// screen to the shell. The listener mounts are StrictMode safe: a dispose that
// lands before the registration resolves is handled inside each mount, not
// here. The agent listeners live at the root so approval asks land in the
// store whether or not the panel is up, the draft-pool listener so a landed
// pool row refreshes the reading pane's chips whether or not it is open, and
// the mailto listener so a parsed open reaches the composer's route whatever
// surface stands.

import { useEffect } from 'react';

import { AppShell } from '@/components/layout/AppShell';
import { mountDraftPoolEvents } from '@/events/draftPoolEvents';
import { mountAgentEvents } from '@/events/agentEvents';
import { mountMailtoEvents } from '@/events/mailtoEvents';
import { hydrate, mountSurfaceEvents } from '@/events/surfaceEvents';
import { applyAppearance } from '@/services/appearance';
import { useUiStore } from '@/stores/uiStore';

export default function App() {
  const themeMode = useUiStore((s) => s.themeMode);

  useEffect(() => {
    useUiStore.getState().setTheme(themeMode);
  }, [themeMode]);

  // The device-local appearance attributes land once, before the shell's
  // first interactive paint.
  useEffect(() => applyAppearance(), []);

  useEffect(() => {
    void hydrate();
    return mountSurfaceEvents();
  }, []);

  useEffect(() => mountAgentEvents(), []);

  useEffect(() => mountDraftPoolEvents(), []);

  useEffect(() => mountMailtoEvents(), []);

  // The webview's native context menu shows nowhere: the surfaces that own
  // one render their own, and everywhere else a right-click does nothing
  // rather than offering the browser's. Text fields keep theirs.
  useEffect(() => {
    const suppress = (event: MouseEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest('input, textarea, [contenteditable="true"]') !== null) {
        return;
      }
      event.preventDefault();
    };
    document.addEventListener('contextmenu', suppress);
    return () => document.removeEventListener('contextmenu', suppress);
  }, []);

  return <AppShell />;
}
