// SPDX-License-Identifier: GPL-3.0-only
// The three marks the wire widened the editor to carry: a font size whose
// attribute is one of the four words the core's own catalog names (with the
// pixel each renders at, the core's mapping), a text colour, and a
// highlight. Each renders as a span carrying its data attribute and its
// inline style, so the editor's own HTML round trips the marks back in.
// The palette the two colour pickers offer is the app's: the eight skin
// swatches beside black and white.

import { Mark, mergeAttributes } from '@tiptap/core';

import type { FontSize } from '@/services/ipc';

/** The four steps the font size control offers, from the shared catalog. */
export const FONT_STEPS: ReadonlyArray<{ id: FontSize; label: string; px: number }> = [
  { id: 'Small', label: 'Small', px: 13 },
  { id: 'Normal', label: 'Normal', px: 15 },
  { id: 'Large', label: 'Large', px: 18 },
  { id: 'Huge', label: 'Huge', px: 24 },
];

/** The pixel one step renders at, the core's own mapping. */
export function fontPx(size: FontSize): number {
  return FONT_STEPS.find((step) => step.id === size)?.px ?? 15;
}

/** The app palette the colour pickers offer: the skin swatches and ink. */
export const EDITOR_PALETTE: ReadonlyArray<{ label: string; hex: string }> = [
  { label: 'Iris', hex: '#7a6ff0' },
  { label: 'Blue', hex: '#0a59f7' },
  { label: 'Indigo', hex: '#6366f1' },
  { label: 'Rose', hex: '#f43f5e' },
  { label: 'Emerald', hex: '#10b981' },
  { label: 'Amber', hex: '#f59e0b' },
  { label: 'Violet', hex: '#8b5cf6' },
  { label: 'Orange', hex: '#f97316' },
  { label: 'Black', hex: '#000000' },
  { label: 'White', hex: '#ffffff' },
];

/** The font size mark: the attribute is one of the catalog's four words. */
export const FontSizeMark = Mark.create({
  name: 'fontSize',

  addAttributes() {
    return {
      size: {
        default: 'Normal',
        parseHTML: (element) => element.getAttribute('data-font-size') ?? 'Normal',
        renderHTML: (attributes) => ({
          'data-font-size': attributes.size,
          style: `font-size: ${fontPx(attributes.size as FontSize)}px`,
        }),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'span[data-font-size]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['span', mergeAttributes(HTMLAttributes), 0];
  },
});

/** The text colour mark over the app palette's `#rrggbb` values. */
export const ColorMark = Mark.create({
  name: 'color',

  addAttributes() {
    return {
      color: {
        default: null,
        parseHTML: (element) => element.getAttribute('data-text-color'),
        renderHTML: (attributes) => ({
          'data-text-color': attributes.color,
          style: `color: ${attributes.color}`,
        }),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'span[data-text-color]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['span', mergeAttributes(HTMLAttributes), 0];
  },
});

/** The highlight mark, the palette's background leg. */
export const HighlightMark = Mark.create({
  name: 'highlight',

  addAttributes() {
    return {
      color: {
        default: null,
        parseHTML: (element) => element.getAttribute('data-highlight'),
        renderHTML: (attributes) => ({
          'data-highlight': attributes.color,
          style: `background-color: ${attributes.color}`,
        }),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'span[data-highlight]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['span', mergeAttributes(HTMLAttributes), 0];
  },
});
