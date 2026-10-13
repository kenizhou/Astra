// SPDX-License-Identifier: GPL-3.0-only
// The tool rail: the view buttons switch the shell's view, the drawer toggle
// opens and shuts the folder drawer flag, and the agent and AI documents
// toggles flip their panels' flags. The outbox lives in the folder tree now,
// so the rail carries no toggle for it.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { ToolWindowBar } from './ToolWindowBar';
import { useUiStore } from '@/stores/uiStore';

beforeEach(() => {
  useUiStore.setState({
    shellView: 'mail',
    folderDrawerOpen: false,
    agentOpen: false,
    aiPanelOpen: false,
  });
});

afterEach(cleanup);

describe('the tool rail view buttons', () => {
  it('marks mail as the pressed view by default', () => {
    render(<ToolWindowBar />);

    expect(screen.getByRole('button', { name: 'Mail' }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('button', { name: 'Calendar' }).getAttribute('aria-pressed')).toBe(
      'false',
    );
    expect(screen.getByRole('button', { name: 'People' }).getAttribute('aria-pressed')).toBe(
      'false',
    );
  });

  it('switches the shell view from the rail', () => {
    render(<ToolWindowBar />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Calendar' }));
    });

    expect(useUiStore.getState().shellView).toBe('calendar');
    expect(screen.getByRole('button', { name: 'Calendar' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
    expect(screen.getByRole('button', { name: 'Mail' }).getAttribute('aria-pressed')).toBe(
      'false',
    );

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'People' }));
    });

    expect(useUiStore.getState().shellView).toBe('people');
  });
});

describe('the tool rail drawer toggle', () => {
  it('opens and shuts the folder drawer flag', () => {
    render(<ToolWindowBar />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Toggle folder pane' }));
    });
    expect(useUiStore.getState().folderDrawerOpen).toBe(true);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Toggle folder pane' }));
    });
    expect(useUiStore.getState().folderDrawerOpen).toBe(false);
  });
});

describe('the tool rail panel toggles', () => {
  it('carries no outbox toggle; the folder tree owns the outbox now', () => {
    render(<ToolWindowBar />);

    expect(screen.queryByRole('button', { name: /Outbox/ })).toBeNull();
  });

  it('toggles the agent panel flag', () => {
    render(<ToolWindowBar />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Agent' }));
    });
    expect(useUiStore.getState().agentOpen).toBe(true);
  });

  it('toggles the AI documents panel flag', () => {
    render(<ToolWindowBar />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'AI' }));
    });
    expect(useUiStore.getState().aiPanelOpen).toBe(true);
  });
});
