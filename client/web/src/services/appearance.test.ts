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

describe('the density axis', () => {
  it('defaults to comfortable when nothing is stored', () => {
    expect(useAppearanceStore.getState().density).toBe('comfortable');
  });

  it('writes the density attribute and its storage key together', () => {
    useAppearanceStore.getState().setDensity('compact');
    expect(document.documentElement.dataset.density).toBe('compact');
    expect(localStorage.getItem('astra.density')).toBe('compact');
    useAppearanceStore.getState().setDensity('comfortable');
    expect(document.documentElement.dataset.density).toBe('comfortable');
  });

  it('hydrates a persisted density on a fresh load', async () => {
    localStorage.setItem('astra.density', 'compact');
    vi.resetModules();

    const fresh = await import('./appearance');

    expect(fresh.useAppearanceStore.getState().density).toBe('compact');
    fresh.applyAppearance();
    expect(document.documentElement.dataset.density).toBe('compact');
  });

  it('falls back to comfortable when the stored density is unknown', async () => {
    localStorage.setItem('astra.density', 'dense');
    vi.resetModules();

    const fresh = await import('./appearance');

    expect(fresh.useAppearanceStore.getState().density).toBe('comfortable');
  });
});

describe('the reading zoom', () => {
  it('defaults to the default step and writes its storage key on set', () => {
    expect(useAppearanceStore.getState().readingZoom).toBe('default');

    useAppearanceStore.getState().setReadingZoom('large');
    expect(useAppearanceStore.getState().readingZoom).toBe('large');
    expect(localStorage.getItem('astra.readingZoom')).toBe('large');

    useAppearanceStore.getState().setReadingZoom('small');
    expect(localStorage.getItem('astra.readingZoom')).toBe('small');
  });

  it('hydrates a persisted step and falls back to default when stored unknown', async () => {
    localStorage.setItem('astra.readingZoom', 'large');
    vi.resetModules();
    const large = await import('./appearance');
    expect(large.useAppearanceStore.getState().readingZoom).toBe('large');

    localStorage.clear();
    localStorage.setItem('astra.readingZoom', 'enormous');
    vi.resetModules();
    const unknown = await import('./appearance');
    expect(unknown.useAppearanceStore.getState().readingZoom).toBe('default');
  });

  it('writes no document-root attribute: the zoom reaches the body, not the chrome', () => {
    useAppearanceStore.getState().setReadingZoom('large');

    expect(document.documentElement.dataset.readingZoom).toBeUndefined();
  });
});

describe('the swipe action choices', () => {
  it('defaults the right swipe to archive and the left to trash', () => {
    expect(useAppearanceStore.getState().swipePrimary).toBe('archive');
    expect(useAppearanceStore.getState().swipeSecondary).toBe('trash');
  });

  it('writes each direction its own storage key', () => {
    useAppearanceStore.getState().setSwipePrimary('flag');
    useAppearanceStore.getState().setSwipeSecondary('read');

    expect(localStorage.getItem('astra.swipePrimary')).toBe('flag');
    expect(localStorage.getItem('astra.swipeSecondary')).toBe('read');
    expect(useAppearanceStore.getState().swipePrimary).toBe('flag');
    expect(useAppearanceStore.getState().swipeSecondary).toBe('read');
  });

  it('hydrates persisted choices and falls back to the defaults when stored unknown', async () => {
    localStorage.setItem('astra.swipePrimary', 'trash');
    localStorage.setItem('astra.swipeSecondary', 'read');
    vi.resetModules();
    const stored = await import('./appearance');
    expect(stored.useAppearanceStore.getState().swipePrimary).toBe('trash');
    expect(stored.useAppearanceStore.getState().swipeSecondary).toBe('read');

    localStorage.clear();
    localStorage.setItem('astra.swipePrimary', 'snooze');
    vi.resetModules();
    const unknown = await import('./appearance');
    expect(unknown.useAppearanceStore.getState().swipePrimary).toBe('archive');
    expect(unknown.useAppearanceStore.getState().swipeSecondary).toBe('trash');
  });
});
