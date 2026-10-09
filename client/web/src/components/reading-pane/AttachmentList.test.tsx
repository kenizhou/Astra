// SPDX-License-Identifier: GPL-3.0-only
// The attachment rows: read-only metadata (name, media type, human size) per
// part beneath the open message's body; saving arrives with the attachments
// slice.
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

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
