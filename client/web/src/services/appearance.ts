// SPDX-License-Identifier: GPL-3.0-only
// The device-local appearance choices: the accent skin, the contrast mode,
// the font-size step, the density axis, and the texture axis. They are
// presentation only; the persisted settings block has no fields for them,
// so they never cross the settings intents. Instead they live in
// localStorage and land as attributes on the document root (data-skin,
// data-contrast, data-font-size, data-density, data-texture), which
// skins.css and theme.css read. The store hydrates from localStorage at
// module load, and applyAppearance re-asserts the attributes at startup.

import { create } from 'zustand';

/** The accent skins, in the General page's swatch order; iris is the default. */
export type SkinId =
  | 'iris'
  | 'blue'
  | 'indigo'
  | 'rose'
  | 'emerald'
  | 'amber'
  | 'violet'
  | 'orange';

export type ContrastMode = 'default' | 'high';

export type FontSizeStep = 'small' | 'default' | 'large';

/** The density steps: the UI1 axis (comfortable 28px rows, compact 24px). */
export type DensityMode = 'comfortable' | 'compact';

/** The texture steps: warm is the adopted A3 execution and the default,
 *  calm is its hue-260 sibling, classic the pre-texture cool slate. */
export type TextureMode = 'warm' | 'calm' | 'classic';

/** The reading pane's position, kylins' three-way: docked right of the
 *  list, docked beneath it, or hidden outright. */
export type ReadingPosition = 'right' | 'bottom' | 'off';

/** The reading body's zoom steps, one per step the control offers: the same
 *  three-step shape the app font size takes. */
export type ReadingZoomStep = 'small' | 'default' | 'large';

/** One act a row swipe can take, the four the intent surface serves. */
export type SwipeAction = 'archive' | 'trash' | 'read' | 'flag';

/** The swatch catalogue the General page renders; swatch is the preview dot. */
export const SKINS: ReadonlyArray<{ id: SkinId; label: string; swatch: string }> = [
  { id: 'iris', label: 'Iris', swatch: '#7a6ff0' },
  { id: 'blue', label: 'Blue', swatch: '#0a59f7' },
  { id: 'indigo', label: 'Indigo', swatch: '#6366f1' },
  { id: 'rose', label: 'Rose', swatch: '#f43f5e' },
  { id: 'emerald', label: 'Emerald', swatch: '#10b981' },
  { id: 'amber', label: 'Amber', swatch: '#f59e0b' },
  { id: 'violet', label: 'Violet', swatch: '#8b5cf6' },
  { id: 'orange', label: 'Orange', swatch: '#f97316' },
];

const KEYS = {
  skin: 'astra.skin',
  contrast: 'astra.contrast',
  fontSize: 'astra.fontSize',
  density: 'astra.density',
  texture: 'astra.texture',
  readingPosition: 'astra.readingPosition',
  readingZoom: 'astra.readingZoom',
  swipePrimary: 'astra.swipePrimary',
  swipeSecondary: 'astra.swipeSecondary',
} as const;

function readSkin(): SkinId {
  const raw = localStorage.getItem(KEYS.skin);
  return SKINS.some((s) => s.id === raw) ? (raw as SkinId) : 'iris';
}

function readContrast(): ContrastMode {
  return localStorage.getItem(KEYS.contrast) === 'high' ? 'high' : 'default';
}

function readFontSize(): FontSizeStep {
  const raw = localStorage.getItem(KEYS.fontSize);
  return raw === 'small' || raw === 'large' ? raw : 'default';
}

function readDensity(): DensityMode {
  return localStorage.getItem(KEYS.density) === 'compact' ? 'compact' : 'comfortable';
}

function readTexture(): TextureMode {
  const raw = localStorage.getItem(KEYS.texture);
  return raw === 'calm' || raw === 'classic' ? raw : 'warm';
}

function readReadingPosition(): ReadingPosition {
  const raw = localStorage.getItem(KEYS.readingPosition);
  return raw === 'bottom' || raw === 'off' ? raw : 'right';
}

function readReadingZoom(): ReadingZoomStep {
  const raw = localStorage.getItem(KEYS.readingZoom);
  return raw === 'small' || raw === 'large' ? raw : 'default';
}

