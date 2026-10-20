// SPDX-License-Identifier: GPL-3.0-only
// The reading pane's Save As: the menu item calls the attachment service and
// the list shows the outcome, the saved path or the refusal's sentence, and
// Download All loops the service over every row.

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const attachmentsService = vi.hoisted(() => ({ saveAttachment: vi.fn() }));

vi.mock('@/services/attachments', () => attachmentsService);

import { AttachmentList } from './AttachmentList';
import type { AttachmentRow } from '@/services/ipc';

beforeEach(() => {
  attachmentsService.saveAttachment.mockReset();
});

afterEach(cleanup);

const MESSAGE = { account: 'work', key: 'm1' };

function attachment(over: Partial<AttachmentRow> = {}): AttachmentRow {
  return { id: 1, file_name: 'report.pdf', media_type: 'application/pdf', size: 4096, ...over };
}

function openMenu(on: AttachmentRow): void {
  fireEvent.contextMenu(screen.getByText(on.file_name));
}

describe('saving from the attachment menu', () => {
  it('calls the service with the open message and the row id', () => {
    attachmentsService.saveAttachment.mockResolvedValue('cancelled');
    render(<AttachmentList attachments={[attachment()]} message={MESSAGE} />);
    openMenu(attachment());

    fireEvent.click(screen.getByRole('menuitem', { name: 'Save As…' }));

    expect(attachmentsService.saveAttachment).toHaveBeenCalledWith(MESSAGE, 1);
  });

  it('shows the saved path when the save lands', async () => {
    attachmentsService.saveAttachment.mockResolvedValue({ saved: { path: 'C:\\saved\\report.pdf' } });
    render(<AttachmentList attachments={[attachment()]} message={MESSAGE} />);
    openMenu(attachment());

    fireEvent.click(screen.getByRole('menuitem', { name: 'Save As…' }));

    expect((await screen.findByRole('status')).textContent).toContain('C:\\saved\\report.pdf');
  });

  it('shows the refusal sentence when the save fails', async () => {
    attachmentsService.saveAttachment.mockResolvedValue({ failed: { reason: 'the disk is full' } });
    render(<AttachmentList attachments={[attachment()]} message={MESSAGE} />);
    openMenu(attachment());

    fireEvent.click(screen.getByRole('menuitem', { name: 'Save As…' }));

    expect((await screen.findByRole('status')).textContent).toContain('the disk is full');
  });

  it('loops the service over every row from Download All', async () => {
    attachmentsService.saveAttachment.mockResolvedValue('cancelled');
    const rows = [attachment(), attachment({ id: 5, file_name: 'logo.png' })];
    render(<AttachmentList attachments={rows} message={MESSAGE} />);
    openMenu(rows[1]!);

    fireEvent.click(screen.getByRole('menuitem', { name: 'Download All' }));

    await waitFor(() =>
      expect(attachmentsService.saveAttachment).toHaveBeenCalledTimes(2),
    );
    expect(attachmentsService.saveAttachment).toHaveBeenCalledWith(MESSAGE, 1);
    expect(attachmentsService.saveAttachment).toHaveBeenCalledWith(MESSAGE, 5);
  });
});
