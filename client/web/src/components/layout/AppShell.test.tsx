// SPDX-License-Identifier: GPL-3.0-only
// The shell's composer and outbox wiring: the New message action opens the
// composer pane with the first account as its From, stays inert without an
// account, the app start resume pulls the stored draft into the pane when
// the host has one, a landed compose request opens the pane hydrated, and
// the tool rail's outbox toggle opens the outbox panel. The rail's agent
// toggle docks the agent panel as the mail row's right column, gone off the
// mail view. The settings screen rides above the shell from the title
// bar's gear, through the categories, down to the accounts page and About.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// jsdom has no ResizeObserver; the resizable panel row measures through one.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
vi.stubGlobal('ResizeObserver', ResizeObserverStub);

const ipc = vi.hoisted(() => ({ dispatch: vi.fn(), draftResume: vi.fn(), settings: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  draftResume: ipc.draftResume,
  settings: ipc.settings,
}));

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: true,
}));

const agent = vi.hoisted(() => ({
  agentStatus: vi.fn(),
  agentPrompt: vi.fn(),
  agentAbort: vi.fn(),
  permissionResponse: vi.fn(),
}));

vi.mock('@/services/agent', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...agent,
}));

import { AppShell } from './AppShell';
import type { ComposeRequestSnapshot, ResumeView } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useComposeRequestStore } from '@/stores/composeRequestStore';
import { useSendingStore } from '@/stores/sendingStore';
import { useUiStore } from '@/stores/uiStore';
import { useAgentStore } from '@/features/agent/agentStore';
import { useComposerStore } from '@/features/composer/composerStore';
import { documentToEditor } from '@/features/composer/documentWire';
import { useSendResultStore } from '@/features/outbox/sendResult';

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.draftResume.mockReset();
  ipc.draftResume.mockResolvedValue(null);
  ipc.settings.mockReset();
  ipc.settings.mockResolvedValue({
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
    about: { version: '0.1.0' },
  });
  useUiStore.getState().setShellView('mail');
  useUiStore.getState().setSettingsOpen(false);
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work' }] });
  useComposerStore.setState({
    open: false,
    resumeAttempted: false,
    composition: null,
    from: '',
    fromLabel: '',
    to: '',
    cc: '',
    bcc: '',
    subject: '',
    editor: documentToEditor({ blocks: [{ Paragraph: { content: [] } }], attachments: [] }),
    savedFingerprint: null,
    request: null,
  });
  useComposeRequestStore.setState({ snapshot: null });
  useSendingStore.getState().applySnapshot({ rows: [] });
  useUiStore.getState().setOutboxOpen(false);
  useUiStore.getState().setAgentOpen(false);
  useUiStore.getState().setFolderDrawerOpen(false);
  useSendResultStore.getState().dismiss();
  useSendResultStore.setState({ settled: [], answered: [] });
  agent.agentStatus.mockReset();
  agent.agentStatus.mockResolvedValue({ state: 'down', model: null });
  agent.agentPrompt.mockReset();
  agent.agentAbort.mockReset();
  agent.permissionResponse.mockReset();
  useAgentStore.getState().reset();
});

afterEach(cleanup);

describe('the shell composer wiring', () => {
  it('opens the composer pane from the new message action', () => {
    render(<AppShell />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'New message' }));
    });

    expect(screen.getByRole('region', { name: 'Composer' })).toBeTruthy();
    expect(screen.getByText('Work')).toBeTruthy();
  });

  it('keeps the new message action inert without an account', () => {
    useAccountStore.setState({ accounts: [] });
    render(<AppShell />);

    const action = screen.getByRole('button', { name: 'New message' }) as HTMLButtonElement;
    expect(action.disabled).toBe(true);

    act(() => {
      action.click();
    });

    expect(screen.queryByRole('region', { name: 'Composer' })).toBeNull();
  });

  it('keeps the new message action inert over the people view', () => {
    render(<AppShell />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'People' }));
    });

    const action = screen.getByRole('button', { name: 'New message' }) as HTMLButtonElement;
    expect(action.disabled).toBe(true);
  });

  it('resumes the stored draft into the pane at app start', async () => {
    const view: ResumeView = {
      account: 'work',
      to: 'you@test.local',
      cc: '',
      bcc: '',
      subject: 'Later',
      body_text: 'Half written',
    };
    ipc.draftResume.mockResolvedValue(view);
    render(<AppShell />);

    await act(async () => {
      await Promise.resolve();
    });

    expect(ipc.draftResume).toHaveBeenCalledTimes(1);
    expect((screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement).value).toBe(
      'you@test.local',
    );
    expect((screen.getByRole('textbox', { name: 'Subject' }) as HTMLInputElement).value).toBe(
      'Later',
    );
  });

  it('opens the composer hydrated when a compose request lands', () => {
    const request: ComposeRequestSnapshot = {
      kind: 'reply',
      from: 'work',
      to: 'ada@example.test',
      cc: '',
      subject: 'Re: Quarterly report',
      in_reply_to: 'orig-1@server.test',
      document: { blocks: [], attachments: [] },
    };
    render(<AppShell />);

    act(() => {
      useComposeRequestStore.getState().applySnapshot(request);
    });

    expect((screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement).value).toBe(
      'ada@example.test',
    );
    expect((screen.getByRole('textbox', { name: 'Subject' }) as HTMLInputElement).value).toBe(
      'Re: Quarterly report',
    );
  });
});

