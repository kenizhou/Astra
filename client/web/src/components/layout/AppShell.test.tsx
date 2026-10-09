// SPDX-License-Identifier: GPL-3.0-only
// The shell's composer wiring: the New message action opens the composer
// pane with the first account as its From, stays inert without an account,
// and the app start resume pulls the stored draft into the pane when the
// host has one.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn(), draftResume: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  draftResume: ipc.draftResume,
}));

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: true,
}));

import { AppShell } from './AppShell';
import type { ResumeView } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useComposerStore } from '@/features/composer/Composer';
import { documentToEditor } from '@/features/composer/documentWire';

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.draftResume.mockReset();
  ipc.draftResume.mockResolvedValue(null);
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work' }] });
  useComposerStore.setState({
    open: false,
    resumeAttempted: false,
    composition: null,
    from: '',
    fromLabel: '',
    to: '',
    cc: '',
    bcc: '',
    subject: '',
    editor: documentToEditor({ blocks: [{ Paragraph: { content: [] } }], attachments: [] }),
    savedFingerprint: null,
  });
});

afterEach(cleanup);

describe('the shell composer wiring', () => {
  it('opens the composer pane from the new message action', () => {
    render(<AppShell />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'New message' }));
    });

    expect(screen.getByRole('region', { name: 'Composer' })).toBeTruthy();
    expect(screen.getByText('Work')).toBeTruthy();
  });

  it('keeps the new message action inert without an account', () => {
    useAccountStore.setState({ accounts: [] });
    render(<AppShell />);

    const action = screen.getByRole('button', { name: 'New message' }) as HTMLButtonElement;
    expect(action.disabled).toBe(true);

    act(() => {
      action.click();
    });

    expect(screen.queryByRole('region', { name: 'Composer' })).toBeNull();
  });

  it('resumes the stored draft into the pane at app start', async () => {
    const view: ResumeView = {
      account: 'work',
      to: 'you@test.local',
      cc: '',
      bcc: '',
      subject: 'Later',
      body_text: 'Half written',
    };
    ipc.draftResume.mockResolvedValue(view);
    render(<AppShell />);

    await act(async () => {
      await Promise.resolve();
    });

    expect(ipc.draftResume).toHaveBeenCalledTimes(1);
    expect((screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement).value).toBe(
      'you@test.local',
    );
    expect((screen.getByRole('textbox', { name: 'Subject' }) as HTMLInputElement).value).toBe(
      'Later',
    );
  });
});
