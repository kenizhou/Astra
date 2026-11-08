// SPDX-License-Identifier: GPL-3.0-only
// The navigation arms: the anchor date and the loadable window each view
// needs, derived in the snapshot's own zone and dispatched together so the
// core's window and its anchor never disagree, the range write carrying the
// view so the shape preference the next launch restores moves with it.
import { cleanup } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { navigateTo } from './navigate';

const ZONE = 'Europe/Amsterdam';

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe('the navigation writes', () => {
  it('carries the view beside the window, so the shape preference moves', () => {
    navigateTo('2026-10-21', 'work_week', ZONE);

    expect(ipc.dispatch).toHaveBeenCalledTimes(2);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"set_date":{"date":"2026-10-21"}}}',
    );
    // The week of the 19th opens at Amsterdam midnight (22:00Z on the 18th).
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"calendar":{"set_range":{"start":"2026-10-18T22:00:00Z","days":7,"view":"work_week"}}}',
    );
  });

  it('names the month view the month window serves', () => {
    navigateTo('2026-10-15', 'month', ZONE);

    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toContain('"view":"month"');
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toContain('"days":42');
  });
});
