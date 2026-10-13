// SPDX-License-Identifier: GPL-3.0-only
// The Shortcuts page: a static inventory of the shortcuts the shell wires
// today, rendered from the SHORTCUTS table so a new shortcut lands here in
// the same change that wires it. Nothing on the page is editable.

/** One row: the keys, what they do, and where they answer. */
interface ShortcutEntry {
  keys: string;
  action: string;
  context: string;
}

/** Every shortcut wired in the shell today, grouped by where it answers. */
const SHORTCUTS: ReadonlyArray<ShortcutEntry> = [
  { keys: 'Ctrl+K', action: 'Open the search island', context: 'Everywhere' },
  { keys: 'Enter', action: 'Run the typed search', context: 'Search island' },
  {
    keys: 'Escape',
    action: 'Clear the typed query; with the field empty, fold the island away',
    context: 'Search island',
  },
  { keys: 'Escape', action: 'Close this settings screen', context: 'Settings' },
  { keys: 'Escape', action: 'Close the folder drawer', context: 'Folder drawer' },
  { keys: 'Escape', action: 'Close the open menu', context: 'Menu bar' },
  {
    keys: 'Arrow Up / Arrow Down',
    action: 'Move through the recipient suggestions',
    context: 'Composer',
  },
  { keys: 'Enter', action: 'Add the highlighted recipient', context: 'Composer' },
  { keys: 'Escape', action: 'Dismiss the recipient suggestions', context: 'Composer' },
  { keys: 'Tab', action: 'Accept the inline writing suggestion', context: 'Composer' },
  { keys: 'Enter or Space', action: 'Open the event editor', context: 'Calendar event' },
];

/** The key combo, drawn as a key cap. */
function Kbd({ children }: { children: React.ReactNode }) {
  return (
    <kbd className="inline-flex h-6 min-w-7 items-center justify-center rounded-[var(--radius-xs)] border border-border bg-background px-1.5 text-xs font-medium text-foreground shadow-[var(--shadow-sm)]">
      {children}
    </kbd>
  );
}

/** The Shortcuts page of the settings screen. */
export function ShortcutsDetail() {
  return (
    <section aria-label="Shortcuts" className="flex flex-col gap-4">
      <h2 className="text-sm font-semibold text-foreground">Shortcuts</h2>
      <table className="w-full border-collapse text-xs">
        <thead>
          <tr className="text-left text-muted-foreground">
            <th scope="col" className="pb-2 pr-4 font-medium">
              Shortcut
            </th>
            <th scope="col" className="pb-2 pr-4 font-medium">
              Action
            </th>
            <th scope="col" className="pb-2 font-medium">
              Where
            </th>
          </tr>
        </thead>
        <tbody>
          {SHORTCUTS.map((entry) => (
            <tr key={`${entry.context}:${entry.keys}:${entry.action}`} className="border-t border-border-subtle">
              <td className="py-2 pr-4 whitespace-nowrap">
                <Kbd>{entry.keys}</Kbd>
              </td>
              <td className="py-2 pr-4 text-foreground">{entry.action}</td>
              <td className="py-2 text-muted-foreground">{entry.context}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
