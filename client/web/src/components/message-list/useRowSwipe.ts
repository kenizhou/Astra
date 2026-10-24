// SPDX-License-Identifier: GPL-3.0-only
// The row's touch-swipe gesture: the touch pointer owns the horizontal axis
// (a mouse pointer never starts a swipe, so the folder drag keeps its
// channel), a vertical-dominant move hands the axis back to the virtualiser
// (the row's pan-y touch-action lets the browser take the vertical pan and
// cancel the pointer), and a release past the threshold fires the
// direction's configured action with its undo toast. A fired swipe swallows
// the click that follows, so the row does not also open.

import { useRef, useState } from 'react';

import { dispatch } from '@/services/ipc';
import { useAppearanceStore } from '@/services/appearance';
import { useFolderStore } from '@/stores/folderStore';
import type { MessageRowView } from '@/services/surfaces';

import {
  SWIPE_REVEAL_PX,
  swipeActionFor,
  swipeFires,
  swipeIntent,
  swipeReverse,
  swipeToastSentence,
  type SwipeAction,
  type SwipeDirection,
} from './rowSwipe';
import { useSwipeToastStore } from './swipeToastStore';

export interface RowSwipe {
  /** The live horizontal translation of the row's content. */
  dx: number;
  /** The direction whose action stands revealed, null while at rest. */
  revealed: SwipeDirection | null;
  /** The act the reveal names, for the layer's label and chrome. */
  revealedAction: SwipeAction | null;
  /** Whether a completed swipe should swallow the click that follows. */
  swiped: boolean;
  /** Spends a standing swallow, so the click after the swallowed one lands. */
  clearSwallowed: () => void;
  /** The pointer handlers the row's root carries. */
  handlers: {
    onPointerDown: (event: React.PointerEvent) => void;
    onPointerMove: (event: React.PointerEvent) => void;
    onPointerUp: (event: React.PointerEvent) => void;
    onPointerCancel: (event: React.PointerEvent) => void;
  };
}

/** One gesture's origin, tracked while the touch pointer is down. */
interface Gesture {
  pointerId: number;
  startX: number;
  startY: number;
}

export function useRowSwipe(row: MessageRowView): RowSwipe {
  const [dx, setDx] = useState(0);
  const [revealed, setRevealed] = useState<SwipeDirection | null>(null);
  const [swiped, setSwiped] = useState(false);
  const gesture = useRef<Gesture | null>(null);
  const swipePrimary = useAppearanceStore((s) => s.swipePrimary);
  const swipeSecondary = useAppearanceStore((s) => s.swipeSecondary);

  // The fire path resolves the sitting folder at the moment of release, so
  // the undo names the folder the row sat in.
  const fire = (direction: SwipeDirection): void => {
    const action = swipeActionFor(direction, swipePrimary, swipeSecondary);
    const sitting = useFolderStore.getState().selected;
    void dispatch(swipeIntent(action, row));
    useSwipeToastStore
      .getState()
      .show(swipeToastSentence(action), swipeReverse(action, row, sitting));
    setSwiped(true);
  };

  const clearSwallowed = (): void => {
    setSwiped(false);
  };

  const reset = (): void => {
    gesture.current = null;
    setDx(0);
    setRevealed(null);
  };

  return {
    dx,
    revealed,
    revealedAction:
      revealed === null ? null : swipeActionFor(revealed, swipePrimary, swipeSecondary),
    swiped,
    clearSwallowed,
    handlers: {
      // Only the touch pointer starts a swipe: the mouse owns the folder
      // drag, and a stylus is not this slice's gesture.
      onPointerDown: (event) => {
        if (event.pointerType !== 'touch') {
          return;
        }
        gesture.current = {
          pointerId: event.pointerId,
          startX: event.clientX,
          startY: event.clientY,
        };
        setSwiped(false);
      },
      onPointerMove: (event) => {
        const active = gesture.current;
        if (active === null || active.pointerId !== event.pointerId) {
          return;
        }
        const movedX = event.clientX - active.startX;
        const movedY = event.clientY - active.startY;
        if (Math.abs(movedY) > Math.abs(movedX) && Math.abs(movedY) > 12) {
          // The vertical axis won: the virtualiser's scroll owns the gesture
          // from here, and the browser's pan-y takes it outright.
          reset();
          return;
        }
        const clamped = Math.max(-SWIPE_REVEAL_PX, Math.min(SWIPE_REVEAL_PX, movedX));
        setDx(clamped);
        setRevealed(clamped > 0 ? 'right' : clamped < 0 ? 'left' : null);
      },
      onPointerUp: (event) => {
        const active = gesture.current;
        if (active === null || active.pointerId !== event.pointerId) {
          return;
        }
        const movedX = event.clientX - active.startX;
        const movedY = event.clientY - active.startY;
        if (swipeFires(movedX, movedY)) {
          fire(movedX > 0 ? 'right' : 'left');
        }
        reset();
      },
      onPointerCancel: () => {
        if (gesture.current !== null) {
          reset();
        }
      },
    },
  };
}