/** The standing swipe actions, one per direction, falling back to the
 *  defaults when the stored value names no act. */
function readSwipeAction(
  key: 'swipePrimary' | 'swipeSecondary',
  fallback: SwipeAction,
): SwipeAction {
  const raw = localStorage.getItem(KEYS[key]);
  return raw === 'archive' || raw === 'trash' || raw === 'read' || raw === 'flag'
    ? raw
    : fallback;
}

interface AppearanceState {
  skin: SkinId;
  contrast: ContrastMode;
  fontSize: FontSizeStep;
  density: DensityMode;
  texture: TextureMode;
  readingPosition: ReadingPosition;
  readingZoom: ReadingZoomStep;
  swipePrimary: SwipeAction;
  swipeSecondary: SwipeAction;
  setSkin: (skin: SkinId) => void;
  setContrast: (contrast: ContrastMode) => void;
  setFontSize: (step: FontSizeStep) => void;
  setDensity: (density: DensityMode) => void;
  setTexture: (texture: TextureMode) => void;
  setReadingPosition: (position: ReadingPosition) => void;
  setReadingZoom: (step: ReadingZoomStep) => void;
  setSwipePrimary: (action: SwipeAction) => void;
  setSwipeSecondary: (action: SwipeAction) => void;
}

/** Writes the five attributes onto the document root. */
function applyAttributes(
  state: Pick<AppearanceState, 'skin' | 'contrast' | 'fontSize' | 'density' | 'texture'>,
): void {
  const root = document.documentElement;
  root.dataset.skin = state.skin;
  root.dataset.contrast = state.contrast;
  root.dataset.fontSize = state.fontSize;
  root.dataset.density = state.density;
  root.dataset.texture = state.texture;
}

export const useAppearanceStore = create<AppearanceState>((set) => ({
  skin: readSkin(),
  contrast: readContrast(),
  fontSize: readFontSize(),
  density: readDensity(),
  texture: readTexture(),
  readingPosition: readReadingPosition(),
  readingZoom: readReadingZoom(),
  swipePrimary: readSwipeAction('swipePrimary', 'archive'),
  swipeSecondary: readSwipeAction('swipeSecondary', 'trash'),

  setSkin: (skin) => {
    localStorage.setItem(KEYS.skin, skin);
    set({ skin });
    applyAttributes(useAppearanceStore.getState());
  },

  setContrast: (contrast) => {
    localStorage.setItem(KEYS.contrast, contrast);
    set({ contrast });
    applyAttributes(useAppearanceStore.getState());
  },

  setFontSize: (fontSize) => {
    localStorage.setItem(KEYS.fontSize, fontSize);
    set({ fontSize });
    applyAttributes(useAppearanceStore.getState());
  },

  setDensity: (density) => {
    localStorage.setItem(KEYS.density, density);
    set({ density });
    applyAttributes(useAppearanceStore.getState());
  },

  setTexture: (texture) => {
    localStorage.setItem(KEYS.texture, texture);
    set({ texture });
    applyAttributes(useAppearanceStore.getState());
  },

  setReadingPosition: (readingPosition) => {
    localStorage.setItem(KEYS.readingPosition, readingPosition);
    set({ readingPosition });
  },

  // The reading body's zoom and the swipe actions write no document-root
  // attribute: the zoom reaches the reading body's own document, and the
  // swipe actions reach the row the gesture lands on.
  setReadingZoom: (readingZoom) => {
    localStorage.setItem(KEYS.readingZoom, readingZoom);
    set({ readingZoom });
  },

  setSwipePrimary: (swipePrimary) => {
    localStorage.setItem(KEYS.swipePrimary, swipePrimary);
    set({ swipePrimary });
  },

  setSwipeSecondary: (swipeSecondary) => {
    localStorage.setItem(KEYS.swipeSecondary, swipeSecondary);
    set({ swipeSecondary });
  },
}));

/** Re-asserts the persisted attributes on the document root; called once at startup. */
export function applyAppearance(): void {
  applyAttributes(useAppearanceStore.getState());
}
