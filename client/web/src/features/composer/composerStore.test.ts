// SPDX-License-Identifier: GPL-3.0-only
// The composer store's quote default: a request's quoted original opens
// below the reply or above it per the settings block, the reply typing in
// the space the setting leaves, and the document the composer then holds is
// the per-message truth the setting only seeds.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const settingsState = vi.hoisted(() => ({
  settings: { quote_below: false },
}));

vi.mock('@/features/settings/settingsStore', () => ({
  useSettingsStore: { getState: () => settingsState },
}));

import { useComposerStore } from './composerStore';
import type { Block, ComposeRequestSnapshot } from '@/services/ipc';

/** One plain paragraph block with the given words. */
function paragraph(text: string): Block {
  return {
    Paragraph: {
      content: [
        {
          Text: {
            text,
            bold: false,
            italic: false,
            underline: false,
            font_size: null,
            color: null,
            highlight: null,
            link: null,
          },
        },
      ],
    },
  };
}

/** The quoted original a reply request carries. */
function quoteBlock(): Block {
  return {
    Quote: {
      style: 'Indented',
      attribution: { line: 'Ada wrote:', headers: [] },
      body_html: '<div>The numbers are in.</div>',
      body_plain: 'The numbers are in.',
    },
  };
}

/** A reply request whose document holds a drafted line above the quote. */
function request(): ComposeRequestSnapshot {
  return {
    kind: 'reply',
    from: 'work',
    to: 'ada@example.test',
    cc: '',
    subject: 'Re: Quarterly report',
    in_reply_to: 'orig-1@server.test',
    document: { blocks: [paragraph('Thanks for the numbers.'), quoteBlock()], attachments: [] },
  };
}

beforeEach(() => {
  settingsState.settings.quote_below = false;
  useComposerStore.getState().closePane();
});

describe('the composer quote default', () => {
  it('opens with the quote above the reply when the setting says so', () => {
    useComposerStore.getState().openRequest(request(), 'Work');

    const editor = useComposerStore.getState().editor;
    expect(editor.retained).toHaveLength(1);
    expect(editor.retained[0]?.index).toBe(0);
    expect('Quote' in (editor.retained[0]?.block ?? {})).toBe(true);
  });

  it('keeps the quote below the reply on the default setting', () => {
    settingsState.settings.quote_below = true;
    useComposerStore.getState().openRequest(request(), 'Work');

    const editor = useComposerStore.getState().editor;
    expect(editor.retained[0]?.index).toBe(1);
  });
});
