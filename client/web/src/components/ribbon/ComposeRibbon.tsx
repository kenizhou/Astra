// SPDX-License-Identifier: GPL-3.0-only
// The ribbon's compose mode, standing while a composition is open: Send
// rides the store's one submit path (the pane's own Send button too), the
// save hint repeats the status surface's word for this composition, the
// snippet picker inserts through the live editor handle the pane
// registered, and Discard runs the pane's discard flow: the stored copy
// goes and the pane closes locally.

import { SentIcon, TrashIcon } from '@/components/icons';
import { insertAtSelection } from '@/features/composer/Editor';
import { SaveBadge } from '@/features/composer/Composer';
import { SnippetPicker } from '@/features/composer/SnippetPicker';
import { composerEditor, useComposerStore } from '@/features/composer/composerStore';
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
      <RibbonGroup>
        <SnippetPicker
          onInsert={(body) => {
            const editor = composerEditor();
            if (editor !== null) {
              insertAtSelection(editor, body);
            }
          }}
        />
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
