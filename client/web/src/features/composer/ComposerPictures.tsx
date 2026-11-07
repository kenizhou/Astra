// SPDX-License-Identifier: GPL-3.0-only
// The composer's picture surface: the retained image blocks rendered as
// visible pictures below the editor, in staging order, sourced from the
// staged bytes behind object URLs revoked when their pictures go. A row
// with no bytes client-side (a carried row from a resumed draft) renders
// its fallback placeholder; that edge is the ledger's, not solved here.
// The resize grips ride the surface from outside: the overlay lives in
// the body, and a dragged width writes into the retained block the
// picture renders, never into the editor's own tree.

import { useEffect, useRef } from 'react';

import { useComposerStore } from './composerStore';
import { installPictureResize } from './pictureResize';
import { setImageWidth } from './pictureWidth';
import type { RetainedBlock } from './documentWire';
import type { Block, DraftAttachment } from '@/services/ipc';

/** One picture the surface renders, from the block that holds it. */
interface Picture {
  id: string;
  alt: string;
  widthPx: number | null;
  /** The row's blob handle, when the bytes stand client-side. */
  blob: string | null;
}

/** The retained blocks' image inlines as pictures, in staging order,
 *  joined to their rows for the bytes the src needs. */
function picturesOf(retained: RetainedBlock[], attachments: DraftAttachment[]): Picture[] {
  const blocks: Block[] = retained.map((kept) => kept.block);
  const pictures: Picture[] = [];
  for (const block of blocks) {
    if (!('Paragraph' in block)) {
      continue;
    }
    for (const inline of block.Paragraph.content) {
      if (!('Image' in inline)) {
        continue;
      }
      const row = attachments.find((entry) => entry.id === inline.Image.attachment_id);
      pictures.push({
        id: inline.Image.attachment_id,
        alt: inline.Image.alt_text,
        widthPx: inline.Image.width_px,
        blob: row?.blob ?? null,
      });
    }
  }
  return pictures;
}

export function ComposerPictures() {
  const retained = useComposerStore((s) => s.editor.retained);
  const attachments = useComposerStore((s) => s.editor.attachments);
  const files = useComposerStore((s) => s.files);
  const hostRef = useRef<HTMLDivElement>(null);
  const pictures = picturesOf(retained, attachments);

  // One object URL per blob handle, minted on the render that first needs
  // it and revoked when the picture that held it goes or the surface
  // unmounts: a leaked URL pins the staged bytes for the session. The
  // mint sits in the render (idempotent behind the map), so a StrictMode
  // remount re-mints what the unmount cleanup revoked.
  const urls = useRef(new Map<string, string>());
  for (const picture of pictures) {
    const blob = picture.blob;
    if (
      blob !== null &&
      !urls.current.has(blob) &&
      files[blob] !== undefined &&
      typeof URL.createObjectURL === 'function'
    ) {
      urls.current.set(blob, URL.createObjectURL(files[blob]));
    }
  }
  useEffect(() => {
    const held = urls.current;
    const standing = new Set(
      pictures.flatMap((picture) => (picture.blob === null ? [] : [picture.blob])),
    );
    for (const [blob, url] of held) {
      if (!standing.has(blob)) {
        held.delete(blob);
        URL.revokeObjectURL(url);
      }
    }
  });
  useEffect(() => {
    const held = urls.current;
    return () => {
      for (const url of held.values()) {
        URL.revokeObjectURL(url);
      }
      held.clear();
    };
  }, []);

  // The grips: chrome outside the surface's own tree, writing the dragged
  // width into the block the picture renders.
  useEffect(() => {
    const host = hostRef.current;
    if (host === null) {
      return undefined;
    }
    return installPictureResize(host, (image, width) => {
      const id = image.dataset.attachment;
      if (id !== undefined) {
        setImageWidth(id, width);
      }
    });
  }, []);

  return (
    <div
      ref={hostRef}
      aria-label="Inline pictures"
      className={
        pictures.length === 0
          ? 'relative shrink-0'
          : 'relative max-h-[50%] shrink-0 overflow-y-auto border-b border-border-subtle bg-background px-4 py-2'
      }
    >
      <div className="flex flex-col items-start gap-2">
        {pictures.map((picture) => {
          const src = picture.blob === null ? null : (urls.current.get(picture.blob) ?? null);
          if (src === null) {
            return (
              <div
                key={picture.id}
                data-placeholder={picture.alt}
                className="flex h-16 w-40 items-center justify-center rounded-[var(--radius-sm)] border border-dashed border-border-subtle text-xs text-muted-foreground"
              >
                {picture.alt}
              </div>
            );
          }
          return (
            <img
              key={picture.id}
              data-attachment={picture.id}
              src={src}
              alt={picture.alt}
              style={picture.widthPx === null ? undefined : { width: `${picture.widthPx}px` }}
              className="h-auto max-w-full"
            />
          );
        })}
      </div>
    </div>
  );
}
