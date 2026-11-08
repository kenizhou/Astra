// SPDX-License-Identifier: GPL-3.0-only
// The chat log's entry vocabulary and the stored-parts mapping: the
// shapes the live fold and the loaded-thread render share, so a reloaded
// turn paints what the live turn rendered. Split from agentStore for the
// line gate.

import type { ChatMessage } from '@/services/agent';

/** One tool card: the call's name, its argument summary, its audited
 *  status, and the output it answered (already capped at the source; the
 *  card truncates again for display). Mirrors the stored tool part, so a
 *  loaded thread paints the card the live fold rendered. */
export interface ToolCard {
  name: string;
  args?: unknown;
  status: string;
  output?: string;
}

/** One chat log entry: who said it and what. A `tool` entry is the card
 *  (its `tool` field carries the card; `text` stays empty), keyed by the
 *  call id so the result envelope completes the card it belongs to. */
export interface AgentMessage {
  id: string;
  role: 'user' | 'assistant' | 'note' | 'tool';
  text: string;
  toolCallId?: string;
  tool?: ToolCard;
}

/** One message row as log entries: the row's parts in order, the same
 *  shapes the live fold produces, so a reloaded turn paints what the
 *  live turn rendered. Tool parts are completed cards (the row records
 *  only calls their result envelopes answered); note parts keep their
 *  line shape from before the cards landed. */
export function rowEntries(row: ChatMessage): AgentMessage[] {
  const entries: AgentMessage[] = [];
  for (const [at, part] of row.parts.entries()) {
    if (part.kind === 'text') {
      entries.push({
        id: `row-${row.id}-${at}`,
        role: row.role === 'user' ? 'user' : 'assistant',
        text: part.text ?? '',
      });
    } else if (part.kind === 'tool') {
      const tool: ToolCard = { name: part.name ?? '', status: part.status ?? '' };
      if (part.args !== undefined) tool.args = part.args;
      if (part.output !== undefined) tool.output = part.output;
      entries.push({ id: `row-${row.id}-${at}`, role: 'tool', text: '', tool });
    } else if (part.kind === 'note') {
      entries.push({ id: `row-${row.id}-${at}`, role: 'note', text: part.text ?? '' });
    }
  }
  return entries;
}
