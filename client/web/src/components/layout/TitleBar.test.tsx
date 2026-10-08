// SPDX-License-Identifier: GPL-3.0-only
// The double-click maximise is scoped to the drag regions: a double click on
// the buttons rides the bubble to the header, so the handler lives on the
// regions alone and a control can never net an extra toggle.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const windowApi = vi.hoisted(() => ({
  toggleMaximize: vi.fn(async () => undefined),
  minimize: vi.fn(async () => undefined),
  close: vi.fn(async () => undefined),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    toggleMaximize: windowApi.toggleMaximize,
    minimize: windowApi.minimize,
    close: windowApi.close,
    isMaximized: async () => false,
    onResized: async () => () => undefined,
  }),
}));

import { TitleBar } from './TitleBar';
import { useUiStore } from '@/stores/uiStore';

beforeEach(() => {
  windowApi.toggleMaximize.mockClear();
  useUiStore.setState({ themeMode: 'light' });
});

afterEach(() => {
  cleanup();
});

describe('the title bar double-click maximise', () => {
  it('toggles maximise from a drag region', async () => {
    render(<TitleBar />);
    fireEvent.doubleClick(screen.getByText('Astra Mail'));
    await waitFor(() => expect(windowApi.toggleMaximize).toHaveBeenCalledTimes(1));
  });

  it('never toggles maximise from the buttons the header also holds', async () => {
    render(<TitleBar />);
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Add account' }));
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Toggle theme' }));
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Minimise' }));
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Maximise' }));
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Close window' }));

    // The dblclick events bubble to the header; nothing there may answer.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(windowApi.toggleMaximize).not.toHaveBeenCalled();
  });
});
