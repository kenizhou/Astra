// SPDX-License-Identifier: GPL-3.0-only
// The device-local appearance module: a choice writes its localStorage key
// and its document-root attribute at once, a fresh load hydrates from the
// persisted keys, and an unknown stored value falls back to the default.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  applyAppearance,
  useAppearanceStore,
  SKINS,
} from './appearance';

beforeEach(() => {
  localStorage.clear();
  const root = document.documentElement;
  delete root.dataset.skin;
  delete root.dataset.contrast;
  delete root.dataset.fontSize;
  useAppearanceStore.setState({ skin: 'iris', contrast: 'default', fontSize: 'default' });
});

afterEach(() => {
  localStorage.clear();
});

describe('the appearance store', () => {
  it('catalogues the eight skins with iris first', () => {
    expect(SKINS.map((s) => s.id)).toEqual([
      'iris',
      'blue',
      'indigo',
      'rose',
      'emerald',
      'amber',
      'violet',
      'orange',
    ]);
  });

  it('writes the skin attribute and its storage key together', () => {
    useAppearanceStore.getState().setSkin('rose');

    expect(document.documentElement.dataset.skin).toBe('rose');
    expect(localStorage.getItem('astra.skin')).toBe('rose');
    expect(useAppearanceStore.getState().skin).toBe('rose');
  });

  it('hydrates a persisted skin on a fresh load', async () => {
    localStorage.setItem('astra.skin', 'emerald');
    vi.resetModules();

    const fresh = await import('./appearance');

    expect(fresh.useAppearanceStore.getState().skin).toBe('emerald');
    fresh.applyAppearance();
    expect(document.documentElement.dataset.skin).toBe('emerald');
  });

  it('falls back to iris when the stored skin is unknown', async () => {
    localStorage.setItem('astra.skin', 'chartreuse');
    vi.resetModules();

    const fresh = await import('./appearance');

    expect(fresh.useAppearanceStore.getState().skin).toBe('iris');
  });

  it('writes the contrast attribute and its storage key together', () => {
    useAppearanceStore.getState().setContrast('high');

    expect(document.documentElement.dataset.contrast).toBe('high');
    expect(localStorage.getItem('astra.contrast')).toBe('high');

    useAppearanceStore.getState().setContrast('default');
    expect(document.documentElement.dataset.contrast).toBe('default');
  });

  it('writes the font-size attribute and its storage key together', () => {
    useAppearanceStore.getState().setFontSize('large');

    expect(document.documentElement.dataset.fontSize).toBe('large');
    expect(localStorage.getItem('astra.fontSize')).toBe('large');
  });

  it('applies the persisted attributes at startup', () => {
    localStorage.setItem('astra.skin', 'amber');
    localStorage.setItem('astra.contrast', 'high');
    localStorage.setItem('astra.fontSize', 'small');
    useAppearanceStore.setState({ skin: 'amber', contrast: 'high', fontSize: 'small' });

    applyAppearance();

    const root = document.documentElement;
    expect(root.dataset.skin).toBe('amber');
    expect(root.dataset.contrast).toBe('high');
    expect(root.dataset.fontSize).toBe('small');
  });
});
