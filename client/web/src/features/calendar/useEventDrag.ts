// SPDX-License-Identifier: GPL-3.0-only
// The drag gestures on the time grids. A pointer down on a movable block and
// up somewhere else converts through the shared geometry into the drag
// payload the move intent takes, both ends snapped to the slot grid. A
// pointer down on empty time begins a drag-create instead: the gesture
// previews the slot it would claim, one hour at the least like the
// mail-calendar grids, and its pointer up hands the slot to the grid's
// create callback; a press that never leaves its slot is a click and
// creates nothing. The maths itself lives in calendarRange and
// gridGeometry; this hook only holds the gestures and dispatches their
// results. A committed move holds its moved position optimistically until
// the snapshot's next publish replaces the rows it was drawn from.

import { useCallback, useEffect, useRef, useState, type RefObject } from 'react';

import { dispatch, intents } from '@/services/ipc';
import type { OccurrenceRow } from '@/services/ipc';

import { dragFromGrid, snapToSlot } from './calendarRange';
import { clientToGrid, type GridPoint } from './gridGeometry';

/** One move gesture in flight. */
interface DragState {
  /** The row being dragged. */
  row: OccurrenceRow;
  /** Where the gesture began, on the grid. */
  from: GridPoint;
  /** Where the pointer now holds, on the grid. */
  to: GridPoint;
}

/** One create gesture in flight: the anchor and the pointer's position. */
interface CreateGesture {
  from: GridPoint;
  to: GridPoint;
}

/** The slot a committed drag-create asks the editor to open prefilled with. */
export interface CreateSlot {
  /** The civil day key of the column the drag began in. */
  day: string;
  /** The slot's start, minutes since midnight, snapped to the slot grid. */
  startMinutes: number;
  /** The slot's end, minutes since midnight, snapped to the slot grid. */
  endMinutes: number;
}

/** The slot a create gesture claims: the anchor's hour band stretched to the
 * pointer, so the smallest real drag still creates an hour. */
function createSlotOf(gesture: CreateGesture, day: string): CreateSlot {
  const band = Math.min(Math.floor(gesture.from.minutes / 60) * 60, 23 * 60);
  const pointer = snapToSlot(gesture.to.minutes);
  return {
    day,
    startMinutes: Math.min(band, pointer),
    endMinutes: Math.min(Math.max(band + 60, pointer), 24 * 60),
  };
}

/** Whether the gesture ever left the slot it began in; a stay is a click. */
function moved(gesture: CreateGesture): boolean {
  return snapToSlot(gesture.from.minutes) !== snapToSlot(gesture.to.minutes);
}

/** A committed drag the next publish has not confirmed or refused yet. */
export interface PendingMove {
  /** The row as it stood when the drag committed. */
  row: OccurrenceRow;
  /** The drag's day delta. */
  days: number;
  /** The drag's minute delta. */
  minutes: number;
}

/** The drag plumbing a time grid binds to its columns container. */
export interface EventDrag {
  /** The move gesture in flight, for the ghost the grid draws while it lasts. */
  dragging: DragState | null;
  /** The create gesture in flight, for the preview band the grid draws. */
  creating: CreateSlot | null;
  /** The committed drag whose moved position renders until the next publish. */
  pending: PendingMove | null;
  /** Starts a drag of `row` from a pointer down on its block. */
  beginDrag: (row: OccurrenceRow, event: React.PointerEvent) => void;
  /**
   * Whether the just-finished gesture was a real drag, consuming the click
   * that follows it so a moved block does not also open its editor.
   */
  consumeClick: () => boolean;
  /** The handlers the columns container binds. */
  handlers: {
    onPointerDown: (event: React.PointerEvent) => void;
    onPointerMove: (event: React.PointerEvent) => void;
    onPointerUp: (event: React.PointerEvent) => void;
    onPointerCancel: () => void;
  };
}

/**
 * The drag gestures over a grid of day columns, over the occurrence rows the
 * snapshot holds. The hook owns the grid's ref, which the grid attaches to
 * its columns container so every pointer position converts against the box
 * the user is actually looking at, read at event time rather than render
 * time. The rows clear the pending overlay: a publish that confirms the
 * write renders the moved row itself, and one that refuses it snaps the
 * block back to what the store still holds. A pointer down that lands on a
 * block is that block's own move gesture; only empty time starts a create.
 */
export function useEventDrag(
  days: string[],
  occurrences: OccurrenceRow[],
  onCreateSlot?: (slot: CreateSlot) => void,
): EventDrag & {
  gridRef: RefObject<HTMLDivElement | null>;
} {
  const dayCount = days.length;
  const [dragging, setDragging] = useState<DragState | null>(null);
  const [creating, setCreating] = useState<CreateSlot | null>(null);
  const [pending, setPending] = useState<PendingMove | null>(null);
  const gesture = useRef<DragState | null>(null);
  const createGesture = useRef<CreateGesture | null>(null);
  const clickToConsume = useRef(false);
  const gridRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    setPending(null);
  }, [occurrences]);

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

  /** Reads the pointer's latest position into the gesture in flight. */
  const track = useCallback(
    (event: React.PointerEvent): GridPoint | null => {
      const grid = gridRef.current;
      if (grid === null) {
        return null;
      }
      const rect = grid.getBoundingClientRect();
      const to = clientToGrid(event.clientX, event.clientY, rect, dayCount);
      if (gesture.current !== null) {
        gesture.current = { ...gesture.current, to };
      }
      if (createGesture.current !== null) {
        createGesture.current = { ...createGesture.current, to };
      }
      return to;
    },
    [dayCount],
  );

  const onPointerDown = useCallback(
    (event: React.PointerEvent) => {
      // A press on a block is that block's own move gesture; a press with a
      // secondary button starts nothing.
      if (
        onCreateSlot === undefined ||
        event.button !== 0 ||
        (event.target as HTMLElement).closest('[data-block]') !== null
      ) {
        return;
      }
      const from = track(event);
      if (from === null) {
        return;
      }
      createGesture.current = { from, to: from };
      setCreating(createSlotOf(createGesture.current, days[from.day]!));
    },
    [days, onCreateSlot, track],
  );

  const onPointerMove = useCallback(
    (event: React.PointerEvent) => {
      track(event);
      if (gesture.current !== null) {
        setDragging(gesture.current);
      }
      if (createGesture.current !== null) {
        setCreating(createSlotOf(createGesture.current, days[createGesture.current.from.day]!));
      }
    },
    [days, track],
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
    setPending({ row: state.row, days: delta.days, minutes: delta.minutes });
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
      if (createGesture.current !== null) {
        const finished = createGesture.current;
        createGesture.current = null;
        setCreating(null);
        // A press that never left its slot is a click on empty time.
        if (moved(finished) && onCreateSlot !== undefined) {
          onCreateSlot(createSlotOf(finished, days[finished.from.day]!));
        }
        return;
      }
      commit();
    },
    [commit, days, onCreateSlot, track],
  );

  const onPointerCancel = useCallback(() => {
    gesture.current = null;
    createGesture.current = null;
    setDragging(null);
    setCreating(null);
  }, []);

  const consumeClick = useCallback(() => {
    const consumed = clickToConsume.current;
    clickToConsume.current = false;
    return consumed;
  }, []);

  return {
    gridRef,
    dragging,
    creating,
    pending,
    beginDrag,
    consumeClick,
    handlers: { onPointerDown, onPointerMove, onPointerUp, onPointerCancel },
  };
}
