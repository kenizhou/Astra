// SPDX-License-Identifier: GPL-3.0-only
// The translate block: the six languages with English standing selected,
// the on-demand run into the chosen language with its loading state, the
// result in a collapsible block whose cached instant return re-expands
// without a second call, the empty answer read as the failure the host
// cannot name beside the retry, the refused run's own sentence, and the
// reset a newly opened message takes.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ai = vi.hoisted(() => ({
  translateMessage: vi.fn(),
}));

vi.mock('@/services/ai', () => ai);

import { TranslateBlock } from './TranslateBlock';
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

/** The select's options in their fixed order. */
const LANGUAGES = ['english', 'chinese', 'german', 'french', 'japanese', 'spanish'];

beforeEach(() => {
  ai.translateMessage.mockReset();
});

afterEach(cleanup);

describe('the translate block', () => {
  it('offers the six languages with english standing selected', () => {
    useReadingStore.getState().applySnapshot(snapshot());
    render(<TranslateBlock />);

    const select = screen.getByRole('combobox', { name: 'Translate to' }) as HTMLSelectElement;
    expect(select.value).toBe('english');
    expect(Array.from(select.options).map((option) => option.value)).toEqual(LANGUAGES);
  });

  it('translates on demand, loading while the run is in flight', async () => {
    useReadingStore.getState().applySnapshot(snapshot());
    let resolveRun: (text: string) => void = () => undefined;
    ai.translateMessage.mockImplementation(
      () =>
        new Promise<string>((resolve) => {
          resolveRun = resolve;
        }),
    );
    render(<TranslateBlock />);

    fireEvent.click(screen.getByRole('button', { name: 'Translate' }));
    expect(ai.translateMessage).toHaveBeenCalledWith(
      { account: 'work', key: 'imap:v1:u1' },
      'english',
    );
    const loading = screen.getByRole('button', { name: 'Translating' }) as HTMLButtonElement;
    expect(loading.disabled).toBe(true);

    resolveRun('Der Bericht ist da.');
    await waitFor(() => {
      expect(screen.getByText('Der Bericht ist da.')).toBeTruthy();
    });
    expect(screen.getByText('Translation (English)')).toBeTruthy();
  });

  it('carries the chosen language into the run', async () => {
    useReadingStore.getState().applySnapshot(snapshot());
    ai.translateMessage.mockResolvedValue('Bonjour.');
    render(<TranslateBlock />);

    fireEvent.change(screen.getByRole('combobox', { name: 'Translate to' }), {
      target: { value: 'french' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Translate' }));

    expect(ai.translateMessage).toHaveBeenCalledWith(
      { account: 'work', key: 'imap:v1:u1' },
      'french',
    );
    await waitFor(() => {
      expect(screen.getByText('Translation (French)')).toBeTruthy();
    });
  });

  it('collapses and re-expands its block without a second call', async () => {
    useReadingStore.getState().applySnapshot(snapshot());
    ai.translateMessage.mockResolvedValue('Der Bericht ist da.');
    render(<TranslateBlock />);

    fireEvent.click(screen.getByRole('button', { name: 'Translate' }));
    await waitFor(() => {
      expect(screen.getByText('Der Bericht ist da.')).toBeTruthy();
    });

    fireEvent.click(screen.getByRole('button', { name: 'Hide translation' }));
    expect(screen.queryByText('Der Bericht ist da.')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Show translation' }));
    expect(screen.getByText('Der Bericht ist da.')).toBeTruthy();
    expect(ai.translateMessage).toHaveBeenCalledOnce();
  });

  it('runs fresh for another language while the first result stands', async () => {
    useReadingStore.getState().applySnapshot(snapshot());
    ai.translateMessage.mockResolvedValue('Der Bericht ist da.');
    render(<TranslateBlock />);

    fireEvent.click(screen.getByRole('button', { name: 'Translate' }));
    await waitFor(() => {
      expect(screen.getByText('Der Bericht ist da.')).toBeTruthy();
    });

    ai.translateMessage.mockResolvedValue('Bonjour.');
    fireEvent.change(screen.getByRole('combobox', { name: 'Translate to' }), {
      target: { value: 'french' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Translate' }));

    expect(ai.translateMessage).toHaveBeenCalledTimes(2);
    await waitFor(() => {
      expect(screen.getByText('Bonjour.')).toBeTruthy();
    });
    expect(screen.queryByText('Der Bericht ist da.')).toBeNull();
  });

  it('reads an empty answer as the failure the host cannot name, with its retry', async () => {
    useReadingStore.getState().applySnapshot(snapshot());
    ai.translateMessage.mockResolvedValueOnce('').mockResolvedValueOnce('Der Bericht ist da.');
    render(<TranslateBlock />);

    fireEvent.click(screen.getByRole('button', { name: 'Translate' }));
    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeTruthy();
    });
    expect(screen.getByRole('alert').textContent).toBe('The translation is unavailable.');
    expect(screen.queryByText('Translation (English)')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Translate' }));
    await waitFor(() => {
      expect(screen.getByText('Der Bericht ist da.')).toBeTruthy();
    });
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('surfaces a refused run beside the retry it offers', async () => {
    useReadingStore.getState().applySnapshot(snapshot());
    ai.translateMessage
      .mockRejectedValueOnce('the ai surface refused: the endpoint is down')
      .mockResolvedValueOnce('Der Bericht ist da.');
    render(<TranslateBlock />);

    fireEvent.click(screen.getByRole('button', { name: 'Translate' }));
    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeTruthy();
    });
    expect(screen.getByRole('alert').textContent).toContain(
      'the ai surface refused: the endpoint is down',
    );

    fireEvent.click(screen.getByRole('button', { name: 'Translate' }));
    await waitFor(() => {
      expect(screen.getByText('Der Bericht ist da.')).toBeTruthy();
    });
    expect(ai.translateMessage).toHaveBeenCalledTimes(2);
  });

  it('resets when another message opens', async () => {
    useReadingStore.getState().applySnapshot(snapshot('imap:v1:u1'));
    ai.translateMessage.mockResolvedValue('Der Bericht ist da.');
    render(<TranslateBlock />);

    fireEvent.change(screen.getByRole('combobox', { name: 'Translate to' }), {
      target: { value: 'german' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Translate' }));
    await waitFor(() => {
      expect(screen.getByText('Der Bericht ist da.')).toBeTruthy();
    });

    act(() => {
      useReadingStore.getState().applySnapshot(snapshot('imap:v1:u2'));
    });

    expect(screen.queryByText('Der Bericht ist da.')).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(
      (screen.getByRole('combobox', { name: 'Translate to' }) as HTMLSelectElement).value,
    ).toBe('english');
    expect(screen.getByRole('button', { name: 'Translate' })).toBeTruthy();
  });

  it('never settles a run under a message the reader switched away from', async () => {
    useReadingStore.getState().applySnapshot(snapshot('imap:v1:u1'));
    let resolveRun: (text: string) => void = () => undefined;
    ai.translateMessage.mockImplementation(
      () =>
        new Promise<string>((resolve) => {
          resolveRun = resolve;
        }),
    );
    render(<TranslateBlock />);

    fireEvent.click(screen.getByRole('button', { name: 'Translate' }));
    act(() => {
      useReadingStore.getState().applySnapshot(snapshot('imap:v1:u2'));
    });

    resolveRun('Der Bericht ist da.');
    await waitFor(() => {
      expect(ai.translateMessage).toHaveBeenCalledOnce();
    });
    expect(screen.queryByText('Der Bericht ist da.')).toBeNull();
    expect(screen.getByRole('button', { name: 'Translate' })).toBeTruthy();
  });

  it('stands absent with nothing open', () => {
    useReadingStore.getState().applySnapshot(null);
    const { container } = render(<TranslateBlock />);

    expect(container.firstChild).toBeNull();
  });
});
