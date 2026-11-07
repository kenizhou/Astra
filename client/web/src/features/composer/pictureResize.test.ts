// SPDX-License-Identifier: GPL-3.0-only
// The picture resize gesture's contract: the pure projection a corner
// drag answers with, and the grips the overlay stands over the rendered
// picture (press pins them, hover moves them, every move measured from
// the drag start, the width applied on the move).

import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { draggedWidth, installPictureResize, type ResizeStart } from './pictureResize';

/// A 400x300 picture, dragged from whichever corner the test is about.
function start(corner: { x: number; y: number }): ResizeStart {
  return { width: 400, height: 300, corner };
}

const NW = { x: -1, y: -1 };
const NE = { x: 1, y: -1 };
const SE = { x: 1, y: 1 };
const ROOM = 10000;

describe('a corner drag', () => {
  it('grows the picture by the distance travelled along its diagonal', () => {
    // 400x300 has a 500 diagonal, so a (40, 30) pull away from the
    // bottom-right corner is 50 along it: a tenth longer, a tenth wider.
    expect(draggedWidth(start(SE), 40, 30, ROOM)).toBe(440);
    expect(draggedWidth(start(SE), -40, -30, ROOM)).toBe(360);
  });

  it('grows on a pull away from the middle, from every corner', () => {
    // The picture is a block of its own under the editor, so a west corner
    // has to grow on a LEFTWARD drag, or two of the four handles would
    // shrink the picture when pulled outwards.
    expect(draggedWidth(start(NW), -40, -30, ROOM)).toBe(440);
    expect(draggedWidth(start(NE), 40, -30, ROOM)).toBe(440);
  });

  it('leaves the size alone when the pointer moves across the diagonal', () => {
    // The ratio is locked, so only travel along the diagonal is a size.
    // Without the projection the two axes each answer with a different
    // width and the picture jumps between them.
    expect(draggedWidth(start(SE), 30, -40, ROOM)).toBe(400);
  });

  it('stops at the width the column can actually show', () => {
    expect(draggedWidth(start(SE), 5000, 3750, 620)).toBe(620);
  });

  it('stops before the picture is too small to grab', () => {
    expect(draggedWidth(start(SE), -5000, -3750, ROOM)).toBe(24);
    // A column narrower than the floor must not invert the clamp into a
    // zero-width picture.
    expect(draggedWidth(start(SE), -5000, -3750, 10)).toBe(24);
  });

  it('answers with the floor for a picture that has no size yet', () => {
    expect(draggedWidth({ width: 0, height: 0, corner: SE }, 40, 30, ROOM)).toBe(24);
  });
});

