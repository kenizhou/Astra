// SPDX-License-Identifier: GPL-3.0-only
// The diagnostics command wire: the tail asks by its command name with the
// line count, and the export asks by its own name and answers the shared
// save outcome family.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import { logExport, logTail } from './diagnostics';

beforeEach(() => {
  invoke.mockReset();
});

describe('the diagnostics commands', () => {
  it('asks the tail by name with the line count', async () => {
    invoke.mockResolvedValue(['one', 'two']);
    const lines = await logTail(200);
    expect(lines).toEqual(['one', 'two']);
    expect(invoke).toHaveBeenCalledWith('log_tail', { n: 200 });
  });

  it('asks the export by name and answers the shared outcome family', async () => {
    invoke.mockResolvedValue({ saved: { path: 'C:\\Users\\ada\\astra.log' } });
    const outcome = await logExport();
    expect(outcome).toEqual({ saved: { path: 'C:\\Users\\ada\\astra.log' } });
    expect(invoke).toHaveBeenCalledWith('log_export');
  });
});
