// SPDX-License-Identifier: GPL-3.0-only
// The ribbon's compose mode, standing while the quick-reply strip's inline
// composition is open: Send rides the store's one submit path (the pane's
// own Send button too), the save hint repeats the status surface's word for
// this composition, the pop-out transfer moves the draft to its own window
// (the pane's Pop out button rides the same helper), and Discard runs the
// pane's discard flow: the stored copy goes and the strip collapses
// locally. A pop-out composer window has its own store in its own webview;
// this ribbon never sees it.

import { isTauri } from '@/events/surfaceEvents';
import { PopOutIcon, SentIcon, TrashIcon } from '@/components/icons';
import { SaveBadge } from '@/features/composer/Composer';
import { popOutComposer } from '@/features/composer/composeWindow';
import { useComposerStore } from '@/features/composer/composerStore';
import { PluginComposerActions } from '@/features/plugins/PluginComposerActions';
import { dispatch, intents } from '@/services/ipc';

import { RibbonButton, RibbonGroup } from './RibbonPrimitives';

export function ComposeRibbon({ iconOnly }: { iconOnly: boolean }) {
  const composition = useComposerStore((s) => s.composition);
  const to = useComposerStore((s) => s.to);
  const sending = useComposerStore((s) => s.sending);
  const sendGated = to.trim() === '';

  const discard = (): void => {
    if (composition === null) {
      return;
    }
    void dispatch(intents.discardDraft(composition));
    useComposerStore.getState().closePane();
  };

  return (
    <>
      <RibbonGroup>
        <RibbonButton
          label="Send"
          icon={SentIcon}
          primary
          disabled={sendGated || sending}
          title={sendGated ? 'Add a recipient first' : 'Send'}
          iconOnly={iconOnly}
          onClick={() => useComposerStore.getState().submit()}
        />
        {composition !== null && <SaveBadge composition={composition} />}
      </RibbonGroup>
      <PluginComposerActions />
      <RibbonGroup>
        {isTauri && (
          <RibbonButton
            label="Pop out"
            icon={PopOutIcon}
            iconOnly={iconOnly}
            onClick={() => void popOutComposer()}
          />
        )}
        <RibbonButton
          label="Discard"
          icon={TrashIcon}
          iconOnly={iconOnly}
          onClick={discard}
        />
      </RibbonGroup>
    </>
  );
}
