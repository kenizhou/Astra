// SPDX-License-Identifier: GPL-3.0-only

// Hugeicons wrappers under semantic names. Every glyph renders at 16px with
// 1.5 stroke by default and accepts size, strokeWidth, and className overrides.
import { HugeiconsIcon } from '@hugeicons/react';
import type { SVGProps } from 'react';
import {
  Add01Icon,
  ArchiveIcon as ArchiveIconData,
  BookOpen02Icon as ReadingIconData,
  Calendar03Icon,
  Cancel01Icon,
  CheckIcon as CheckIconData,
  ChevronLeftIcon as ChevronLeftIconData,
  ChevronRightIcon as ChevronRightIconData,
  Delete02Icon,
  EllipsisVerticalIcon as MoreIconData,
  Flag02Icon,
  Folder01Icon,
  Forward01Icon,
  InformationCircleIcon as AboutIconData,
  InboxIcon as InboxIconData,
  KeyboardIcon as ShortcutsIconData,
  LeftToRightListBulletIcon,
  LeftToRightListNumberIcon,
  Link01Icon,
  Mail01Icon,
  MailEdit01Icon,
  MailOpen01Icon,
  MailReply01Icon,
  MailReplyAll01Icon,
  MailSend01Icon,
  MoonIcon as MoonIconData,
  Note01Icon,
  PanelLeftIcon as PanelLeftIconData,
  PaperclipIcon as PaperclipIconData,
  RedoIcon as RedoIconData,
  RefreshIcon as RefreshIconData,
  Robot01Icon,
  Search01Icon,
  SentIcon as SentIconData,
  Settings02Icon,
  SlidersHorizontalIcon as AdvancedIconData,
  SpamIcon,
  SparklesIcon,
  Sun03Icon,
  TextBoldIcon,
  TextItalicIcon,
  TextUnderlineIcon,
  UndoIcon as UndoIconData,
  UserMultipleIcon,
} from '@hugeicons/core-free-icons';

export interface IconProps extends Omit<SVGProps<SVGSVGElement>, 'ref'> {
  size?: number | string;
  strokeWidth?: number;
}

function makeIcon(iconData: Parameters<typeof HugeiconsIcon>[0]['icon']) {
  return function Icon({ size = 16, strokeWidth = 1.5, ...rest }: IconProps) {
    return <HugeiconsIcon icon={iconData} size={size} strokeWidth={strokeWidth} {...rest} />;
  };
}

export const MailIcon = makeIcon(Mail01Icon);
export const CalendarIcon = makeIcon(Calendar03Icon);
export const PeopleIcon = makeIcon(UserMultipleIcon);
export const AgentIcon = makeIcon(Robot01Icon);
export const AiIcon = makeIcon(SparklesIcon);
export const OutboxIcon = makeIcon(MailSend01Icon);
export const PanelLeftIcon = makeIcon(PanelLeftIconData);
export const SearchIcon = makeIcon(Search01Icon);
export const PlusIcon = makeIcon(Add01Icon);
export const MoonIcon = makeIcon(MoonIconData);
export const SunIcon = makeIcon(Sun03Icon);
export const InboxIcon = makeIcon(InboxIconData);
export const SentIcon = makeIcon(SentIconData);
export const DraftsIcon = makeIcon(MailEdit01Icon);
export const TrashIcon = makeIcon(Delete02Icon);
export const ArchiveIcon = makeIcon(ArchiveIconData);
export const JunkIcon = makeIcon(SpamIcon);
export const FolderIcon = makeIcon(Folder01Icon);
export const FlagIcon = makeIcon(Flag02Icon);
export const PaperclipIcon = makeIcon(PaperclipIconData);
export const ReplyIcon = makeIcon(MailReply01Icon);
export const ReplyAllIcon = makeIcon(MailReplyAll01Icon);
export const ForwardIcon = makeIcon(Forward01Icon);
export const ComposeIcon = makeIcon(MailEdit01Icon);
export const MarkReadIcon = makeIcon(MailOpen01Icon);
export const SnippetsIcon = makeIcon(Note01Icon);
export const CloseIcon = makeIcon(Cancel01Icon);
export const ChevronLeftIcon = makeIcon(ChevronLeftIconData);
export const ChevronRightIcon = makeIcon(ChevronRightIconData);
export const SettingsIcon = makeIcon(Settings02Icon);
export const CheckIcon = makeIcon(CheckIconData);
export const ReadingIcon = makeIcon(ReadingIconData);
export const ShortcutsIcon = makeIcon(ShortcutsIconData);
export const AboutIcon = makeIcon(AboutIconData);
export const AdvancedIcon = makeIcon(AdvancedIconData);
export const BoldIcon = makeIcon(TextBoldIcon);
export const ItalicIcon = makeIcon(TextItalicIcon);
export const UnderlineIcon = makeIcon(TextUnderlineIcon);
export const BulletListIcon = makeIcon(LeftToRightListBulletIcon);
export const OrderedListIcon = makeIcon(LeftToRightListNumberIcon);
export const LinkIcon = makeIcon(Link01Icon);
export const UndoIcon = makeIcon(UndoIconData);
export const RedoIcon = makeIcon(RedoIconData);
export const RefreshIcon = makeIcon(RefreshIconData);
export const MoreIcon = makeIcon(MoreIconData);
