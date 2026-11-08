// SPDX-License-Identifier: GPL-3.0-only
// The device zone's web-side arm: the browser's own resolution reported
// through the core's arm, which owns the validation, the once-per-value
// latch, and the picked-zone refusal. A resolution that answers nothing
// reports nothing.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { reportDeviceZone, resolvedDeviceZone } from './deviceZone';

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
});

/** Stands the standard formatter in with one that answers `zone`. */
function standZone(zone: string | undefined): void {
  vi.stubGlobal('Intl', {
    DateTimeFormat: class {
      resolvedOptions(): { timeZone?: string } {
        return { timeZone: zone };
      }
    },
  });
}

describe('the web-side device zone report', () => {
  it('resolves the browser zone through the standard formatter', () => {
    expect(resolvedDeviceZone()).toBe(
      new Intl.DateTimeFormat().resolvedOptions().timeZone ?? null,
    );
  });

  it('reports the resolved zone through the core arm', () => {
    standZone('Asia/Shanghai');
    try {
      reportDeviceZone();
    } finally {
      vi.unstubAllGlobals();
    }

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"report_device_zone":{"zone":"Asia/Shanghai"}}}',
    );
  });

  it('reports nothing when the resolution answers nothing', () => {
    standZone(undefined);
    try {
      reportDeviceZone();
    } finally {
      vi.unstubAllGlobals();
    }

    expect(ipc.dispatch).not.toHaveBeenCalled();
  });
});
