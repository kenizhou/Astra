// SPDX-License-Identifier: GPL-3.0-only
// The attachment browser store: the open gate, the sender prefilter an opener
// hands in, the close that drops it, and the Clear that lifts it while the
// overlay stands.

import { beforeEach, describe, expect, it } from 'vitest';

import {
  closeAttachmentBrowser,
  openAttachmentBrowser,
  useAttachmentBrowserStore,
} from './attachmentBrowserStore';

beforeEach(() => {
  useAttachmentBrowserStore.setState({ isOpen: false, sender: null });
});

describe('the attachment browser store', () => {
  it('opens unfiltered by default', () => {
    openAttachmentBrowser();
    const state = useAttachmentBrowserStore.getState();
    expect(state.isOpen).toBe(true);
    expect(state.sender).toBeNull();
  });

  it('opens narrowed to the sender an opener hands in', () => {
    openAttachmentBrowser('ada@example.test');
    const state = useAttachmentBrowserStore.getState();
    expect(state.isOpen).toBe(true);
    expect(state.sender).toBe('ada@example.test');
  });

  it('closes and drops the prefilter', () => {
    openAttachmentBrowser('ada@example.test');
    closeAttachmentBrowser();
    const state = useAttachmentBrowserStore.getState();
    expect(state.isOpen).toBe(false);
    expect(state.sender).toBeNull();
  });

  it('clears the sender while the overlay stands', () => {
    openAttachmentBrowser('ada@example.test');
    useAttachmentBrowserStore.getState().clearSender();
    const state = useAttachmentBrowserStore.getState();
    expect(state.isOpen).toBe(true);
    expect(state.sender).toBeNull();
  });
});
