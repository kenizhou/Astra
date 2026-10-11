// SPDX-License-Identifier: GPL-3.0-only
// The AI documents panel's state: the account it serves, the tab it
// shows, the window the days selector names, and each kind's slot (the
// latest document, its read, and the generation in flight). Every ask
// crosses through the ai service's wrappers; a refusal lands as a typed
// error carrying which half failed and the host's own sentence. An open
// epoch guards every write: an ask that resolves after the panel opened
// another account writes nothing, so a document or refusal always
// belongs to the account on screen.

import { create } from 'zustand';

import {
  digestGenerate,
  latestDocument,
  reportGenerate,
  type DocumentKind,
} from '@/services/ai';

/** The window the selector offers, floor to ceiling. */
export const DAYS_MIN = 1;
export const DAYS_MAX = 31;
export const DAYS_DEFAULT = 7;

/** The render fields of one document: what the panel draws. */
export interface PanelDocument {
  period_start: number;
  period_end: number;
  content: string;
}

/** Which ask failed: the latest read or the generation. */
export type PanelErrorSource = 'read' | 'generate';

/** One refusal as the panel holds it: the half that failed and its word. */
export interface PanelError {
  source: PanelErrorSource;
  message: string;
}

/** One kind's slot: its document, its read, and its refusal. */
export interface KindSlot {
  document: PanelDocument | null;
  loading: boolean;
  error: PanelError | null;
}

/** The rejection's sentence, whatever shape the wire threw. */
function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

interface AiPanelState {
  /** The tab the panel shows; the digest first. */
  tab: DocumentKind;
  /** The window the days selector names; seven until changed. */
  days: number;
  /** The account the panel serves; null before the first open. */
  account: string | null;
  /**
   * The open epoch: every open() bumps it, and each ask captures the
   * epoch it left on. An ask that resolves after a later open (an
   * account switch mid-flight) writes nothing, so one account's answer
   * can never land in the account the panel now serves.
   */
  epoch: number;
  /** Each kind's slot. */
  docs: Record<DocumentKind, KindSlot>;
  /** The kind whose generation is in flight; null at rest. */
  generating: DocumentKind | null;
  /** Returns the panel to its closed state. */
  reset: () => void;
  /** Serves one account: records it and pulls both kinds' latest. */
  open: (account: string) => void;
  /** Switches the tab shown. */
  setTab: (tab: DocumentKind) => void;
  /** Records the window the selector named. */
  setDays: (days: number) => void;
  /** Pulls one kind's latest document. */
  pull: (kind: DocumentKind) => Promise<void>;
  /** Generates one kind over the selected window. */
  generate: (kind: DocumentKind) => Promise<void>;
}

const emptySlot: KindSlot = { document: null, loading: false, error: null };

function slotWith(slot: KindSlot, over: Partial<KindSlot>): KindSlot {
  return { ...slot, ...over };
}

export const useAiPanelStore = create<AiPanelState>((set, get) => ({
  tab: 'digest',
  days: DAYS_DEFAULT,
  account: null,
  epoch: 0,
  docs: { digest: { ...emptySlot }, report: { ...emptySlot } },
  generating: null,

  reset: () =>
    set({
      tab: 'digest',
      days: DAYS_DEFAULT,
      account: null,
      epoch: 0,
      docs: { digest: { ...emptySlot }, report: { ...emptySlot } },
      generating: null,
    }),

  open: (account) => {
    // The bump retires every ask still in flight for the account the
    // panel is leaving, and the generating reset hands the button to
    // the account arriving.
    set((state) => ({
      account,
      epoch: state.epoch + 1,
      generating: null,
      docs: { digest: { ...emptySlot }, report: { ...emptySlot } },
    }));
    void get().pull('digest');
    void get().pull('report');
  },

  setTab: (tab) => set({ tab }),

  setDays: (days) => set({ days }),

  pull: async (kind) => {
    const { account, epoch } = get();
    if (account === null) {
      return;
    }
    set((state) => ({
      docs: { ...state.docs, [kind]: slotWith(state.docs[kind], { loading: true, error: null }) },
    }));
    try {
      const latest = await latestDocument(account, kind);
      if (get().epoch !== epoch) {
        return;
      }
      set((state) => ({
        docs: {
          ...state.docs,
          [kind]: slotWith(state.docs[kind], {
            loading: false,
            document:
              latest === null
                ? null
                : {
                    period_start: latest.period_start,
                    period_end: latest.period_end,
                    content: latest.content,
                  },
          }),
        },
      }));
    } catch (err) {
      if (get().epoch !== epoch) {
        return;
      }
      set((state) => ({
        docs: {
          ...state.docs,
          [kind]: slotWith(state.docs[kind], {
            loading: false,
            error: { source: 'read', message: errorText(err) },
          }),
        },
      }));
    }
  },

  generate: async (kind) => {
    const { account, days, generating, epoch } = get();
    if (account === null || generating !== null) {
      return;
    }
    set({ generating: kind });
    try {
      const generated =
        kind === 'digest'
          ? await digestGenerate(account, days)
          : await reportGenerate(account, days);
      if (get().epoch !== epoch) {
        return;
      }
      set((state) => ({
        generating: null,
        docs: {
          ...state.docs,
          [kind]: slotWith(state.docs[kind], {
            error: null,
            document: {
              period_start: generated.period_start,
              period_end: generated.period_end,
              content: generated.content,
            },
          }),
        },
      }));
    } catch (err) {
      if (get().epoch !== epoch) {
        return;
      }
      set((state) => ({
        generating: null,
        docs: {
          ...state.docs,
          [kind]: slotWith(state.docs[kind], {
            error: { source: 'generate', message: errorText(err) },
          }),
        },
      }));
    }
  },
}));
