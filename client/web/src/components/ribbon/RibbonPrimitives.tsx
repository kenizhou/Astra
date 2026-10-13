// SPDX-License-Identifier: GPL-3.0-only
// The ribbon's building blocks: a group of buttons separated from the next
// group by a hairline, and the button itself, labelled by text beside its
// icon or, under the compact breakpoint, by its tooltip alone. A group
// never wraps: when space runs out the shell clips whole trailing groups.

import type { ComponentType } from 'react';

import type { IconProps } from '@/components/icons';

/** One cluster of ribbon buttons, hairline-separated from the next. */
export function RibbonGroup({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex shrink-0 flex-nowrap items-center gap-0.5 border-r border-border-subtle px-1.5 last:border-r-0">
      {children}
    </div>
  );
}

interface RibbonButtonProps {
  /** The button's name and tooltip; the tooltip keeps it when icon-only. */
  label: string;
  icon: ComponentType<IconProps>;
  /** The filled variant the group's one primary verb wears. */
  primary?: boolean;
  disabled?: boolean;
  /** A tooltip richer than the label, when the gating needs explaining. */
  title?: string;
  /** Collapse to the icon alone (the compact breakpoint); the label stays
   *  on the tooltip and the accessible name. */
  iconOnly?: boolean;
  onClick: () => void;
}

export function RibbonButton({
  label,
  icon: Icon,
  primary = false,
  disabled = false,
  title,
  iconOnly = false,
  onClick,
}: RibbonButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      title={title ?? label}
      disabled={disabled}
      onClick={onClick}
      className={`my-auto flex h-9 items-center gap-1.5 rounded-md text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-50 ${
        primary
          ? 'bg-primary px-2.5 font-medium text-primary-foreground hover:bg-primary-hover disabled:hover:bg-primary'
          : 'px-2 text-muted-text hover:bg-hover hover:text-foreground disabled:hover:bg-transparent disabled:hover:text-muted-text'
      } ${iconOnly ? 'w-9 justify-center px-0' : ''}`}
    >
      <Icon size={16} />
      <span className={`whitespace-nowrap ${iconOnly ? 'sr-only' : ''}`}>{label}</span>
    </button>
  );
}
