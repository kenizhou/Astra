// SPDX-License-Identifier: GPL-3.0-only
// The Shortcuts page: a static table of the shortcuts the shell wires
// today; Ctrl+K's search island heads the list, and every row renders its
// key cap, its action, and where the keys answer.
import { cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { ShortcutsDetail } from './ShortcutsDetail';

afterEach(cleanup);

describe('the Shortcuts detail', () => {
  it('lists Ctrl+K for the search island', () => {
    render(<ShortcutsDetail />);

    expect(screen.getByText('Ctrl+K')).toBeTruthy();
    expect(screen.getByText('Open the search island')).toBeTruthy();
  });

  it('lists the Escape behaviours that are wired', () => {
    render(<ShortcutsDetail />);

    expect(screen.getByText('Close this settings screen')).toBeTruthy();
    expect(screen.getByText('Close the folder drawer')).toBeTruthy();
    expect(screen.getByText('Close the open menu')).toBeTruthy();
  });

  it('renders one row per entry under the three column heads', () => {
    render(<ShortcutsDetail />);

    const table = screen.getByRole('table');
    const heads = within(table)
      .getAllByRole('columnheader')
      .map((head) => head.textContent);
    expect(heads).toEqual(['Shortcut', 'Action', 'Where']);
    expect(within(table).getAllByRole('row')).toHaveLength(12);
  });
});
