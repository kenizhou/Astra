// SPDX-License-Identifier: GPL-3.0-only
// The agent panel: the chat log renders the streamed turn (user bubbles,
// a streaming assistant bubble, the tool notes), the input sends with the
// exact IPC payload, the approval card dispatches the answer, the abort
// button dispatches the cancel, and the status bar's toggle opens and
// shuts the panel.
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
  agentPrompt: vi.fn(),
  agentAbort: vi.fn(),
  permissionResponse: vi.fn(),
}));

vi.mock('@/services/agent', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  agentPrompt: ipc.agentPrompt,
  agentAbort: ipc.agentAbort,
  permissionResponse: ipc.permissionResponse,
}));

import type { PendingApproval } from '@/services/agent';
import { useUiStore } from '@/stores/uiStore';

import { useAgentStore } from './agentStore';
import { AgentPanel, AgentStatus } from './AgentPanel';

function reset(): void {
  ipc.agentPrompt.mockReset();
  ipc.agentAbort.mockReset();
  ipc.permissionResponse.mockReset();
  ipc.agentPrompt.mockResolvedValue('p-1');
  ipc.agentAbort.mockResolvedValue(undefined);
  ipc.permissionResponse.mockResolvedValue(undefined);
  useAgentStore.getState().reset();
  useUiStore.getState().setAgentOpen(false);
}

beforeEach(reset);

afterEach(cleanup);

describe('the status toggle', () => {
  it('opens and shuts the panel', () => {
    render(
      <>
        <AgentStatus />
        <AgentPanel />
      </>,
    );
    expect(screen.queryByRole('region', { name: 'Agent' })).toBeNull();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Agent' }));
    });
    expect(screen.getByRole('region', { name: 'Agent' })).toBeTruthy();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Close agent' }));
    });
    expect(screen.queryByRole('region', { name: 'Agent' })).toBeNull();
  });

  it('carries the approvals awaiting a decision', () => {
    act(() => {
      useAgentStore
        .getState()
        .addApproval({ id: 'a1', tool_call_id: 'c1', tool: 'save_draft', level: 'confirm', args: {} });
    });
    render(<AgentStatus />);

    expect(screen.getByRole('button', { name: 'Agent 1' })).toBeTruthy();
  });
});

describe('the chat log', () => {
  it('renders the streamed turn: the user bubble, the streaming assistant bubble, and the tool notes', () => {
    act(() => {
      const store = useAgentStore.getState();
      store.sendPrompt('summarise the inbox');
    });
    act(() => {
      const store = useAgentStore.getState();
      store.applyStreamEvent({ session_id: 'agent:main', event: { kind: 'text_delta', text: 'Hello' } });
      store.applyStreamEvent({ session_id: 'agent:main', event: { kind: 'text_delta', text: ' there' } });
      store.applyStreamEvent({
        session_id: 'agent:main',
        event: { kind: 'tool_call', name: 'list_messages', args: {}, tool_call_id: 'c1' },
      });
      store.applyStreamEvent({
        session_id: 'agent:main',
        event: { kind: 'tool_result', tool_call_id: 'c1', name: 'list_messages', status: 'ok' },
      });
    });
    useUiStore.getState().setAgentOpen(true);
    render(<AgentPanel />);

    const log = screen.getByRole('log', { name: 'Agent conversation' });
    expect(within(log).getByText('summarise the inbox')).toBeTruthy();
    expect(within(log).getByText('Hello there')).toBeTruthy();
    expect(within(log).getByText('called list_messages')).toBeTruthy();
    expect(within(log).getByText('list_messages: ok')).toBeTruthy();
  });

  it('folds a turn that ends in error into a note and clears the live turn', async () => {
    await act(async () => {
      await useAgentStore.getState().sendPrompt('hello');
    });
    act(() => {
      useAgentStore.getState().applyStreamEvent({
        session_id: 'agent:main',
        event: { kind: 'error', message: 'prompt before init' },
      });
    });
    useUiStore.getState().setAgentOpen(true);
    render(<AgentPanel />);

    const log = screen.getByRole('log', { name: 'Agent conversation' });
    expect(within(log).getByText('prompt before init')).toBeTruthy();
    expect(useAgentStore.getState().turn).toBeNull();
  });
});

