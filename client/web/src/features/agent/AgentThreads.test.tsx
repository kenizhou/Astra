// SPDX-License-Identifier: GPL-3.0-only
// The agent panel's thread surface: the thread list paints the store's
// rows, selection loads a thread, rename and delete dispatch through the
// host, the new-chat gesture starts a fresh thread, a send adopts the
// minted chat, and a background turn's live tail stays out of the current
// log until its thread is selected again. Split from AgentPanel.test.tsx
// for the file-size gate.
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
  agentPrompt: vi.fn(),
  agentAbort: vi.fn(),
  agentStatus: vi.fn(),
  permissionResponse: vi.fn(),
  chatList: vi.fn(),
  chatLoad: vi.fn(),
  chatRename: vi.fn(),
  chatDelete: vi.fn(),
}));

vi.mock('@/services/agent', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  agentPrompt: ipc.agentPrompt,
  chatList: ipc.chatList,
  chatLoad: ipc.chatLoad,
  chatRename: ipc.chatRename,
  chatDelete: ipc.chatDelete,
}));

import { useUiStore } from '@/stores/uiStore';

import { useAgentStore } from './agentStore';
import { AgentPanel } from './AgentPanel';

function reset(): void {
  ipc.agentPrompt.mockReset();
  ipc.agentAbort.mockReset();
  ipc.agentStatus.mockReset();
  ipc.permissionResponse.mockReset();
  ipc.chatList.mockReset();
  ipc.chatLoad.mockReset();
  ipc.chatRename.mockReset();
  ipc.chatDelete.mockReset();
  ipc.agentPrompt.mockResolvedValue({ prompt_id: 'p-1', chat_id: 'chat-1' });
  ipc.agentAbort.mockResolvedValue(undefined);
  ipc.agentStatus.mockResolvedValue({ state: 'down', model: null });
  ipc.permissionResponse.mockResolvedValue(undefined);
  ipc.chatList.mockResolvedValue([]);
  ipc.chatLoad.mockResolvedValue({
    chat: { id: 'chat-1', title: 'Thread one', created_at: 1, last_touched_at: 2 },
    messages: [],
  });
  ipc.chatRename.mockResolvedValue(true);
  ipc.chatDelete.mockResolvedValue(true);
  useAgentStore.getState().reset();
  useUiStore.getState().setAgentOpen(false);
}

beforeEach(reset);

afterEach(cleanup);

describe('the thread list', () => {
  const rows = (id: string): { chat: unknown; messages: unknown[] } => ({
    chat: { id, title: id === 'chat-1' ? 'Friday plans' : 'Search the inbox', created_at: 1, last_touched_at: 2 },
    messages:
      id === 'chat-1'
        ? [
            {
              id: 1,
              chat_id: id,
              role: 'user',
              parts: [{ kind: 'text', text: 'draft the reply' }],
              metadata: null,
              created_at: 1,
            },
            {
              id: 2,
              chat_id: id,
              role: 'assistant',
              parts: [
                { kind: 'text', text: 'Draft saved.' },
                { kind: 'tool', name: 'save_draft', status: 'ok' },
                { kind: 'note', text: 'the tool budget is spent; the assistant finishes in text' },
              ],
              metadata: { model: 'mock-1', finish_reason: 'done', tool_calls: 1 },
              created_at: 2,
            },
          ]
        : [],
  });

  function openWithThreads(): void {
    ipc.chatList.mockResolvedValue([
      { id: 'chat-1', title: 'Friday plans', created_at: 1, last_touched_at: 3 },
      { id: 'chat-2', title: 'Search the inbox', created_at: 1, last_touched_at: 2 },
    ]);
    useUiStore.getState().setAgentOpen(true);
    render(<AgentPanel />);
  }

  it('paints the threads when the panel opens', async () => {
    openWithThreads();
    await act(async () => {
      await Promise.resolve();
    });

    const list = screen.getByRole('list', { name: 'Agent threads' });
    expect(within(list).getByRole('button', { name: 'Friday plans' })).toBeTruthy();
    expect(within(list).getByRole('button', { name: 'Search the inbox' })).toBeTruthy();
  });

  it('selects a thread and paints its turns from the store rows alone', async () => {
    ipc.chatLoad.mockResolvedValue(rows('chat-1'));
    openWithThreads();
    await act(async () => {
      await Promise.resolve();
    });

    await act(async () => {
      fireEvent.click(within(screen.getByRole('list', { name: 'Agent threads' })).getByRole('button', { name: 'Friday plans' }));
    });
    expect(ipc.chatLoad).toHaveBeenCalledWith('chat-1');

    const log = screen.getByRole('log', { name: 'Agent conversation' });
    expect(within(log).getByText('draft the reply')).toBeTruthy();
    expect(within(log).getByText('Draft saved.')).toBeTruthy();
    expect(within(log).getByText('save_draft: ok')).toBeTruthy();
    expect(
      within(log).getByText('the tool budget is spent; the assistant finishes in text'),
    ).toBeTruthy();
  });

  it('renames the thread through the host and refreshes the list', async () => {
    openWithThreads();
    await act(async () => {
      await Promise.resolve();
    });

    const list = screen.getByRole('list', { name: 'Agent threads' });
    await act(async () => {
      fireEvent.click(within(list).getByRole('button', { name: 'Rename Friday plans' }));
    });
    const input = within(list).getByRole('textbox', { name: 'Rename thread' }) as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'Renamed thread' } });
    await act(async () => {
      fireEvent.click(within(list).getByRole('button', { name: 'Save' }));
    });

    expect(ipc.chatRename).toHaveBeenCalledWith('chat-1', 'Renamed thread');
    expect(ipc.chatList).toHaveBeenCalledTimes(2);
  });

  it('deletes the thread and clears the view when it was the selected one', async () => {
    ipc.chatLoad.mockResolvedValue(rows('chat-1'));
    openWithThreads();
    await act(async () => {
      await Promise.resolve();
    });
    await act(async () => {
      fireEvent.click(within(screen.getByRole('list', { name: 'Agent threads' })).getByRole('button', { name: 'Friday plans' }));
    });

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Delete Friday plans' }));
    });
    expect(ipc.chatDelete).toHaveBeenCalledWith('chat-1');
    expect(useAgentStore.getState().currentChatId).toBeNull();

    const log = screen.getByRole('log', { name: 'Agent conversation' });
    expect(within(log).queryByText('draft the reply')).toBeNull();
  });

  it('starts a fresh thread on the new-chat gesture', async () => {
    ipc.chatLoad.mockResolvedValue(rows('chat-1'));
    openWithThreads();
    await act(async () => {
      await Promise.resolve();
    });
    await act(async () => {
      fireEvent.click(within(screen.getByRole('list', { name: 'Agent threads' })).getByRole('button', { name: 'Friday plans' }));
    });

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'New chat' }));
    });
    expect(useAgentStore.getState().currentChatId).toBeNull();
    expect(screen.getByRole('log', { name: 'Agent conversation' }).textContent).toBe('');
  });
});

