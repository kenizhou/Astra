// SPDX-License-Identifier: GPL-3.0-only
// The attachment rows: read-only metadata (name, media type, human size) per
// part beneath the open message's body; saving arrives with the attachments
// slice.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { AttachmentList } from './AttachmentList';
import type { AttachmentRow } from '@/services/ipc';

afterEach(cleanup);

function attachment(over: Partial<AttachmentRow> = {}): AttachmentRow {
  return { id: 1, file_name: 'report.pdf', media_type: 'application/pdf', size: 4096, ...over };
}

describe('the attachment list', () => {
  it('renders one metadata row per attachment', () => {
    render(
      <AttachmentList
        attachments={[
          attachment(),
          attachment({ id: 5, file_name: 'logo.png', media_type: 'image/png', size: 512 }),
          attachment({ id: 9, file_name: 'deck.pdf', media_type: 'application/pdf', size: 2_202_009 }),
        ]}
      />,
    );

    expect(screen.getByText('report.pdf')).toBeTruthy();
    expect(screen.getAllByText('application/pdf')).toHaveLength(2);
    expect(screen.getByText('4.0 KB')).toBeTruthy();
    expect(screen.getByText('logo.png')).toBeTruthy();
    expect(screen.getByText('image/png')).toBeTruthy();
    expect(screen.getByText('512 B')).toBeTruthy();
    expect(screen.getByText('2.1 MB')).toBeTruthy();
    expect(screen.getByLabelText('Attachments').childElementCount).toBe(3);
  });

  it('renders nothing for an empty list', () => {
    const { container } = render(<AttachmentList attachments={[]} />);

    expect(container.firstChild).toBeNull();
  });
});

describe('the attachment menu', () => {
  it('opens on right-click with placeholders and a working copy', () => {
    render(<AttachmentList attachments={[attachment()]} />);

    fireEvent.contextMenu(screen.getByText('report.pdf'));

    for (const label of ['Preview', 'Save As…']) {
      expect(screen.getByRole('menuitem', { name: label }).getAttribute('aria-disabled')).toBe('true');
    }
    // One attachment: no Download All row.
    expect(screen.queryByRole('menuitem', { name: 'Download All' })).toBeNull();
    expect(screen.getByRole('menuitem', { name: 'Copy File Name' })).toBeTruthy();
  });

  it('offers Download All over several attachments', () => {
    render(
      <AttachmentList
        attachments={[attachment(), attachment({ id: 5, file_name: 'logo.png' })]}
      />,
    );

    fireEvent.contextMenu(screen.getByText('logo.png'));

    expect(screen.getByRole('menuitem', { name: 'Download All' }).getAttribute('aria-disabled')).toBe('true');
  });

  it('copies the file name from the menu', () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    render(<AttachmentList attachments={[attachment()]} />);

    fireEvent.contextMenu(screen.getByText('report.pdf'));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Copy File Name' }));

    expect(writeText).toHaveBeenCalledWith('report.pdf');
  });

  it('opens from the keyboard with Shift+F10', () => {
    render(<AttachmentList attachments={[attachment()]} />);

    fireEvent.keyDown(screen.getByText('report.pdf'), { key: 'F10', shiftKey: true });

    expect(screen.getByRole('menu')).toBeTruthy();
  });
});
