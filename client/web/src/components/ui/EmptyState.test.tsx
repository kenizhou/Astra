// SPDX-License-Identifier: GPL-3.0-only
// The state family's contract: one EmptyState renders its glyph, message,
// and optional action; the skeleton rows follow the density axis.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { EmptyState, SkeletonRows } from './EmptyState';

afterEach(cleanup);

describe('EmptyState', () => {
  it('renders the glyph, the message, and no action when none is given', () => {
    render(<EmptyState glyph={<span data-testid="glyph" />} message="Select a folder to read its mail." />);

    expect(screen.getByTestId('glyph')).toBeTruthy();
    expect(screen.getByText('Select a folder to read its mail.')).toBeTruthy();
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('renders the action and fires it', () => {
    const onAction = vi.fn();
    render(
      <EmptyState
        glyph={<span data-testid="glyph" />}
        message="No messages in this folder."
        action={{ label: 'Refresh', onClick: onAction }}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    expect(onAction).toHaveBeenCalledTimes(1);
  });
});

describe('SkeletonRows', () => {
  it('renders three bars per pane, on the density axis', () => {
    const { container } = render(<SkeletonRows />);

    const bars = container.querySelectorAll('[data-skeleton-bar]');
    expect(bars).toHaveLength(3);
    for (const bar of bars) {
      expect((bar as HTMLElement).style.minHeight).toBe('var(--row-h)');
    }
  });
});
