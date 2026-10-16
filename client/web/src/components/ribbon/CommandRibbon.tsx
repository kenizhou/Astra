// SPDX-License-Identifier: GPL-3.0-only
// The command ribbon: the flush strip over the mail view's cards, the
// calendar toolbar's own design: 32px tall, surface-toned, its bottom rule
// the faint border, controls at text-xs. Its mode follows the inline
// composer: read mode while the strip is collapsed, compose mode while a
// quick reply stands open in it. A pop-out composer window runs its own
// store in its own webview, so it never turns this ribbon. The buttons
// never shrink: at the narrow window stage they collapse to icon-only,
// their labels kept on the tooltips, their glyphs at full size.

import { useWindowStage } from '@/hooks/useWindowSize';
import { useComposerStore } from '@/features/composer/composerStore';
import { useUiStore } from '@/stores/uiStore';

import { ComposeRibbon } from './ComposeRibbon';
import { ReadRibbon } from './ReadRibbon';

export function CommandRibbon() {
  const shellView = useUiStore((s) => s.shellView);
  const composing = useComposerStore((s) => s.open && s.composition !== null);
  const iconOnly = useWindowStage() === 'narrow';

  if (shellView !== 'mail') {
    return null;
  }

  return (
    <nav
      aria-label="Command ribbon"
      className="flex h-9 shrink-0 items-center border-b border-border-subtle bg-surface px-3"
    >
      <div className="flex min-w-0 flex-nowrap items-center gap-1 overflow-hidden">
        {composing ? <ComposeRibbon iconOnly={iconOnly} /> : <ReadRibbon iconOnly={iconOnly} />}
      </div>
    </nav>
  );
}
