// SPDX-License-Identifier: GPL-3.0-only
// The ribbon's quick steps group: one button per saved step, labelled by
// its name and wearing its action's glyph, disabled without a selection and
// running the step over the selection in a click; the Manage button opens
// the store-gated manager. The labels collapse to the tooltips under the
// compact breakpoint, the other groups' own behaviour.

import type { ComponentType } from 'react';

import {
  ArchiveIcon,
  FlagIcon,
  FolderIcon,
  MarkReadIcon,
  SettingsIcon,
  TrashIcon,
  type IconProps,
} from '@/components/icons';
import { useListStore } from '@/stores/listStore';

import { QuickStepsManager } from './QuickStepsManager';
import { describeQuickStep, useQuickStepsStore, type QuickStepAction } from './quickStepsStore';
import { runQuickStepOverSelection } from './runQuickStep';
import { RibbonButton, RibbonGroup } from '@/components/ribbon/RibbonPrimitives';

/** The glyph each action wears on its step button. */
const ACTION_ICONS: Record<QuickStepAction, ComponentType<IconProps>> = {
  archive: ArchiveIcon,
  trash: TrashIcon,
  'mark-read': MarkReadIcon,
  'mark-unread': MarkReadIcon,
  flag: FlagIcon,
  unflag: FlagIcon,
  move: FolderIcon,
};

export function QuickStepsRibbonGroup({ iconOnly }: { iconOnly: boolean }) {
  const steps = useQuickStepsStore((s) => s.steps);
  const selectedKey = useListStore((s) => s.selectedKey);
  const selectedKeys = useListStore((s) => s.selectedKeys);
  const hasSelection = selectedKeys.length > 0 || selectedKey !== null;

  return (
    <RibbonGroup>
      {steps.map((step) => (
        <RibbonButton
          key={step.id}
          label={step.name}
          icon={ACTION_ICONS[step.action]}
          disabled={!hasSelection}
          title={`${step.name}: ${describeQuickStep(step)}`}
          iconOnly={iconOnly}
          onClick={() => runQuickStepOverSelection(step)}
        />
      ))}
      <RibbonButton
        label="Manage quick steps"
        icon={SettingsIcon}
        iconOnly={iconOnly}
        onClick={() => useQuickStepsStore.getState().openManager()}
      />
      <QuickStepsManager />
    </RibbonGroup>
  );
}