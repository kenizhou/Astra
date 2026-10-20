// SPDX-License-Identifier: GPL-3.0-only
// The composer's attachment surface: the Attach button opens the webview's
// own multi-select input, every picked file stands as a removable row, and a
// drag onto the composer attaches a document outright while a picture asks
// inline or file, the answer recorded on the row's disposition.

import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { ComposerAttachments } from './ComposerAttachments';
import { useComposerStore } from './composerStore';
import { editorToDocument } from './documentWire';

beforeEach(() => {
  useComposerStore.getState().closePane();
  useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });
});

afterEach(cleanup);

function pdf(name = 'report.pdf'): File {
  return new File([new Uint8Array([80, 68, 70])], name, { type: 'application/pdf' });
}

function png(name = 'photo.png'): File {
  return new File([new Uint8Array([1, 2])], name, { type: 'image/png' });
}

describe('the attachment rows', () => {
  it('stands each picked file as a removable row', () => {
    render(<ComposerAttachments />);
    act(() => {
      useComposerStore.getState().attachFiles([pdf()]);
    });

    expect(screen.getByText('report.pdf')).toBeTruthy();
    expect(screen.getByLabelText('Remove report.pdf')).toBeTruthy();
  });

  it('removes the row from the control', () => {
    render(<ComposerAttachments />);
    act(() => {
      useComposerStore.getState().attachFiles([pdf()]);
    });

    fireEvent.click(screen.getByLabelText('Remove report.pdf'));
    expect(useComposerStore.getState().editor.attachments).toHaveLength(0);
  });
});

describe('the attach button', () => {
  it('hands the input selection to the store as attachments', () => {
    const { container } = render(<ComposerAttachments />);
    // The button and the input it opens share their label; the input is the
    // one carrying the selection.
    const input = container.querySelector('input[type="file"]') as HTMLInputElement;
    Object.defineProperty(input, 'files', { value: [pdf(), pdf('notes.pdf')] });

    fireEvent.change(input);

    expect(useComposerStore.getState().editor.attachments.map((row) => row.file_name)).toEqual([
      'report.pdf',
      'notes.pdf',
    ]);
  });
});

describe('the drop', () => {
  it('attaches a dropped document outright', () => {
    render(<ComposerAttachments />);
    act(() => {
      useComposerStore.getState().dropFiles([pdf()]);
    });

    const rows = useComposerStore.getState().editor.attachments;
    expect(rows).toHaveLength(1);
    expect(rows[0]!.disposition).toBe('Attachment');
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('asks inline or file for a dropped picture and records the answer', () => {
    render(<ComposerAttachments />);
    act(() => {
      useComposerStore.getState().dropFiles([png()]);
    });

    expect(screen.getByRole('dialog')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Attach as file' }));
    const filed = useComposerStore.getState().editor.attachments[0]!;
    expect(filed.disposition).toBe('Attachment');

    act(() => {
      useComposerStore.getState().dropFiles([png('shot.png')]);
    });
    fireEvent.click(screen.getByRole('button', { name: 'Insert inline' }));
    const inlined = useComposerStore.getState().editor.attachments[1]!;
    expect(inlined.disposition).toEqual({ Inline: { cid: expect.any(String) } });
    // The inline answer references the row from the document, so the part
    // rides the send as body content rather than orphaned metadata.
    const document = editorToDocument(useComposerStore.getState().editor);
    expect(JSON.stringify(document.blocks)).toContain(inlined.id);
  });
});
