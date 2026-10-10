// SPDX-License-Identifier: GPL-3.0-only
// The composition root: hydrate both surfaces once, mount the surface_changed,
// agent, and draft-pool event listeners for the session, and hand the screen
// to the shell. The listener mounts are StrictMode safe: a dispose that lands
// before the registration resolves is handled inside each mount, not here.
// The agent listeners live at the root so approval asks land in the store
// whether or not the panel is up, and the draft-pool listener so a landed
// pool row refreshes the reading pane's chips whether or not it is open.

import { useEffect } from 'react';

import { AppShell } from '@/components/layout/AppShell';
import { mountDraftPoolEvents } from '@/events/draftPoolEvents';
import { mountAgentEvents } from '@/events/agentEvents';
import { hydrate, mountSurfaceEvents } from '@/events/surfaceEvents';
import { useUiStore } from '@/stores/uiStore';

export default function App() {
  const themeMode = useUiStore((s) => s.themeMode);

  useEffect(() => {
    useUiStore.getState().setTheme(themeMode);
  }, [themeMode]);

  useEffect(() => {
    void hydrate();
    return mountSurfaceEvents();
  }, []);

  useEffect(() => mountAgentEvents(), []);

  useEffect(() => mountDraftPoolEvents(), []);

  return <AppShell />;
}
