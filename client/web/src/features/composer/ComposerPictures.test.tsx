// SPDX-License-Identifier: GPL-3.0-only
// The picture surface's contract: the retained image blocks render as
// visible pictures below the editor in staging order, sourced from the
// staged bytes (an object URL revoked when the picture goes), a row with
// no client-side bytes renders its fallback placeholder, the width the
// block names is the width the picture shows, and the drag gesture
// writes the width back into the block.

import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ComposerPictures } from './ComposerPictures';
import { useComposerStore } from './composerStore';
import { editorToDocument } from './documentWire';
import type { DraftAttachment } from '@/services/ipc';

function png(name = 'photo.png'): File {
  return new File([new Uint8Array([1, 2])], name, { type: 'image/png' });
}

/** A carried row: pre-attached by a resume, holding no bytes client-side. */
function carriedRow(id: string, name = `${id}.png`): DraftAttachment {
  return {
    id,
    blob: null,
    file_name: name,
    media_type: 'image/png',
    size: 512,
    disposition: { Inline: { cid: `cid-${id}` } },
    data_url: null,
  };
}

beforeEach(() => {
  useComposerStore.getState().closePane();
  useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });
});

afterEach(() => {
  cleanup();
  useComposerStore.getState().closePane();
});

function pictures(): HTMLElement[] {
  return Array.from(
    screen.getByLabelText('Inline pictures').querySelectorAll('img, [data-placeholder]'),
  ) as HTMLElement[];
}

describe('the rendered pictures', () => {
  it('renders each staged inline picture below the editor, in staging order', () => {
    render(<ComposerPictures />);
    act(() => {
      useComposerStore.getState().pasteImages([png(), png('shot.png')]);
    });

    const imgs = screen.getByLabelText('Inline pictures').querySelectorAll('img');
    expect(imgs).toHaveLength(2);
    expect(imgs[0]!.getAttribute('alt')).toBe('photo.png');
    expect(imgs[1]!.getAttribute('alt')).toBe('shot.png');
  });

  it('sources each picture from the staged bytes and revokes the URL when it goes', () => {
    const minted: string[] = [];
    const revoked: string[] = [];
    vi.stubGlobal(
      'URL',
      class extends URL {
        static createObjectURL(file: File): string {
          const url = `blob:stub-${file.name}`;
          minted.push(url);
          return url;
        }
        static revokeObjectURL(url: string): void {
          revoked.push(url);
        }
      },
    );
    render(<ComposerPictures />);
    act(() => {
      useComposerStore.getState().pasteImages([png()]);
    });

    const img = screen.getByLabelText('Inline pictures').querySelector('img')!;
    expect(img.getAttribute('src')).toBe('blob:stub-photo.png');
    expect(minted).toEqual(['blob:stub-photo.png']);

    // The row's removal strips the block, and the picture's visibility
    // follows the block's presence: the URL it held is revoked.
    const id = useComposerStore.getState().editor.attachments[0]!.id;
    act(() => {
      useComposerStore.getState().removeAttachment(id);
    });
    expect(screen.getByLabelText('Inline pictures').querySelectorAll('img')).toHaveLength(0);
    expect(revoked).toEqual(['blob:stub-photo.png']);
    vi.unstubAllGlobals();
  });

  it('shows the fallback placeholder for a row with no bytes client-side', () => {
    // A carried row from a resumed draft holds its parts host-side: no
    // blob handle names bytes in the composer, so the picture renders its
    // placeholder rather than a broken image.
    render(<ComposerPictures />);
    act(() => {
      const editor = useComposerStore.getState().editor;
      useComposerStore.setState({
        editor: {
          ...editor,
          retained: [
            {
              index: 1_000_000,
              block: {
                Paragraph: {
                  content: [{ Image: { attachment_id: 'carried', alt_text: 'chart.png', width_px: null } }],
                },
              },
            },
          ],
          attachments: [...editor.attachments, carriedRow('carried')],
        },
      });
    });

    const shown = pictures();
    expect(shown).toHaveLength(1);
    expect(shown[0]!.getAttribute('data-placeholder')).toBe('chart.png');
    expect(screen.getByLabelText('Inline pictures').querySelectorAll('img')).toHaveLength(0);
  });

  it('carries the width the block names, and no height of its own', () => {
    render(<ComposerPictures />);
    act(() => {
      useComposerStore.getState().pasteImages([png()]);
    });
    const id = useComposerStore.getState().editor.attachments[0]!.id;
    act(() => {
      const editor = useComposerStore.getState().editor;
      useComposerStore.setState({
        editor: {
          ...editor,
          retained: editor.retained.map((kept) => ({
            ...kept,
            block: {
              Paragraph: {
                content: [
                  { Image: { attachment_id: id, alt_text: 'photo.png', width_px: 260 } },
                ],
              },
            },
          })),
        },
      });
    });

    const img = screen.getByLabelText('Inline pictures').querySelector('img')!;
    expect(img.style.width).toBe('260px');
    // Only the width travels: the height follows the picture's own ratio.
    expect(img.style.height).toBe('');
    expect(img.getAttribute('height')).toBeNull();
  });

  it('takes the picture with the row when the row is removed', () => {
    render(<ComposerPictures />);
    act(() => {
      useComposerStore.getState().pasteImages([png(), png('shot.png')]);
    });
    const first = useComposerStore.getState().editor.attachments[0]!.id;

    act(() => {
      useComposerStore.getState().removeAttachment(first);
    });

    const imgs = screen.getByLabelText('Inline pictures').querySelectorAll('img');
    expect(imgs).toHaveLength(1);
    expect(imgs[0]!.getAttribute('alt')).toBe('shot.png');
  });
});

describe('the resize gesture over the rendered picture', () => {
  function rect(width: number, height: number) {
    return { x: 0, y: 0, top: 0, left: 0, right: width, bottom: height, width, height } as DOMRect;
  }

  function press(target: Element, type: string, x: number, y: number, pointerType = 'mouse') {
    act(() => {
      target.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          clientX: x,
          clientY: y,
          pointerId: 1,
          pointerType,
        }),
      );
    });
  }

  it('writes the dragged width into the block the submit reads', () => {
    vi.stubGlobal(
      'URL',
      class extends URL {
        static createObjectURL(): string {
          return 'blob:stub';
        }
        static revokeObjectURL(): void {
          // The URL lifecycle is the surface's own contract, pinned above.
        }
      },
    );
    render(<ComposerPictures />);
    act(() => {
      useComposerStore.getState().pasteImages([png()]);
    });
    const img = screen.getByLabelText('Inline pictures').querySelector('img')!;
    img.getBoundingClientRect = () => rect(400, 300);

    press(img, 'pointerdown', 200, 150);
    const corner = document.querySelector('.astra-picture-resize .grip:last-child') as HTMLElement;
    press(corner, 'pointerdown', 400, 300);
    press(corner, 'pointermove', 440, 330);
    press(corner, 'pointerup', 440, 330);

    const inline = editorToDocument(useComposerStore.getState().editor).blocks.flatMap((block) =>
      'Paragraph' in block ? block.Paragraph.content : [],
    ).find((entry) => 'Image' in entry) as { Image: { width_px: number } };
    expect(inline.Image.width_px).toBe(440);
    expect(img.style.width).toBe('440px');
    vi.unstubAllGlobals();
  });
});
