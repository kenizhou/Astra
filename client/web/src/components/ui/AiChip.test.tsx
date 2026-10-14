// SPDX-License-Identifier: GPL-3.0-only
// The AiChip contract: provenance text, the five-state lifecycle, and the
// explicit approve action, with the retry only on a failed generation.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { AiChip } from './AiChip';

afterEach(cleanup);

describe('AiChip', () => {
  it('renders its label and provenance', () => {
    render(<AiChip label="Summarise" provenance="from this thread" state="pending" />);

    expect(screen.getByText('Summarise')).toBeTruthy();
    expect(screen.getByText('from this thread')).toBeTruthy();
  });

  it('offers the approve action only in the approve state', () => {
    const onApprove = vi.fn();
    render(<AiChip label="Draft reply" provenance="from this thread" state="approve" onApprove={onApprove} />);

    fireEvent.click(screen.getByRole('button', { name: /Draft reply/ }));
    expect(onApprove).toHaveBeenCalledTimes(1);
  });

  it('offers the retry only when the generation failed', () => {
    const onRetry = vi.fn();
    render(<AiChip label="Summarise" provenance="from this thread" state="failed" onRetry={onRetry} />);

    fireEvent.click(screen.getByRole('button', { name: /retry/i }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('is inert while generating and quiet once approved', () => {
    render(<AiChip label="Summarise" provenance="from this thread" state="generating" />);
    expect(screen.queryByRole('button')).toBeNull();

    render(<AiChip label="Summarise" provenance="from this thread" state="approved" />);
    expect(screen.queryByRole('button', { name: /approve/i })).toBeNull();
  });
});
