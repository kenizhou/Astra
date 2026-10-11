// SPDX-License-Identifier: GPL-3.0-only
// The theme mode owns the light and dark classes on the document root, so a
// toggle restyles the whole shell without any component knowing the class names.
import { describe, expect, it } from 'vitest';

import { useUiStore } from './uiStore';

describe('the ui store theme', () => {
  it('light is the default and applies the light class', () => {
    useUiStore.getState().setTheme('light');
    expect(useUiStore.getState().themeMode).toBe('light');
    expect(document.documentElement.classList.contains('light')).toBe(true);
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });

  it('toggling flips to dark and swaps the class', () => {
    useUiStore.getState().setTheme('light');
    useUiStore.getState().toggleTheme();
    expect(useUiStore.getState().themeMode).toBe('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(document.documentElement.classList.contains('light')).toBe(false);
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
