# Design system reference

The one durable home for the rules the UI1 slice mints. `theme.css` holds
the tokens; this page holds the rules that govern them. When the two
disagree, the tokens are wrong until the rule is argued down in a review.

## The density axis

`data-density` on the document root, owned by `appearance.ts` beside
`data-skin`, `data-contrast`, and `data-font-size`. Two steps: comfortable
(28px rows, 28px hit floors, 12px pane padding) and compact (24px, 24px,
8px). Comfortable is the default; an absent attribute behaves as
comfortable; an unknown persisted value falls back to comfortable. The
General settings page exposes the axis beside Font size.

Rows are floors: content taller than the step grows the row, never
truncates it. Rows size with `min-height`; no fixed-height text rows
anywhere. The message list's own three-step density (compact, normal,
comfortable; 44/56/72px with its preview lines) stays a content choice
per surface; the axis governs the chrome.

## The texture axis

`data-texture` on the document root, owned by `appearance.ts` beside the
other appearance attributes. Three steps: warm (the default; the approved
A3 execution, warm graphite at hue 60 in dark, parchment in light), calm
(the hue-260 sibling: true-neutral graphite, cool porcelain), and classic
(the pre-texture cool slate, the base file's own values). The values live
in `theme.css` as attribute-scoped override blocks ahead of the
high-contrast section, so high contrast always flattens the texture. The
`--paper` token keeps the reading pane one quiet step lighter than the
canvas; the reading pane consumes it directly. The General settings page
exposes the axis beside Accent. The values and their evidence are recorded
in `docs/designs/evolved-iris-texture.md`.

## Motion

The duration scale is 100/120/140/160ms (`--dur-fast`, `--dur-row`,
`--dur-view`, `--dur-panel`). `--ease-spring` belongs to the command
palette alone; nothing else may ease with it. Everything drops to instant
under `prefers-reduced-motion`, the skeleton shimmer included.

## Typography

Tabular numerals (`font-feature-settings: 'tnum'`) on every count,
timestamp, and duration. The bundled face (UI1 D2) ships with the weight
stops 380/450/560 used as: light previews, medium senders, semibold
subjects.

## The state family

One `EmptyState` (glyph tile, message, optional primary action) replaces
every bare empty state; its per-surface copy lives in the content map
(`components/ui/stateContent.ts`), the single source of truth.
`SkeletonRows` renders three bars per pane, row-height matched to the
axis. Inline errors name the problem, state the cause when known, and end
with the recovery action; the settings save error is the worked example.
Success stays silent on settings saves.

## The AI grammar

Every AI chip carries its provenance ("from this thread", "from your
draft", "from today's inbox"), its state, and an always-explicit approve
action. The lifecycle has five states: pending (dashed), generating
(shimmer), approve (accent border), approved (plain confirmed text),
failed (amber border with a retry). Chips appear only where the behaviour
behind them is already live. This mirrors the sidecar's fail-closed
posture: no chip implies a capability the host has not granted.

## The account palette

Eight defaults (`--account-1` through `--account-8` in `theme.css`),
assigned in rotation over the account order, the same eight in both
themes. Collisions are acceptable at eight accounts or fewer. To extend
or remap: add or change a `--account-N` token in `theme.css`; the palette
has no settings UI.

## Appearance attribute naming

Attributes are concept-named and word-valued (`data-density`,
`data-contrast`). `data-font-size` is the grandfathered exception, not a
second precedent: new attributes do not copy its property-style shape.