describe('the live turn and the threads', () => {
  it('adopts the minted chat and rides it on the next send', async () => {
    ipc.agentPrompt.mockResolvedValue({ prompt_id: 'p-7', chat_id: 'chat-minted' });
    useUiStore.getState().setAgentOpen(true);
    render(<AgentPanel />);

    const input = screen.getByRole('textbox', { name: 'Message the agent' });
    fireEvent.change(input, { target: { value: 'plan my week' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    });
    expect(ipc.agentPrompt).toHaveBeenCalledWith('plan my week', undefined, undefined);
    expect(useAgentStore.getState().currentChatId).toBe('chat-minted');

    ipc.agentPrompt.mockClear();
    ipc.agentPrompt.mockResolvedValue({ prompt_id: 'p-8', chat_id: 'chat-minted' });
    await act(async () => {
      useAgentStore.getState().applyStreamEvent({
        session_id: 'agent:chat:chat-minted',
        event: { kind: 'done', prompt_id: 'p-7' },
      });
    });
    await act(async () => {
      await useAgentStore.getState().sendPrompt('and the next');
    });
    expect(ipc.agentPrompt).toHaveBeenCalledWith('and the next', undefined, 'chat-minted');
  });

  it('keeps a background turn out of the current log and shows it on return', async () => {
    await act(async () => {
      await useAgentStore.getState().sendPrompt('long question');
    });
    // The user flips to another thread while the turn streams.
    ipc.chatLoad.mockResolvedValue({
      chat: { id: 'chat-other', title: 'Other thread', created_at: 1, last_touched_at: 2 },
      messages: [{ id: 9, chat_id: 'chat-other', role: 'user', parts: [{ kind: 'text', text: 'older turn' }], metadata: null, created_at: 1 }],
    });
    await act(async () => {
      await useAgentStore.getState().selectChat('chat-other');
    });

    act(() => {
      useAgentStore.getState().applyStreamEvent({
        session_id: 'agent:chat:chat-1',
        event: { kind: 'text_delta', text: 'streaming away' },
      });
    });
    useUiStore.getState().setAgentOpen(true);
    render(<AgentPanel />);
    expect(within(screen.getByRole('log', { name: 'Agent conversation' })).queryByText('streaming away')).toBeNull();

    // Flipping back shows the live tail beside the painted rows.
    await act(async () => {
      await useAgentStore.getState().selectChat('chat-1');
    });
    expect(within(screen.getByRole('log', { name: 'Agent conversation' })).getByText('streaming away')).toBeTruthy();

    // The done folds the tail into the row: the visible log keeps it and
    // the live buffer lets go.
    act(() => {
      useAgentStore.getState().applyStreamEvent({
        session_id: 'agent:chat:chat-1',
        event: { kind: 'done', prompt_id: 'p-1' },
      });
    });
    expect(useAgentStore.getState().turn).toBeNull();
    expect(within(screen.getByRole('log', { name: 'Agent conversation' })).getByText('streaming away')).toBeTruthy();
  });
});