describe('the grips', () => {
  let host: HTMLElement;
  let image: HTMLImageElement;

  /// jsdom lays nothing out, so the picture is told what rectangle it
  /// occupies: 400x300 at the origin, inside a host of the same box.
  function rect(width: number, height: number) {
    return { x: 0, y: 0, top: 0, left: 0, right: width, bottom: height, width, height } as DOMRect;
  }

  function press(target: EventTarget, type: string, x: number, y: number, pointerType = 'mouse') {
    target.dispatchEvent(
      new PointerEvent(type, {
        bubbles: true,
        clientX: x,
        clientY: y,
        pointerId: 1,
        pointerType,
      }),
    );
  }

  function grip(index: number): HTMLElement {
    const grips = document.querySelectorAll('.astra-picture-resize .grip');
    return grips[index] as unknown as HTMLElement;
  }

  function overlay(): HTMLElement {
    return document.querySelector('.astra-picture-resize') as unknown as HTMLElement;
  }

  /** The widths the gesture applied, with the picture each belongs to. */
  let applied: Array<[string, number]>;

  beforeEach(() => {
    host = document.createElement('div');
    document.body.appendChild(host);
    host.innerHTML = '<p><img data-attachment="pic-1" alt="chart"></p>';
    image = host.querySelector('img') as unknown as HTMLImageElement;
    image.getBoundingClientRect = () => rect(400, 300);
    host.getBoundingClientRect = () => rect(620, 800);
    applied = [];
    installPictureResize(host, (target, width) => {
      applied.push([target.dataset.attachment ?? '', width]);
    });
  });

  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('are chrome outside the picture surface, never part of it', () => {
    // The picture surface renders what the document holds; grips inside it
    // would ride the message. The overlay is a sibling in the body.
    expect(host.querySelector('.astra-picture-resize')).toBeNull();
    expect(document.body.querySelector('.astra-picture-resize')).not.toBeNull();
  });

  it('appear on the picture the pointer is over, and go when it leaves', () => {
    expect(overlay().hidden).toBe(true);

    press(image, 'pointermove', 200, 150);
    expect(overlay().hidden).toBe(false);
    expect(overlay().style.width).toBe('400px');

    press(host, 'pointermove', 600, 700);
    expect(overlay().hidden).toBe(true);
  });

  it('stay on a picture a finger tapped, which has no hover to keep them there', () => {
    press(image, 'pointerdown', 200, 150, 'touch');
    expect(overlay().hidden).toBe(false);

    // The gesture that follows is a drag on a grip, so the pointer moving
    // away must not take them off the picture the way it does for a mouse.
    press(host, 'pointermove', 600, 700, 'touch');
    expect(overlay().hidden).toBe(false);

    press(host, 'pointerdown', 600, 700, 'touch');
    expect(overlay().hidden).toBe(true);
  });

  it('resize the picture as a finger drags one, not only as a mouse does', () => {
    press(image, 'pointerdown', 200, 150, 'touch');
    const corner = grip(3);
    press(corner, 'pointerdown', 400, 300, 'touch');
    press(corner, 'pointermove', 440, 330, 'touch');
    press(corner, 'pointerup', 440, 330, 'touch');

    expect(applied).toEqual([['pic-1', 440]]);
  });

  it('measure every move from where the drag began, so a pointer cannot drift the size', () => {
    press(image, 'pointermove', 200, 150);
    const corner = grip(3);
    press(corner, 'pointerdown', 400, 300);
    press(corner, 'pointermove', 440, 330);
    press(corner, 'pointermove', 480, 360);
    press(corner, 'pointerup', 480, 360);

    expect(applied.at(-1)).toEqual(['pic-1', 480]);
  });

  it('follow the mouse to another picture even after a press pinned them to one', () => {
    // The pointer is the answer to "which picture is this about"; a press
    // is only what keeps them there once it leaves.
    const second = document.createElement('img');
    second.dataset.attachment = 'pic-2';
    second.alt = 'photo';
    second.getBoundingClientRect = () => rect(200, 100);
    host.querySelector('p')!.appendChild(second);

    press(image, 'pointerdown', 200, 150);
    expect(overlay().style.width).toBe('400px');

    press(second, 'pointermove', 100, 50);
    expect(overlay().style.width).toBe('200px');
  });

  it('let go of the picture even when the pointer was never captured', () => {
    // A capture the engine refuses, or one the host cancels, sends the
    // rest of the gesture to whatever is under the pointer instead of to
    // the grip. The drag has to end all the same, or the next pass over
    // the grip carries on resizing a picture the user let go of.
    press(image, 'pointermove', 200, 150);
    const corner = grip(3);
    press(corner, 'pointerdown', 400, 300);
    press(corner, 'pointermove', 440, 330);
    press(host, 'pointerup', 440, 330);
    press(host, 'pointermove', 600, 700);

    expect(applied).toEqual([['pic-1', 440]]);
  });

  it('apply on the move, so the person sees the size while the hand still holds it', () => {
    press(image, 'pointermove', 200, 150);
    const corner = grip(3);
    press(corner, 'pointerdown', 400, 300);
    press(corner, 'pointermove', 440, 330);

    expect(applied).toEqual([['pic-1', 440]]);
  });

  it('go when the picture the user was resizing is deleted', async () => {
    press(image, 'pointerdown', 200, 150);
    expect(overlay().hidden).toBe(false);

    image.remove();
    // The surface's React tree owns the picture's presence; the observer
    // the gesture watches the host with reports the removal.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(overlay().hidden).toBe(true);
  });

  it('follow the applied width when React lands it on the picture', async () => {
    press(image, 'pointermove', 200, 150);
    const corner = grip(3);
    press(corner, 'pointerdown', 400, 300);
    press(corner, 'pointermove', 440, 330);
    press(document, 'pointerup', 440, 330);

    // The applied width reaches the picture as a React style update AFTER
    // the drag's own handler returns, so the re-place the handler scheduled
    // measured the stale rect; the watcher owes the overlay a second place
    // over the fresh one, or the grips sit off the corner until the next
    // interaction.
    image.getBoundingClientRect = () => rect(440, 330);
    image.setAttribute('style', 'width: 440px');
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(overlay().style.width).toBe('440px');
  });

  it('stop listening when the surface unmounts', () => {
    const second = installPictureResize(host, () => undefined);
    const before = document.querySelectorAll('.astra-picture-resize').length;
    second();
    expect(document.querySelectorAll('.astra-picture-resize')).toHaveLength(before - 1);
    // The first install's overlay stands; its behaviour is untouched.
    press(image, 'pointermove', 200, 150);
    expect(overlay().hidden).toBe(false);
  });
});
