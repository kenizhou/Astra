// SPDX-License-Identifier: GPL-3.0-only
/// <reference types="vite/client" />

import type { HTMLAttributes } from 'react';

// The reading pane's iframe carries a csp attribute so the framed document is
// served under a barrier the sandbox alone does not state. The attribute left
// the HTML spec as a draft, so the DOM type table has no entry; the
// augmentation declares it once for every iframe in the app.
declare module 'react' {
  interface IframeHTMLAttributes<T> extends HTMLAttributes<T> {
    /** The content security policy the framed document is served under. */
    csp?: string | undefined;
  }
}
