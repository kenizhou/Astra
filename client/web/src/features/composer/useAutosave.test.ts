// SPDX-License-Identifier: GPL-3.0-only
// The idle autosave hook: thirty seconds of quiet after the last edit is
// when the save dispatches, an edit inside the window restarts the clock,
// an unchanged composer costs no dispatch, and an unmounted composer fires
// nothing. The idle span is a seam so the tests run on milliseconds.
import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { DRAFT_IDLE_MS, useAutosave } from './useAutosave';

const onIdle = vi.fn();
const isDirty = vi.fn(() => true);

beforeEach(() => {
  vi.useFakeTimers();
  onIdle.mockClear();
  isDirty.mockClear();
  isDirty.mockReturnValue(true);
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe('the idle autosave', () => {
  it('seams the idle span at thirty seconds', () => {
    expect(DRAFT_IDLE_MS).toBe(30_000);
  });

  it('fires once after the idle and not a moment before', () => {
    const { result } = renderHook(() => useAutosave({ isDirty, onIdle }));

    act(() => result.current.noteEdit());
    act(() => vi.advanceTimersByTime(29_999));
    expect(onIdle).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(1));
    expect(onIdle).toHaveBeenCalledTimes(1);
  });

  it('restarts the clock with every edit', () => {
    const { result } = renderHook(() => useAutosave({ isDirty, onIdle }));

    act(() => result.current.noteEdit());
    act(() => vi.advanceTimersByTime(20_000));
    act(() => result.current.noteEdit());
    act(() => vi.advanceTimersByTime(29_999));
    expect(onIdle).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(1));
    expect(onIdle).toHaveBeenCalledTimes(1);

    act(() => vi.advanceTimersByTime(60_000));
    expect(onIdle).toHaveBeenCalledTimes(1);
  });

  it('skips the dispatch when nothing changed', () => {
    isDirty.mockReturnValue(false);
    const { result } = renderHook(() => useAutosave({ isDirty, onIdle }));

    act(() => result.current.noteEdit());
    act(() => vi.advanceTimersByTime(30_000));

    expect(onIdle).not.toHaveBeenCalled();
  });

  it('reads the dirtiness at fire time, not at edit time', () => {
    isDirty.mockReturnValue(true);
    const { result } = renderHook(() => useAutosave({ isDirty, onIdle }));

    act(() => result.current.noteEdit());
    isDirty.mockReturnValue(false);
    act(() => vi.advanceTimersByTime(30_000));

    expect(onIdle).not.toHaveBeenCalled();
  });

  it('fires nothing once unmounted', () => {
    const { result, unmount } = renderHook(() => useAutosave({ isDirty, onIdle }));

    act(() => result.current.noteEdit());
    unmount();
    act(() => vi.advanceTimersByTime(60_000));

    expect(onIdle).not.toHaveBeenCalled();
  });

  it('honours the idle seam for a shorter window', () => {
    const { result } = renderHook(() => useAutosave({ isDirty, onIdle, idleMs: 100 }));

    act(() => result.current.noteEdit());
    act(() => vi.advanceTimersByTime(99));
    expect(onIdle).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(1));
    expect(onIdle).toHaveBeenCalledTimes(1);
  });
});
