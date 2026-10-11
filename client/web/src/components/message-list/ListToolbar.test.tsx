// SPDX-License-Identifier: GPL-3.0-only
// The list toolbar's split control: three segmented buttons carrying the
// counts of the snapshot's rows per side, the pressed state following the
// chosen filter, and every click crossing back with the side it names.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ListToolbar } from './ListToolbar';

afterEach(cleanup);

describe('the split control', () => {
  it('renders the three sides with their count badges', () => {
    render(
      <ListToolbar
        folderName="Inbox"
        viewMode="flat"
        splitFilter="all"
        splitCounts={{ all: 12, actioned: 7, fyi: 3 }}
        onSplitFilter={() => undefined}
        search={null}
      />,
    );

    expect(screen.getByRole('group', { name: 'Split' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'All 12' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Actioned 7' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'FYI 3' })).toBeTruthy();
  });

  it('presses the chosen side alone', () => {
    render(
      <ListToolbar
        folderName="Inbox"
        viewMode="flat"
        splitFilter="fyi"
        splitCounts={{ all: 12, actioned: 7, fyi: 3 }}
        onSplitFilter={() => undefined}
        search={null}
      />,
    );

    expect(screen.getByRole('button', { name: 'All 12' }).getAttribute('aria-pressed')).toBe('false');
    expect(screen.getByRole('button', { name: 'Actioned 7' }).getAttribute('aria-pressed')).toBe('false');
    expect(screen.getByRole('button', { name: 'FYI 3' }).getAttribute('aria-pressed')).toBe('true');
  });

  it('crosses the clicked side back', () => {
    const onSplitFilter = vi.fn();
    render(
      <ListToolbar
        folderName="Inbox"
        viewMode="flat"
        splitFilter="all"
        splitCounts={{ all: 12, actioned: 7, fyi: 3 }}
        onSplitFilter={onSplitFilter}
        search={null}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Actioned 7' }));
    expect(onSplitFilter).toHaveBeenCalledWith('actioned');

    fireEvent.click(screen.getByRole('button', { name: 'FYI 3' }));
    expect(onSplitFilter).toHaveBeenCalledWith('fyi');

    fireEvent.click(screen.getByRole('button', { name: 'All 12' }));
    expect(onSplitFilter).toHaveBeenCalledWith('all');
  });

  it('keeps the view mode control beside the split', () => {
    render(
      <ListToolbar
        folderName="Inbox"
        viewMode="threaded"
        splitFilter="all"
        splitCounts={{ all: 0, actioned: 0, fyi: 0 }}
        onSplitFilter={() => undefined}
        search={null}
      />,
    );

    expect(screen.getByRole('button', { name: 'Threaded' }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('button', { name: 'All 0' })).toBeTruthy();
  });
});
