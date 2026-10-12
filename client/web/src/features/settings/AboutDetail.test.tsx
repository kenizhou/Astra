// SPDX-License-Identifier: GPL-3.0-only
// The About detail: the version and the attributions, the core's answer
// straight from the snapshot, never the web's own guess.
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { AboutDetail } from './AboutDetail';
import { useSettingsStore } from './settingsStore';

beforeEach(() => {
  useSettingsStore.getState().applySnapshot({
    settings: {
      theme: 'follow_host',
      twenty_four_clock: false,
      zone: 'Etc/UTC',
      first_day_of_week: 'mo',
      horizon_days: 42,
      quote_below: true,
      view_mode: 'flat',
    },
    accounts: [],
    about: {
      version: '0.1.0',
      attributions: [
        'email-calendar-sync-engine (the mail and calendar engine)',
        'the pi SDK (the agent sidecar)',
        'React, TypeScript, and Vite (the web interface)',
      ],
    },
  });
});

afterEach(cleanup);

describe('the About detail', () => {
  it('renders the version from the snapshot', () => {
    render(<AboutDetail />);
    expect(screen.getByText('Version 0.1.0')).toBeTruthy();
  });

  it('renders every attribution the snapshot carries', () => {
    render(<AboutDetail />);
    expect(screen.getByText('email-calendar-sync-engine (the mail and calendar engine)')).toBeTruthy();
    expect(screen.getByText('the pi SDK (the agent sidecar)')).toBeTruthy();
    expect(screen.getByText('React, TypeScript, and Vite (the web interface)')).toBeTruthy();
  });
});
