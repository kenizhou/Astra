// SPDX-License-Identifier: GPL-3.0-only
// The reading saves wire, pinned against the desktop host's command surface:
// both wrappers invoke the command their Rust side registers with the
// account-and-key payload the serde there reads, and the outcome mirrors the
// three shapes the commands' shared serde answers (pinned on the Rust side
// against these same literals).
import { describe, expect, it, vi, beforeEach } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import { saveAttachment, saveEml } from './attachments';
import type { SaveAttachmentOutcome } from './attachments';

beforeEach(() => {
  invoke.mockReset();
});

describe('the save attachment call', () => {
  it('invokes the command with the message reference and the row id', async () => {
    invoke.mockResolvedValue('cancelled');
    await saveAttachment({ account: 'work', key: 'imap:v1:u42' }, 0);
    expect(invoke).toHaveBeenCalledWith('save_attachment', {
      account: 'work',
      key: 'imap:v1:u42',
      attachment: 0,
    });
  });

  it('answers the outcome the call resolves with', async () => {
    invoke.mockResolvedValue({ saved: { path: 'C:\\saved\\elsewhere.bin' } });
    const outcome: SaveAttachmentOutcome = await saveAttachment(
      { account: 'work', key: 'imap:v1:u42' },
      3,
    );
    expect(outcome).toEqual({ saved: { path: 'C:\\saved\\elsewhere.bin' } });
  });
});

describe('the save eml call', () => {
  it('invokes the eml command with the message reference alone', async () => {
    invoke.mockResolvedValue('cancelled');
    await saveEml({ account: 'work', key: 'imap:v1:u42' });
    expect(invoke).toHaveBeenCalledWith('eml_save', {
      account: 'work',
      key: 'imap:v1:u42',
    });
  });

  it('answers the same outcome family the attachment save answers', async () => {
    invoke.mockResolvedValue({ saved: { path: 'C:\\mail\\Launch.eml' } });
    const outcome: SaveAttachmentOutcome = await saveEml({ account: 'work', key: 'm1' });
    expect(outcome).toEqual({ saved: { path: 'C:\\mail\\Launch.eml' } });
  });
});

describe('the outcome wire', () => {
  it('crosses the saved path under the saved tag', () => {
    const wire = JSON.parse('{"saved":{"path":"C:\\\\saved\\\\elsewhere.bin"}}');
    expect(wire).toEqual({ saved: { path: 'C:\\saved\\elsewhere.bin' } });
  });

  it('crosses the cancel as the bare word', () => {
    const wire = JSON.parse('"cancelled"') as SaveAttachmentOutcome;
    expect(wire).toBe('cancelled');
  });

  it('crosses the refusal under the failed tag with its reason', () => {
    const wire = JSON.parse('{"failed":{"reason":"the disk is full"}}');
    expect(wire).toEqual({ failed: { reason: 'the disk is full' } });
  });
});
