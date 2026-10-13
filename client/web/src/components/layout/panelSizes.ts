// SPDX-License-Identifier: GPL-3.0-only
// react-resizable-panels v4 size semantics: a bare number is pixels, a
// string is a percentage. The shell's layouts speak percentages, so every
// size crosses through pct().
export const pct = (n: number): string => `${n}%`;
