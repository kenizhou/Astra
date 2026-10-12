// SPDX-License-Identifier: GPL-3.0-only
// The recipient field: the finished recipients render as pills with
// per-recipient removal while the DOM input's value stays the one
// comma-separated string (pills are a view, never a second source of
// truth), completion asks on the trailing token and inserts bare addresses,
// the suggestions float over what follows without taking focus and only
// for the focused field, a blank token shows nothing, history-only entries
// are marked apart but never hidden, and a seeded field renders every
// supplied address as a pill with nothing in progress.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ peopleSuggestions: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  peopleSuggestions: ipc.peopleSuggestions,
}));

import type { RecipientSuggestion } from '@/services/ipc';

import { useState } from 'react';

import { RecipientField } from './RecipientField';
import { finishedRecipients, trailingToken } from './recipientTokens';

/** A controlled harness: the field's value lives in state, the way the
 * composer's store holds it, so a change or an accept lands in the prop. */
function Harness(props: { initial: string; label?: string }) {
  const [value, setValue] = useState(props.initial);
  return <RecipientField label={props.label ?? 'To'} value={value} onField={setValue} />;
}

function suggestion(over: Partial<RecipientSuggestion> = {}): RecipientSuggestion {
  return {
    email: 'bob@y.com',
    person_id: 4,
    display_name: 'Bob Why',
    sent_count: 2,
    last_sent: '2026-10-01T08:00:00Z',
    is_saved: true,
    ...over,
  };
}

beforeEach(() => {
  vi.useFakeTimers();
  ipc.peopleSuggestions.mockReset();
  ipc.peopleSuggestions.mockResolvedValue([]);
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

async function flush(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
  });
}

describe('the recipient field pills', () => {
  it('renders every supplied address of a seeded field as a pill, nothing in progress', () => {
    render(<RecipientField label="To" value="a@x.com, b@y.com" onField={() => undefined} />);

    const input = screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement;
    expect(input.value).toBe('a@x.com, b@y.com');
    expect(screen.getByText('a@x.com')).toBeTruthy();
    expect(screen.getByText('b@y.com')).toBeTruthy();
  });

  it('keeps the DOM input equal to the string while pills carry the finished recipients', async () => {
    ipc.peopleSuggestions.mockResolvedValue([suggestion()]);
    render(<Harness initial="a@x.com" />);
    const input = screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement;
    act(() => {
      input.focus();
    });
    await act(async () => {
      fireEvent.change(input, { target: { value: 'a@x.com, bo' } });
      vi.advanceTimersByTime(150);
    });
    await flush();

    fireEvent.click(screen.getByRole('option'));
    await act(async () => {
      vi.advanceTimersByTime(50);
    });

    const after = screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement;
    expect(after.value).toBe('a@x.com, bob@y.com, ');
    expect(
      finishedRecipients(after.value, false).join('|') + '|' + trailingToken(after.value).trim(),
    ).toBe('a@x.com|bob@y.com|');
  });

  it('removes one recipient from its pill, rebuilding the string', () => {
    render(<Harness initial="a@x.com, b@y.com" />);

    fireEvent.click(screen.getByRole('button', { name: 'Remove a@x.com' }));

    const input = screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement;
    expect(input.value).toBe('b@y.com');
  });
});

