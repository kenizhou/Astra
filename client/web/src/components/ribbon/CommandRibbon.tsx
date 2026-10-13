// SPDX-License-Identifier: GPL-3.0-only
// The command ribbon: the 56px strip under the title bar, in the mail view
// only. Its mode follows the composer session: read mode while nothing is
// being composed, compose mode while a composition is open. Under the
// compact breakpoint the buttons collapse to icon-only, their labels kept
// on the tooltips.

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
      className="flex h-[var(--ribbon-h)] shrink-0 items-stretch border-b border-border-subtle bg-chrome px-1 py-1"
    >
      <div className="flex min-w-0 flex-nowrap items-stretch overflow-hidden">
        {composing ? <ComposeRibbon iconOnly={iconOnly} /> : <ReadRibbon iconOnly={iconOnly} />}
      </div>
    </nav>
  );
}
