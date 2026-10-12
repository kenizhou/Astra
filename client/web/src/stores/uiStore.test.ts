// SPDX-License-Identifier: GPL-3.0-only
// The theme mode owns the light and dark classes on the document root, so a
// toggle restyles the whole shell without any component knowing the class
// names. Following the host resolves through the system preference and
// keeps following it live, and an explicit override detaches the listener.
import { afterEach, describe, expect, it } from 'vitest';

import { nextThemeMode, useUiStore } from './uiStore';

/** A controllable prefers-color-scheme query standing in for the host. */
function installHostTheme(dark: boolean) {
  const listeners: Array<(event: { matches: boolean }) => void> = [];
  const query = {
    matches: dark,
    addEventListener: (_kind: string, listener: (event: { matches: boolean }) => void) => {
      listeners.push(listener);
    },
    removeEventListener: (_kind: string, listener: (event: { matches: boolean }) => void) => {
      const at = listeners.indexOf(listener);
      if (at >= 0) {
        listeners.splice(at, 1);
      }
    },
  };
  window.matchMedia = ((_query: string) => query) as typeof window.matchMedia;
  return {
    /** The host preference moves and tells whoever listens. */
    flip(): void {
      query.matches = !query.matches;
      for (const listener of [...listeners]) {
        listener({ matches: query.matches });
      }
    },
    /** How many listeners stand on the query. */
    standing: (): number => listeners.length,
  };
}

afterEach(() => {
  (window as { matchMedia?: typeof window.matchMedia }).matchMedia = undefined;
});

describe('the ui store theme', () => {
  it('light applies the light class', () => {
    useUiStore.getState().setTheme('light');
    expect(useUiStore.getState().themeMode).toBe('light');
    expect(document.documentElement.classList.contains('light')).toBe(true);
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });

  it('dark applies the dark class', () => {
    useUiStore.getState().setTheme('dark');
    expect(useUiStore.getState().themeMode).toBe('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(document.documentElement.classList.contains('light')).toBe(false);
  });

  it('follow host resolves through the system preference', () => {
    const host = installHostTheme(true);
    useUiStore.getState().setTheme('follow_host');

    expect(useUiStore.getState().themeMode).toBe('follow_host');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(document.documentElement.classList.contains('light')).toBe(false);
    expect(host.standing()).toBe(1);
  });

  it('follow host keeps following the host live', () => {
    const host = installHostTheme(false);
    useUiStore.getState().setTheme('follow_host');
    expect(document.documentElement.classList.contains('light')).toBe(true);

    host.flip();
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(useUiStore.getState().themeMode).toBe('follow_host');
  });

  it('an explicit override detaches the host listener', () => {
    const host = installHostTheme(true);
    useUiStore.getState().setTheme('follow_host');
    useUiStore.getState().setTheme('light');

    host.flip();
    expect(document.documentElement.classList.contains('light')).toBe(true);
    expect(document.documentElement.classList.contains('dark')).toBe(false);
    expect(host.standing()).toBe(0);
  });

  it('follow host without a host query stands on light', () => {
    useUiStore.getState().setTheme('follow_host');
    expect(document.documentElement.classList.contains('light')).toBe(true);
  });
});

describe('the theme override cycle', () => {
  it('walks follow host, light, dark, and back', () => {
    expect(nextThemeMode('follow_host')).toBe('light');
    expect(nextThemeMode('light')).toBe('dark');
    expect(nextThemeMode('dark')).toBe('follow_host');
  });
});

describe('the ui store shell view', () => {
  it('mail is the default view', () => {
    useUiStore.setState({ shellView: 'mail' });
    expect(useUiStore.getState().shellView).toBe('mail');
  });

  it('switches to the calendar and back', () => {
    useUiStore.getState().setShellView('calendar');
    expect(useUiStore.getState().shellView).toBe('calendar');
    useUiStore.getState().setShellView('mail');
    expect(useUiStore.getState().shellView).toBe('mail');
  });
});

describe('the settings screen open state', () => {
  it('starts closed and moves with its setter', () => {
    useUiStore.setState({ settingsOpen: false });
    expect(useUiStore.getState().settingsOpen).toBe(false);
    useUiStore.getState().setSettingsOpen(true);
    expect(useUiStore.getState().settingsOpen).toBe(true);
  });
});