describe('the recipient field completion', () => {
  it('asks on the trailing token after the pause', async () => {
    ipc.peopleSuggestions.mockResolvedValue([suggestion()]);
    render(<RecipientField label="To" value="" onField={() => undefined} />);
    const input = screen.getByRole('textbox', { name: 'To' });
    fireEvent.focus(input);

    await act(async () => {
      fireEvent.change(input, { target: { value: 'a@x.com, bo' } });
      fireEvent.change(input, { target: { value: 'a@x.com, bob' } });
    });

    expect(ipc.peopleSuggestions).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(150);
    });

    expect(ipc.peopleSuggestions).toHaveBeenCalledTimes(1);
    expect(ipc.peopleSuggestions).toHaveBeenLastCalledWith('bob');
  });

  it('shows nothing for a blank token without asking', async () => {
    render(<RecipientField label="To" value="" onField={() => undefined} />);
    const input = screen.getByRole('textbox', { name: 'To' });
    fireEvent.focus(input);

    await act(async () => {
      fireEvent.change(input, { target: { value: '   ' } });
      vi.advanceTimersByTime(150);
    });
    await flush();

    expect(ipc.peopleSuggestions).not.toHaveBeenCalled();
    expect(screen.queryByRole('listbox')).toBeNull();
  });

  it('inserts the bare address, never the display name, and empties the token', async () => {
    ipc.peopleSuggestions.mockResolvedValue([
      suggestion({ email: 'bob@y.com', display_name: 'Bob Why' }),
    ]);
    render(<Harness initial="a@x.com" />);
    const input = screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement;
    act(() => {
      input.focus();
    });
    await act(async () => {
      fireEvent.change(input, { target: { value: 'a@x.com, bo' } });
      vi.advanceTimersByTime(150);
    });
    await flush();

    fireEvent.click(screen.getByRole('option'));
    await act(async () => {
      vi.advanceTimersByTime(50);
    });

    const after = screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement;
    expect(after.value).toBe('a@x.com, bob@y.com, ');
    expect(after.value).not.toContain('Bob Why');
    expect(trailingToken(after.value).trim()).toBe('');
  });

  it('moves the caret to the end after an accept', async () => {
    ipc.peopleSuggestions.mockResolvedValue([suggestion()]);
    render(<Harness initial="a@x.com" />);
    const input = screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement;
    act(() => {
      input.focus();
    });
    await act(async () => {
      fireEvent.change(input, { target: { value: 'a@x.com, bo' } });
      vi.advanceTimersByTime(150);
    });
    await flush();

    fireEvent.click(screen.getByRole('option'));
    await act(async () => {
      vi.advanceTimersByTime(50);
    });

    expect(input.selectionStart).toBe(input.value.length);
    expect(document.activeElement).toBe(input);
  });

  it('renders the engine order untouched, marking the history-only entries apart', async () => {
    ipc.peopleSuggestions.mockResolvedValue([
      suggestion({ email: 'saved@y.com', display_name: 'Saved Card', is_saved: true }),
      suggestion({
        email: 'history@y.com',
        display_name: 'Only History',
        is_saved: false,
        person_id: undefined,
        provenance: [{ account: 'work', contact: 'c-9' }],
      }),
    ]);
    render(<RecipientField label="To" value="" onField={() => undefined} />);
    const input = screen.getByRole('textbox', { name: 'To' });
    fireEvent.focus(input);
    await act(async () => {
      fireEvent.change(input, { target: { value: 'why' } });
      vi.advanceTimersByTime(150);
    });
    await flush();

    const options = screen.getAllByRole('option');
    expect(options[0]?.textContent).toContain('saved@y.com');
    expect(options[1]?.textContent).toContain('history@y.com');
    expect(options[0]?.textContent).not.toContain('From sent mail');
    expect(options[1]?.textContent).toContain('From sent mail');
  });
});

describe('the floating suggestions list', () => {
  it('floats over what follows, opaque and shadowed, only for the focused field', async () => {
    ipc.peopleSuggestions.mockResolvedValue([suggestion()]);
    render(
      <div>
        <RecipientField label="To" value="" onField={() => undefined} />
        <RecipientField label="Cc" value="" onField={() => undefined} />
        <p>the form below</p>
      </div>,
    );
    const to = screen.getByRole('textbox', { name: 'To' });
    fireEvent.focus(to);
    await act(async () => {
      fireEvent.change(to, { target: { value: 'bo' } });
      vi.advanceTimersByTime(150);
    });
    await flush();

    const list = screen.getByRole('listbox');
    expect(list.className).toContain('absolute');
    expect(list.className).toContain('shadow');
    expect(list.className).toContain('surface-floating');

    fireEvent.blur(to);
    expect(screen.queryByRole('listbox')).toBeNull();

    const cc = screen.getByRole('textbox', { name: 'Cc' });
    fireEvent.focus(cc);
    await act(async () => {
      fireEvent.change(cc, { target: { value: 'bo' } });
      vi.advanceTimersByTime(150);
    });
    await flush();
    expect(screen.getAllByRole('listbox')).toHaveLength(1);
  });

  it('closes on field blur even mid pause', async () => {
    ipc.peopleSuggestions.mockResolvedValue([suggestion()]);
    render(<RecipientField label="To" value="" onField={() => undefined} />);
    const input = screen.getByRole('textbox', { name: 'To' });
    fireEvent.focus(input);
    await act(async () => {
      fireEvent.change(input, { target: { value: 'bo' } });
    });

    fireEvent.blur(input);
    await act(async () => {
      vi.advanceTimersByTime(150);
    });

    expect(ipc.peopleSuggestions).not.toHaveBeenCalled();
    expect(screen.queryByRole('listbox')).toBeNull();
  });

  it('takes no focus when a suggestion is pressed', async () => {
    ipc.peopleSuggestions.mockResolvedValue([suggestion()]);
    render(<RecipientField label="To" value="" onField={() => undefined} />);
    const input = screen.getByRole('textbox', { name: 'To' });
    act(() => {
      input.focus();
    });
    await act(async () => {
      fireEvent.change(input, { target: { value: 'bo' } });
      vi.advanceTimersByTime(150);
    });
    await flush();

    fireEvent.mouseDown(screen.getByRole('option'));
    expect(document.activeElement).toBe(input);
  });

  it('disarms a pending ask when the field goes away', async () => {
    ipc.peopleSuggestions.mockResolvedValue([suggestion()]);
    const { unmount } = render(<RecipientField label="To" value="" onField={() => undefined} />);
    const input = screen.getByRole('textbox', { name: 'To' });
    fireEvent.focus(input);
    await act(async () => {
      fireEvent.change(input, { target: { value: 'bo' } });
    });

    unmount();
    await act(async () => {
      vi.advanceTimersByTime(150);
    });

    expect(ipc.peopleSuggestions).not.toHaveBeenCalled();
  });
});
