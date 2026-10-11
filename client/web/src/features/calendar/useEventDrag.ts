// SPDX-License-Identifier: GPL-3.0-only
// The drag gesture on the time grids: a pointer down on a movable block and
// up somewhere else, converted through the shared geometry into the drag
// payload the move intent takes, both ends snapped to the slot grid. The
// maths itself lives in calendarRange and gridGeometry; this hook only holds
// the gesture and dispatches its result.

import { useCallback, useRef, useState, type RefObject } from 'react';

import { dispatch, intents } from '@/services/ipc';
import type { OccurrenceRow } from '@/services/ipc';

import { dragFromGrid } from './calendarRange';
import { clientToGrid, type GridPoint } from './gridGeometry';

/** One gesture in flight. */
interface DragState {
  /** The row being dragged. */
  row: OccurrenceRow;
  /** Where the gesture began, on the grid. */
  from: GridPoint;
  /** Where the pointer now holds, on the grid. */
  to: GridPoint;
}

/** The drag plumbing a time grid binds to its columns container. */
export interface EventDrag {
  /** The gesture in flight, for the ghost the grid draws while it lasts. */
  dragging: DragState | null;
  /** Starts a drag of `row` from a pointer down on its block. */
  beginDrag: (row: OccurrenceRow, event: React.PointerEvent) => void;
  /**
   * Whether the just-finished gesture was a real drag, consuming the click
   * that follows it so a moved block does not also open its editor.
   */
  consumeClick: () => boolean;
  /** The handlers the columns container binds. */
  handlers: {
    onPointerMove: (event: React.PointerEvent) => void;
    onPointerUp: (event: React.PointerEvent) => void;
    onPointerCancel: () => void;
  };
}

/**
 * The drag gesture over a grid of `dayCount` day columns. The hook owns the
 * grid's ref, which the grid attaches to its columns container so every
 * pointer position converts against the box the user is actually looking at,
 * read at event time rather than render time.
 */
export function useEventDrag(dayCount: number): EventDrag & {
  gridRef: RefObject<HTMLDivElement | null>;
} {
  const [dragging, setDragging] = useState<DragState | null>(null);
  const gesture = useRef<DragState | null>(null);
  const clickToConsume = useRef(false);
  const gridRef = useRef<HTMLDivElement | null>(null);

  const beginDrag = useCallback(
    (row: OccurrenceRow, event: React.PointerEvent) => {
      const grid = gridRef.current;
      if (!row.can_move || grid === null) {
        return;
      }
      const rect = grid.getBoundingClientRect();
      const from = clientToGrid(event.clientX, event.clientY, rect, dayCount);
      gesture.current = { row, from, to: from };
      setDragging(gesture.current);
    },
    [dayCount],
  );

  /** Reads the pointer's latest position into the gesture. */
  const track = useCallback(
    (event: React.PointerEvent) => {
      const grid = gridRef.current;
      if (gesture.current === null || grid === null) {
        return;
      }
      const rect = grid.getBoundingClientRect();
      const to = clientToGrid(event.clientX, event.clientY, rect, dayCount);
      gesture.current = { ...gesture.current, to };
    },
    [dayCount],
  );

  const onPointerMove = useCallback(
    (event: React.PointerEvent) => {
      track(event);
      if (gesture.current !== null) {
        setDragging(gesture.current);
      }
    },
    [track],
  );

  const commit = useCallback(() => {
    const state = gesture.current;
    gesture.current = null;
    setDragging(null);
    if (state === null) {
      return;
    }
    const delta = dragFromGrid(state.from, state.to);
    if (delta === null || (delta.days === 0 && delta.minutes === 0)) {
      return;
    }
    clickToConsume.current = true;
    void dispatch(
      intents.moveCalendarEvent(
        { account: state.row.account, key: state.row.event },
        {
          edge: 'whole',
          days: delta.days,
          minutes: delta.minutes,
          ...(state.row.recurring && state.row.occurrence_start !== undefined
            ? { occurrence: state.row.occurrence_start }
            : {}),
        },
      ),
    );
  }, []);

  const onPointerUp = useCallback(
    (event: React.PointerEvent) => {
      // A drop with no move between still lands where the pointer let go.
      track(event);
      commit();
    },
    [track, commit],
  );

  const onPointerCancel = useCallback(() => {
    gesture.current = null;
    setDragging(null);
  }, []);

  const consumeClick = useCallback(() => {
    const consumed = clickToConsume.current;
    clickToConsume.current = false;
    return consumed;
  }, []);

  return {
    gridRef,
    dragging,
    beginDrag,
    consumeClick,
    handlers: { onPointerMove, onPointerUp, onPointerCancel },
  };
}
