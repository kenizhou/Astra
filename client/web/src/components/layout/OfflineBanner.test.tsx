// SPDX-License-Identifier: GPL-3.0-only
// The offline banner: it stands only while every dialled account is in
// outage, its words name reconnecting and never the word error, it leaves
// with the first healthy pass the sync-progress snapshot reports, and it
// overlays the shell rather than joining the flow, so its arrival and
// departure shift no layout.
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { OfflineBanner } from './OfflineBanner';
import { useUiStore } from '@/stores/uiStore';

beforeEach(() => {
  useUiStore.getState().applySyncProgress({ accounts: [] });
});

afterEach(cleanup);

describe('the offline banner', () => {
  it('renders nothing while the snapshot reports no outage', () => {
    const { container } = render(<OfflineBanner />);

    expect(container.firstChild).toBeNull();
  });

  it('states reconnecting, never the word error, while the banner stands', () => {
    act(() => {
      useUiStore.getState().applySyncProgress({
        accounts: [],
        delivery: { offline_banner: true, outages: ['work'] },
      });
    });
    render(<OfflineBanner />);

    const banner = screen.getByRole('status');
    expect(banner.textContent).toContain('econnecting');
    expect(banner.textContent?.toLowerCase()).not.toContain('error');
  });

  it('leaves with the first snapshot that clears the banner', () => {
    act(() => {
      useUiStore.getState().applySyncProgress({
        accounts: [],
        delivery: { offline_banner: true, outages: ['work'] },
      });
    });
    const { container } = render(<OfflineBanner />);
    expect(screen.getByRole('status')).toBeTruthy();

    act(() => {
      useUiStore.getState().applySyncProgress({
        accounts: [],
        delivery: { offline_banner: false, outages: [] },
      });
    });
    expect(container.firstChild).toBeNull();
  });

  it('overlays the shell, so showing and hiding shift no layout', () => {
    act(() => {
      useUiStore.getState().applySyncProgress({
        accounts: [],
        delivery: { offline_banner: true, outages: ['work'] },
      });
    });
    render(<OfflineBanner />);

    expect(screen.getByRole('status').className).toContain('absolute');
  });
});
