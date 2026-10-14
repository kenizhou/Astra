// SPDX-License-Identifier: GPL-3.0-only
// The command ribbon: the 44px strip under the title bar, in the mail view
// only, floating with an 8px margin above and below so the tint band reads
// as its own layer, and wearing chrome-tint so it steps visibly off the
// title bar's chrome in both themes. Its mode follows the inline composer:
// read mode while the strip is collapsed, compose mode while a quick reply
// stands open in it. A pop-out composer window runs its own store in its
// own webview, so it never turns this ribbon. Under the compact breakpoint
// the buttons collapse to icon-only, their labels kept on the tooltips.

import { useIsCompact } from '@/hooks/useWindowSize';
import { useComposerStore } from '@/features/composer/composerStore';
import { useUiStore } from '@/stores/uiStore';

import { ComposeRibbon } from './ComposeRibbon';
import { ReadRibbon } from './ReadRibbon';

export function CommandRibbon() {
  const shellView = useUiStore((s) => s.shellView);
  const composing = useComposerStore((s) => s.open && s.composition !== null);
  const iconOnly = useIsCompact();

  if (shellView !== 'mail') {
    return null;
  }

  return (
    <nav
      aria-label="Command ribbon"
      className="my-2 flex h-[var(--ribbon-h)] shrink-0 items-stretch border-b border-border-subtle bg-[var(--chrome-tint)] px-1 py-1"
    >
      <div className="flex min-w-0 flex-nowrap items-stretch overflow-hidden">
        {composing ? <ComposeRibbon iconOnly={iconOnly} /> : <ReadRibbon iconOnly={iconOnly} />}
      </div>
    </nav>
  );
}
