// SPDX-License-Identifier: GPL-3.0-only
// Tests for the plugin banner: the level picks the tone and the spoken word,
// and the plugin's text renders verbatim.

import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import type { SecurityBannerView } from '@/services/plugins';

import { PluginBanner } from './PluginBanner';

afterEach(cleanup);

function banner(level: SecurityBannerView['level']): SecurityBannerView {
  return { level, text: 'Signature invalid' };
}

describe('the plugin banner', () => {
  it('renders the plugin words verbatim with the level spoken', () => {
    render(<PluginBanner banner={banner('warning')} />);
    const bar = screen.getByRole('status');
    expect(bar.textContent).toBe('Warning: Signature invalid');
    expect(bar.className).toContain('text-warning');
  });

  it('reads error in the destructive tone and info in the muted one', () => {
    const { rerender } = render(<PluginBanner banner={banner('error')} />);
    expect(screen.getByRole('status').className).toContain('text-destructive');
    expect(screen.getByRole('status').textContent).toContain('Error:');
    rerender(<PluginBanner banner={banner('info')} />);
    expect(screen.getByRole('status').className).toContain('text-muted-foreground');
    expect(screen.getByRole('status').textContent).toContain('Note:');
  });
});
