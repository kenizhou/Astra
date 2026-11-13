// SPDX-License-Identifier: GPL-3.0-only
// The password dialog: the prompt the needs-password verdict mounts, the
// passphrase submission in-process, the re-prompt a wrong passphrase takes
// (the refusal named, the input emptied), the close a decrypt takes, and the
// cancel that leaves the verdict needs-password (fail closed). The submission
// is mocked at the feature service; the reading store carries the affordance.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const crypto = vi.hoisted(() => ({
  submitReadingPassword: vi.fn(),
}));

vi.mock('@/services/crypto', () => crypto);

import { PasswordDialog } from './PasswordDialog';
import { useReadingStore } from '@/stores/readingStore';
import type { ReadingSnapshot } from '@/services/ipc';

/** The open message's snapshot, overridable per test. */
function snapshot(
  prompt: ReadingSnapshot['password_prompt'],
  key = 'imap:v1:u1',
): ReadingSnapshot {
  return {
    message: { account: 'work', key },
    subject: '...',
    from_name: 'Ada Lovelace',
    from_address: 'ada@example.test',
    sent_at: '2026-10-09T08:30:00Z',
    remote_image_count: 0,
    password_prompt: prompt,
  };
}

beforeEach(() => {
  crypto.submitReadingPassword.mockReset();
});

afterEach(cleanup);

describe('the password dialog', () => {
  it('stands absent without a password prompt', () => {
    useReadingStore.getState().applySnapshot(snapshot(undefined));
    const { container } = render(<PasswordDialog />);
    expect(container.firstChild).toBeNull();
  });

  it('stands absent with nothing open', () => {
    useReadingStore.getState().applySnapshot(null);
    const { container } = render(<PasswordDialog />);
    expect(container.firstChild).toBeNull();
  });

  it('mounts on the needs-password affordance and submits the passphrase', async () => {
    useReadingStore.getState().applySnapshot(snapshot('needs_password'));
    crypto.submitReadingPassword.mockResolvedValue(null);
    render(<PasswordDialog />);

    expect(screen.getByRole('dialog')).toBeTruthy();
    const input = screen.getByLabelText('Passphrase') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'hunter2' } });
    fireEvent.click(screen.getByRole('button', { name: 'Open message' }));

    await waitFor(() => {
      expect(crypto.submitReadingPassword).toHaveBeenCalledWith(
        { account: 'work', key: 'imap:v1:u1' },
        'hunter2',
      );
    });
  });

  it('re-prompts on a wrong passphrase, naming the refusal and emptying the input', async () => {
    useReadingStore.getState().applySnapshot(snapshot('needs_password'));
    crypto.submitReadingPassword.mockResolvedValue('wrong_password');
    render(<PasswordDialog />);

    const input = screen.getByLabelText('Passphrase') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'not-the-passphrase' } });
    fireEvent.click(screen.getByRole('button', { name: 'Open message' }));

    await waitFor(() => {
      expect(screen.getByRole('alert').textContent).toContain('The passphrase was not correct');
    });
    expect(input.value).toBe('');
    // The dialog stands: the verdict is still a prompt, fail closed.
    expect(screen.getByRole('dialog')).toBeTruthy();
  });

  it('closes once the passphrase decrypts', async () => {
    useReadingStore.getState().applySnapshot(snapshot('needs_password'));
    crypto.submitReadingPassword.mockResolvedValue(null);
    render(<PasswordDialog />);

    fireEvent.change(screen.getByLabelText('Passphrase'), { target: { value: 'hunter2' } });
    fireEvent.click(screen.getByRole('button', { name: 'Open message' }));

    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });
  });

  it('closes on cancel without submitting, leaving the verdict needs-password', () => {
    useReadingStore.getState().applySnapshot(snapshot('needs_password'));
    render(<PasswordDialog />);

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(screen.queryByRole('dialog')).toBeNull();
    expect(crypto.submitReadingPassword).not.toHaveBeenCalled();
    // The snapshot's affordance stands: a cancel never decrypts or clears it.
    expect(useReadingStore.getState().snapshot?.password_prompt).toBe('needs_password');
  });

  it('resets when another message opens', async () => {
    useReadingStore.getState().applySnapshot(snapshot('needs_password', 'imap:v1:u1'));
    crypto.submitReadingPassword.mockResolvedValue('wrong_password');
    render(<PasswordDialog />);

    fireEvent.change(screen.getByLabelText('Passphrase'), { target: { value: 'wrong' } });
    fireEvent.click(screen.getByRole('button', { name: 'Open message' }));
    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeTruthy();
    });

    act(() => {
      useReadingStore.getState().applySnapshot(snapshot('needs_password', 'imap:v1:u2'));
    });

    // The new message starts empty, with no refusal carried over.
    expect(screen.queryByRole('alert')).toBeNull();
    expect((screen.getByLabelText('Passphrase') as HTMLInputElement).value).toBe('');
  });
});