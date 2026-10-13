# UI1 Evolved Iris Design

Date: 2026-10-13
Status: proposed
Parent evidence: the design audits of 2026-10-13
(`~/.gstack/projects/kenizhou-Astra/designs/design-audit-20261013/`), the
twelve demo pages in `client/web/.gstack/designs/redesign/`, and the
recorded pick (decision 01bb418e: A, Evolved Iris, on all four surfaces).
Port contracts: none directly; this slice is the visual system the SP2m
onward surfaces draw from, ahead of their landing.

## 1. Purpose

The design system the next slices build on, landed as one coherent change
instead of accreting per surface. Two audits graded the current shell A-
and deferred five findings because they were system decisions, not
one-line fixes: the density scale (FINDING-003/004), the empty-state
family (FINDING-005), and the patterns SP2p onward will need anyway
(account chips, AI approve grammar). The user compared three directions on
twelve demo pages and picked Evolved Iris: the current direction,
finished. This slice ships it.

## 2. Decisions

- **D1 (the density axis)**: the `data-font-size` steps generalise into a
  `data-density` comfortable/compact axis on the document root, owned by
  `appearance.ts` beside the existing font-size steps. One scale drives
  row heights (comfortable 28px, compact 24px), hit floors (28/24px, and
  never below 24px anywhere), pane padding, and the message row's
  three-line/one-line switch. The sub-24px targets the audits deferred
  (collapse chevrons, view tabs, rail icons, mini-month cells) move to the
  axis in the same change; the answer to the 14px/16px body question
  becomes a user-facing setting, not a fixed value. The General settings
  page gains the Density segmented control beside Font size.
- **D2 (the typographic voice)**: one bundled variable font ships as an
  app resource ahead of the system stack in `--font-ui`, keeping the
  offline guarantee (the resource bundles with the app; the system stack
  remains the fallback). Candidates are OFL faces; the face itself is
  chosen in the implementation slice with a side-by-side against the demo
  pages. Tabular numerals (`font-feature-settings: 'tnum'`) apply to
  every count, timestamp, and duration.
- **D3 (one state family)**: a single `EmptyState` component
  (glyph tile + message + optional primary action, the demo's `.state`
  pattern) replaces every bare empty state: the list pane's "Select a
  folder", the calendar and people empties, and every surface's loading
  skeleton and inline error adopt the same family. The settings save
  error (specific, at the source, amber) is the error pattern, generalised.
- **D4 (the multi-account model)**: the folder pane gains per-account
  colour dots (a fixed eight-colour account palette beside the accent
  skins) and the Unified inbox row with its unread badge, ahead of SP2p's
  All Accounts group; the visual language lands here, the behaviour there.
- **D5 (the AI grammar)**: every AI chip carries its provenance
  ("from this thread"), its state (pending dashed, approve accent border),
  and an always-explicit approve action, matching the sidecar's
  fail-closed posture. One `AiChip` component serves the reading pane,
  the composer toolbar, and the digest.
- **D6 (the motion tokens)**: a duration scale (100/120/140/160ms) and the
  rule that `--ease-spring` belongs to the command palette alone; row
  selection eases out at 120ms; everything drops to instant under
  `prefers-reduced-motion`. No other animation is added in this slice.
- **D7 (onboarding as the first minute)**: the account setup's detection
  step takes the demo's shape: the trust decision is the step's headline
  (the unencrypted hop named, the encrypted legs named) and the approve
  action is the primary button. Copy follows `a-onboarding.html`; the
  trust rules themselves are unchanged.

## 3. Testing

- vitest per surface: the density axis switches every row height and hit
  floor with one attribute; `EmptyState` renders its three parts; `AiChip`
  renders provenance and both states; the settings page gains the Density
  control and persists it through `appearance.ts`'s localStorage path.
- Rust untouched: no intent, snapshot, or wire shape changes; the floor
  (`cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, the gate) must stay green without edits.
- Visual verification: `/design-review` against the fixture harness per
  surface, in both densities and both themes; the audit's deferred
  findings close or stand recorded as intentionally answered.
- The light theme gets its first full audit pass in this slice (both
  audits so far ran dark-only; recorded at baseline).

## 4. Known gaps (pre-recorded)

- The fixture harness serves at-rest snapshots only, so selected, reading,
  and modal states verify in the built app, not the harness; harness
  event injection is its own follow-up task.
- The bundled font face is chosen in the implementation slice; CJK
  fallback stays the system stack (`PingFang SC`, `Microsoft YaHei`,
  `Noto Sans SC`) regardless of the face chosen.
- B (Editorial) and C (Command) elements stay unshipped by decision; the
  serif reading column and the command palette are separate future picks,
  not parts of this slice.
- The capabilities matrix does not move: this slice changes no contract,
  only the visual system; SP2p's All Accounts group row stays ⬜ until
  its behaviour lands.
- High-contrast mode's flattened token blocks are unaffected by the
  density axis (it overrides series and shadow tokens only) and needs no
  density variant of its own; recorded, not silently skipped.