describe('the input', () => {
  it('sends the prompt with the exact IPC payload and records the user side', async () => {
    useUiStore.getState().setAgentOpen(true);
    render(<AgentPanel />);

    const input = screen.getByRole('textbox', { name: 'Message the agent' });
    fireEvent.change(input, { target: { value: 'summarise today' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    });

    expect(ipc.agentPrompt).toHaveBeenCalledWith('summarise today');
    expect(useAgentStore.getState().turn).toBe('p-1');
    expect(useAgentStore.getState().messages[0]).toMatchObject({
      role: 'user',
      text: 'summarise today',
    });
  });

  it('sends nothing while a turn is live, offering the abort instead', async () => {
    await act(async () => {
      await useAgentStore.getState().sendPrompt('long question');
    });
    ipc.agentPrompt.mockClear();
    useUiStore.getState().setAgentOpen(true);
    render(<AgentPanel />);

    expect(screen.queryByRole('button', { name: 'Send' })).toBeNull();
    const input = screen.getByRole('textbox', { name: 'Message the agent' }) as HTMLInputElement;
    expect(input.disabled).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Abort' }));
    });
    expect(ipc.agentAbort).toHaveBeenCalledWith('p-1');
  });

  it('shows a note when the host refuses the prompt', async () => {
    ipc.agentPrompt.mockRejectedValue('the agent is unavailable');
    useUiStore.getState().setAgentOpen(true);
    render(<AgentPanel />);

    const input = screen.getByRole('textbox', { name: 'Message the agent' });
    fireEvent.change(input, { target: { value: 'hello' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    });

    const log = screen.getByRole('log', { name: 'Agent conversation' });
    expect(within(log).getByText('the prompt failed: the agent is unavailable')).toBeTruthy();
    expect(useAgentStore.getState().turn).toBeNull();
  });
});

describe('the approval card', () => {
  const ask: PendingApproval = {
    id: 'perm-1',
    tool_call_id: 'c9',
    tool: 'save_draft',
    level: 'confirm',
    args: { account: 'work', subject: 'Re: Friday' },
    correlation: 'corr-1',
  };

  function openWithAsk(): void {
    act(() => {
      useAgentStore.getState().addApproval(ask);
    });
    useUiStore.getState().setAgentOpen(true);
    render(<AgentPanel />);
    ipc.permissionResponse.mockClear();
  }

  it('renders the tool and its arguments', () => {
    openWithAsk();

    const card = screen.getByRole('article', { name: 'Approval save_draft' });
    expect(within(card).getByText('save_draft')).toBeTruthy();
    expect(within(card).getByText(/"account":"work"/)).toBeTruthy();
  });

  it('dispatches the approval with the exact IPC payload', async () => {
    openWithAsk();

    await act(async () => {
      fireEvent.click(within(screen.getByRole('article', { name: 'Approval save_draft' })).getByRole('button', { name: 'Approve' }));
    });
    expect(ipc.permissionResponse).toHaveBeenCalledWith('perm-1', true, false);
  });

  it('dispatches the refusal with the exact IPC payload', async () => {
    openWithAsk();

    await act(async () => {
      fireEvent.click(within(screen.getByRole('article', { name: 'Approval save_draft' })).getByRole('button', { name: 'Reject' }));
    });
    expect(ipc.permissionResponse).toHaveBeenCalledWith('perm-1', false, false);
  });

  it('takes the card away when the host reports the approval settled', () => {
    openWithAsk();

    act(() => {
      useAgentStore.getState().resolveApproval('perm-1');
    });
    expect(screen.queryByRole('article', { name: 'Approval save_draft' })).toBeNull();
  });
});

describe('the relay status', () => {
  it('renders the lifecycle word in the header', () => {
    act(() => {
      useAgentStore.getState().setStatus('lost');
    });
    useUiStore.getState().setAgentOpen(true);
    render(<AgentPanel />);

    expect(screen.getByText('lost')).toBeTruthy();
  });
});
