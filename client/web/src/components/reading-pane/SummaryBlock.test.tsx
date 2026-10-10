// SPDX-License-Identifier: GPL-3.0-only
// The summarise block: the button and its loading state, the summary in its
// collapsible block (a cached instant return never spends a second call on
// re-expanding), the failure sentence a refused run surfaces with its retry,
// and the reset a newly opened message takes.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ai = vi.hoisted(() => ({
  summariseMessage: vi.fn(),
}));

vi.mock('@/services/ai', () => ai);

import { SummaryBlock } from './SummaryBlock';
import { useReadingStore } from '@/stores/readingStore';
import type { ReadingSnapshot } from '@/services/ipc';

/** The open message's snapshot, overridable per test. */
function snapshot(key = 'imap:v1:u1'): ReadingSnapshot {
  return {
    message: { account: 'work', key },
    subject: 'Quarterly report',
    from_name: 'Ada Lovelace',
    from_address: 'ada@example.test',
    sent_at: '2026-10-09T08:30:00Z',
    remote_image_count: 0,
  };
}

beforeEach(() => {
  ai.summariseMessage.mockReset();
});

afterEach(cleanup);

describe('the summarise block', () => {
  it('summarises on demand, loading while the run is in flight', async () => {
    useReadingStore.getState().applySnapshot(snapshot());
    let resolveRun: (summary: string) => void = () => undefined;
    ai.summariseMessage.mockImplementation(
      () =>
        new Promise<string>((resolve) => {
          resolveRun = resolve;
        }),
    );
    render(<SummaryBlock />);

    fireEvent.click(screen.getByRole('button', { name: 'Summarise' }));
    expect(ai.summariseMessage).toHaveBeenCalledWith({ account: 'work', key: 'imap:v1:u1' });
    const loading = screen.getByRole('button', { name: 'Summarising' }) as HTMLButtonElement;
    expect(loading.disabled).toBe(true);

    resolveRun('The launch is ready.');
    await waitFor(() => {
      expect(screen.getByText('The launch is ready.')).toBeTruthy();
    });
  });

  it('collapses and re-expands its block without a second call', async () => {
    useReadingStore.getState().applySnapshot(snapshot());
    ai.summariseMessage.mockResolvedValue('The launch is ready.');
    render(<SummaryBlock />);

    fireEvent.click(screen.getByRole('button', { name: 'Summarise' }));
    await waitFor(() => {
      expect(screen.getByText('The launch is ready.')).toBeTruthy();
    });

    fireEvent.click(screen.getByRole('button', { name: 'Hide summary' }));
    expect(screen.queryByText('The launch is ready.')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Show summary' }));
    expect(screen.getByText('The launch is ready.')).toBeTruthy();
    expect(ai.summariseMessage).toHaveBeenCalledOnce();
  });

  it('surfaces a failed run beside the retry it offers', async () => {
    useReadingStore.getState().applySnapshot(snapshot());
    ai.summariseMessage
      .mockRejectedValueOnce('the ai surface refused: the endpoint is down')
      .mockResolvedValueOnce('The launch is ready.');
    render(<SummaryBlock />);

    fireEvent.click(screen.getByRole('button', { name: 'Summarise' }));
    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeTruthy();
    });
    expect(screen.getByRole('alert').textContent).toContain(
      'the ai surface refused: the endpoint is down',
    );

    fireEvent.click(screen.getByRole('button', { name: 'Summarise' }));
    await waitFor(() => {
      expect(screen.getByText('The launch is ready.')).toBeTruthy();
    });
    expect(ai.summariseMessage).toHaveBeenCalledTimes(2);
  });

  it('resets when another message opens', async () => {
    useReadingStore.getState().applySnapshot(snapshot('imap:v1:u1'));
    ai.summariseMessage.mockResolvedValue('The launch is ready.');
    render(<SummaryBlock />);

    fireEvent.click(screen.getByRole('button', { name: 'Summarise' }));
    await waitFor(() => {
      expect(screen.getByText('The launch is ready.')).toBeTruthy();
    });

    act(() => {
      useReadingStore.getState().applySnapshot(snapshot('imap:v1:u2'));
    });

    expect(screen.queryByText('The launch is ready.')).toBeNull();
    expect(screen.getByRole('button', { name: 'Summarise' })).toBeTruthy();
  });

  it('never settles a run under a message the reader switched away from', async () => {
    useReadingStore.getState().applySnapshot(snapshot('imap:v1:u1'));
    let resolveRun: (summary: string) => void = () => undefined;
    ai.summariseMessage.mockImplementation(
      () =>
        new Promise<string>((resolve) => {
          resolveRun = resolve;
        }),
    );
    render(<SummaryBlock />);

    fireEvent.click(screen.getByRole('button', { name: 'Summarise' }));
    act(() => {
      useReadingStore.getState().applySnapshot(snapshot('imap:v1:u2'));
    });
    expect(screen.getByRole('button', { name: 'Summarise' })).toBeTruthy();

    resolveRun('The launch is ready.');
    await waitFor(() => {
      expect(ai.summariseMessage).toHaveBeenCalledOnce();
    });
    expect(screen.queryByText('The launch is ready.')).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByRole('button', { name: 'Summarise' })).toBeTruthy();

    // The switched-away message's next open summarises fresh, unpoisoned.
    ai.summariseMessage.mockResolvedValue('A fresh summary.');
    act(() => {
      useReadingStore.getState().applySnapshot(snapshot('imap:v1:u1'));
    });
    fireEvent.click(screen.getByRole('button', { name: 'Summarise' }));
    await waitFor(() => {
      expect(screen.getByText('A fresh summary.')).toBeTruthy();
    });
  });

  it('never surfaces a failure that settles under a newer message', async () => {
    useReadingStore.getState().applySnapshot(snapshot('imap:v1:u1'));
    let rejectRun: (err: Error) => void = () => undefined;
    ai.summariseMessage.mockImplementation(
      () =>
        new Promise<string>((_resolve, reject) => {
          rejectRun = reject;
        }),
    );
    render(<SummaryBlock />);

    fireEvent.click(screen.getByRole('button', { name: 'Summarise' }));
    act(() => {
      useReadingStore.getState().applySnapshot(snapshot('imap:v1:u2'));
    });
    rejectRun(new Error('the endpoint is down'));
    await waitFor(() => {
      expect(ai.summariseMessage).toHaveBeenCalledOnce();
    });
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByRole('button', { name: 'Summarise' })).toBeTruthy();
  });

  it('stands absent with nothing open', () => {
    useReadingStore.getState().applySnapshot(null);
    const { container } = render(<SummaryBlock />);

    expect(container.firstChild).toBeNull();
  });
});