describe('the shell outbox wiring', () => {
  it('opens the outbox panel from the tool rail and asks the host for the queue', () => {
    render(<AppShell />);

    expect(screen.queryByRole('region', { name: 'Outbox' })).toBeNull();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Outbox' }));
    });

    expect(screen.getByRole('region', { name: 'Outbox' })).toBeTruthy();
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe('{"outbox":"show"}');
  });
});

describe('the shell settings screen', () => {
  it('opens the modal from the gear and returns to the shell on Close', async () => {
    render(<AppShell />);
    expect(screen.queryByRole('dialog', { name: 'Settings' })).toBeNull();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    });

    await waitFor(() => expect(screen.getByRole('dialog', { name: 'Settings' })).toBeTruthy());
    expect(screen.getByRole('button', { name: 'General' })).toBeTruthy();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Close settings' }));
    });
    expect(screen.queryByRole('dialog', { name: 'Settings' })).toBeNull();
  });

  it('reaches the About detail through the source list', async () => {
    render(<AppShell />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    });
    await waitFor(() => expect(ipc.settings).toHaveBeenCalledTimes(1));
    await waitFor(() => screen.getByRole('button', { name: 'About' }));

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'About' }));
    });

    expect(await screen.findByText('Version 0.1.0')).toBeTruthy();
  });

  it('reaches an account page with the dav editor from the accounts rows', async () => {
    ipc.settings.mockResolvedValue({
      settings: {
        theme: 'follow_host',
        twenty_four_clock: false,
        zone: 'Etc/UTC',
        first_day_of_week: 'mo',
        horizon_days: 42,
        quote_below: true,
        view_mode: 'flat',
      },
      accounts: [
        {
          account: 'work',
          address: 'ada@example.test',
          kind: 'imap_smtp',
          legs: { calendar: true, contacts: false },
          dav: {
            caldav_url: 'https://dav.example.test',
            username: 'ada@example.test',
            dav_password_set: true,
          },
        },
      ],
      about: { version: '0.1.0' },
    });
    render(<AppShell />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    });
    await waitFor(() => screen.getByRole('button', { name: 'Accounts' }));

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Accounts' }));
    });
    const row = await screen.findByRole('button', { name: 'Open ada@example.test' });
    expect(row.textContent).toContain('IMAP + SMTP');

    act(() => {
      fireEvent.click(row);
    });

    expect(await screen.findByLabelText('Username')).toBeTruthy();
    expect((screen.getByLabelText('Username') as HTMLInputElement).value).toBe(
      'ada@example.test',
    );
  });
});

describe('the shell agent dock', () => {
  it('docks the agent panel in the mail row from the rail toggle', () => {
    render(<AppShell />);

    expect(screen.queryByRole('region', { name: 'Agent' })).toBeNull();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Agent' }));
    });

    expect(screen.getByRole('region', { name: 'Agent' })).toBeTruthy();
    expect(agent.agentStatus).toHaveBeenCalledTimes(1);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Close agent' }));
    });
    expect(screen.queryByRole('region', { name: 'Agent' })).toBeNull();
  });

  it('leaves the dock behind off the mail view', () => {
    render(<AppShell />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Agent' }));
    });
    expect(screen.getByRole('region', { name: 'Agent' })).toBeTruthy();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Calendar' }));
    });
    expect(screen.queryByRole('region', { name: 'Agent' })).toBeNull();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Mail' }));
    });
    expect(screen.getByRole('region', { name: 'Agent' })).toBeTruthy();
  });
});

describe('the shell view switch', () => {
  it('shows the mail panes by default', () => {
    render(<AppShell />);

    expect(screen.getByRole('region', { name: 'Message list' })).toBeTruthy();
    expect(screen.queryByRole('region', { name: 'Calendar' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Mail' }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('button', { name: 'Calendar' }).getAttribute('aria-pressed')).toBe(
      'false',
    );
    expect(screen.getByRole('button', { name: 'People' }).getAttribute('aria-pressed')).toBe(
      'false',
    );
  });

  it('switches to the calendar and back', () => {
    render(<AppShell />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Calendar' }));
    });

    expect(useUiStore.getState().shellView).toBe('calendar');
    expect(screen.getByRole('region', { name: 'Calendar' })).toBeTruthy();
    expect(screen.queryByRole('region', { name: 'Message list' })).toBeNull();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Mail' }));
    });

    expect(useUiStore.getState().shellView).toBe('mail');
    expect(screen.getByRole('region', { name: 'Message list' })).toBeTruthy();
    expect(screen.queryByRole('region', { name: 'Calendar' })).toBeNull();
  });

  it('switches to the people view beside mail and calendar', () => {
    render(<AppShell />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'People' }));
    });

    expect(useUiStore.getState().shellView).toBe('people');
    expect(screen.getByRole('region', { name: 'People' })).toBeTruthy();
    expect(screen.queryByRole('region', { name: 'Message list' })).toBeNull();
    expect(screen.queryByRole('region', { name: 'Calendar' })).toBeNull();
    expect(screen.getByRole('button', { name: 'People' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
  });
});
