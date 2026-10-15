<!-- /autoplan restore point: "D:\\Projects\\mailclient\\Astra\\.gstack\\tmp\\autoplan\\main-autoplan-restore-20261013-165242.md" -->
## Implementation plan
# UI1 Evolved Iris Design

Date: 2026-10-13
Status: approved
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
finished. This slice ships it. Two costs are paid on the record. The
rejected alternative is per-surface accretion with a later consolidation:
cheaper this month, but it re-litigates density and states on every
surface and lands SP2p's multi-account model with no visual language
ready. The pace cost is one slice of calendar time with no new
user-facing capability: this slice is enabling infrastructure for SP2m
onward, not a differentiator, and the differentiated behaviour (AI
approve, multi-account) stays on the critical path.

## 2. Decisions

- **D1 (the density axis)**: the `data-font-size` steps generalise into a
  `data-density` comfortable/compact axis on the document root, owned by
  `appearance.ts` beside the existing font-size steps. Comfortable is the
  default: an absent attribute behaves as comfortable, and
  `appearance.ts` writes the attribute before the shell's first
  interactive paint, so no density flash on first paint joins the visual
  verification list beside the font check. One scale drives
  row heights (comfortable 28px, compact 24px), hit floors (28/24px, and
  never below 24px anywhere), pane padding (comfortable 12px, compact
  8px), and the message row's
  three-line/one-line switch. The sub-24px targets the audits deferred
  (collapse chevrons, view tabs, rail icons, mini-month cells) move to the
  axis in the same change; the answer to the 14px/16px body question
  becomes a user-facing setting, not a fixed value. Row heights are
  floors: a larger font-size step grows the row rather than truncating
  its content. The virtualized message list's estimate derives from the
  active density (the fixed `estimateSize` follows the axis, or rows
  re-measure with stale estimates and the scrollbar drifts). The General
  settings page gains the Density segmented control beside Font size.
- **D2 (the typographic voice)**: one bundled variable font ships as an
  app resource ahead of the system stack in `--font-ui`. The alternatives,
  priced: (a) system stack only, zero cost and zero voice; (b) a bundled
  Latin face with the system CJK pairing, voice with a bi-font cost on
  mixed-script lines; (c) a bundled face with full CJK coverage, the most
  voice and the most weight. The slice takes (b), and the reason is
  aesthetic differentiation, recorded as such. The shortlist: Inter,
  Source Sans 3, and IBM Plex Sans, all OFL with true tabular numerals
  and variable weights. The selection criteria: x-height against the
  24px compact floor, tabular numeral quality at 12 to 13px, Windows
  hinting at 100 and 125 per cent scaling, and a subset bundle budget
  under 400KB. The pick is recorded as a decision entry (like 01bb418e)
  before merge, with the demo pages as the comparison surface.
  Mixed-script lines are bi-font by design (the bundled Latin face, then
  the system CJK pairing), and the verification includes one Chinese
  mail thread in both densities and both themes, through a CJK fixture
  message the harness gains with CAND-1. The font loads with
  `font-display: swap` and `size-adjust`/`ascent-override` metric
  matching to the fallback stack, and no layout shift on cold load is
  on the visual verification list; the overrides are webview-dependent,
  so the oldest supported webview verifies them explicitly, with the
  fallback recorded (accept the swap shift there, or bump the floor).
  The vendored face pins its exact upstream release and checksum, with
  the OFL licence text beside the resource, both noted in the
  design-system reference. The subset budget is enforced by a size
  check in the xtask gate, or the budget is a wish. Tabular numerals
  (`font-feature-settings: 'tnum'`) apply to
  every count, timestamp, and duration.
- **D3 (one state family)**: a single `EmptyState` component
  (glyph tile + message + optional primary action, the demo's `.state`
  pattern) replaces every bare empty state: the list pane's "Select a
  folder", the calendar and people empties, and every surface's loading
  skeleton and inline error adopt the same family. The settings save
  error (specific, at the source, amber) is the error pattern, generalised,
  and its counterpart is recorded: success stays silent on settings
  saves. Loading skeletons are row-height-matched bars that follow the
  density attribute (28/24px), three bars per pane, with the shimmer the
  demos show. Each surface's EmptyState content (glyph, message, action)
  lives in a content map colocated with the components, the single
  source of truth; the demo markup is a point-in-time snapshot of that
  map, and implementers do not re-derive it. The
  component contracts are pinned here: `EmptyState { glyph, message,
  action? }` and `AiChip { provenance, state, onApprove, onRetry }`.
  Inline error copy names the problem, states the cause when known, and
  ends with the recovery action; the settings save error is the worked
  example. The reduced-motion contract covers everything, the skeleton
  shimmer included, and a named check proves it.
- **D4 (the multi-account model)**: the folder pane gains per-account
  colour dots and the Unified inbox row with its unread badge, ahead of
  SP2p's All Accounts group. The account palette is eight defaults
  assigned in rotation over an extensible token list (no settings UI in
  this slice); the dot is 8px round, set before the account name, the
  same eight in both themes, with per-theme palette validation inside
  the timeboxed light audit; rotation collisions are acceptable at eight
  accounts or fewer, and the policy beyond (how a developer adds or
  remaps a palette token in `theme.css`) lives in the design-system
  reference D6 names. The Unified row sits at the
  top of the folder pane above the first account group and is inert in
  this slice: inert styling with `aria-disabled` and normal pointer
  events, so the "Unified inbox arrives with SP2p" tooltip fires (a
  genuinely disabled control would swallow them).
  Its badge is live, summing the `unread` counts under
  `account_folders[].folders[]` in the mailbox-list snapshot: every
  folder except the junk and trash roles, per-mailbox counts with no
  dedup needed (partial data sums the available accounts with no
  warning chrome), pinned by fixture cases for the role exclusion and
  the partial sum, and a vitest asserting the sum against the
  multi-account fixture; no new data flow ships. The row hides when
  `account_folders.length < 2` (one account makes it a duplicate),
  pinned by a fixture case. The badge hierarchy
  reads aggregate, account, folder: account headers and folders keep
  their existing unread counts. The row stays focusable with the
  tooltip on focus through `aria-describedby`, tested, so the keyboard
  path is not a dead tab stop.
- **D5 (the AI grammar)**: every AI chip carries its provenance
  ("from this thread" on the reading pane, "from your draft" in the
  composer, "from today's inbox" in the digest), its state, and an
  always-explicit approve action, matching the sidecar's
  fail-closed posture. One `AiChip` component serves the reading pane,
  the composer toolbar, and the digest. Chips ship only where their
  behaviour is already live (the reading pane's summarise and draft
  chips, the composer's autocomplete, the digest); an approve action
  appears only where its behaviour exists. The lifecycle has five
  states: pending (dashed), generating (shimmer within the dashed
  frame), approve (accent border), approved (resolves to plain
  confirmed text), and failed (amber border with a retry); every state
  verifies in the harness through CAND-1's event injection.
- **D6 (the motion tokens)**: a duration scale (100/120/140/160ms) and the
  rule that `--ease-spring` belongs to the command palette alone; the token
  already exists in `theme.css`, and this slice adds no new consumer of it;
  row selection eases out at 120ms; everything drops to instant under
  `prefers-reduced-motion`. No other animation is added in this slice.
  The tokens and rules this slice mints (the density scale, the duration
  scale, the ease-spring reservation, the tabular-numerals rule, the
  silent-success rule, the palette collision policy) are enumerated in a
  short design-system reference (`docs/design-system.md` with its Chinese
  review copy), and the ease-spring reservation gains a comment at the
  token definition. Rows size with `min-height`; no fixed-height text
  rows anywhere, a rule the reference states and the per-surface audit
  checks line by line. Appearance attributes are concept-named and
  word-valued (`data-density`); `data-font-size` is the grandfathered
  exception, not a second precedent.
- **D7 (onboarding as the first minute)**: the account setup's detection
  step takes the demo's shape: the trust decision is the step's headline
  (the unencrypted hop named, the encrypted legs named) and the approve
  action is the primary button. Copy follows `a-onboarding.html`; the
  trust rules themselves are unchanged. The reframe is a recorded
  hypothesis (that trust-first raises completion), falsified cheaply:
  the next five first-run dogfood completions carry notes on the
  detection step, recorded in `docs/dogfood/` (a directory the doc_sync
  gate does not mirror), and abandonment or hesitation there counts
  against the hypothesis.

## 3. Testing

The landing order is recorded because verification depends on it:
CAND-1 first (the harness and the CJK fixture), then the D1/D6 token
work, the D3 components, D4, D5, D7, and the light audit last.

CAND-1's injection contract: the harness exposes an explicit injection
API (a fixture event push with the surface name and payload), pushes
are deep clones (never references into the recorded JSON), unlisten
and cleanup semantics match the Tauri bridge's, and injected events
apply in dispatch order with no interleaving against real events
(none exist in fixture mode). The alias applies only under
`--mode fixtures`; a build-level assertion pins that the harness never
lands in the production bundle. `harness.test.ts` pins all four.

- vitest per surface: the density axis switches every row height and hit
  floor with one attribute; `EmptyState` renders its three parts; `AiChip`
  renders provenance and both states; the settings page gains the Density
  control and persists it through `appearance.ts`'s localStorage path.
  `data-density` sits on the document root before the first render
  (a cheap ordering assertion).
- Rust untouched: no intent, snapshot, or wire shape changes; the floor
  (`cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, the gate) must stay green without edits. The
  gate also gains the font subset size check and the assertion that the
  fixture harness never lands in the production bundle.
- Visual verification: `/design-review` against the fixture harness per
  surface, in both densities and both themes; the audit's deferred
  findings close or stand recorded as intentionally answered. One Chinese
  mail thread verifies the bi-font mix through the CJK fixture, in both
  densities and both themes. The calendar's mini-month and the folder
  pane render in both densities as named checks (the 24px floor grows
  the mini-month's cells and the tree's indentation), and the risky
  cell runs too: font-size large in both densities on both panes.
  No layout shift on
  cold load with the font swap. With the large-list fixture, toggling
  Density mid-session and scrolling shows no overlap and no blank gaps.
- The light theme gets its first full audit pass in this slice (both
  audits so far ran dark-only; recorded at baseline), timeboxed: the
  slice fixes the density and token defects it touches, capped at one
  working day, and records the rest as findings for the next slice.

## 4. Known gaps (pre-recorded)

- The fixture harness serves at-rest snapshots only at this slice's
  start; CAND-1 resolves that in this slice (event injection with test
  coverage), so selected, reading, and modal states verify in the
  harness.
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

<!-- autoplan-accepted:ceo -->
- D1 density axis lands as one attribute-driven scale (comfortable 28px and compact 24px rows, 28 and 24px hit floors, pane padding, the message row's line switch), owned by appearance.ts beside data-font-size; the sub-24px targets (collapse chevrons, view tabs, rail icons, mini-month cells) move in the same change; row heights are floors: a larger font-size step grows the row rather than truncating its content; the General settings page gains the Density segmented control; vitest covers the axis switch and the control's persistence.
- D2 one bundled OFL variable font ships as an app resource ahead of the system stack in --font-ui; the alternatives are priced on the record (system stack only, bundled Latin plus system CJK, full CJK coverage) and option (b) is taken for aesthetic differentiation, recorded as such; the shortlist is Inter, Source Sans 3, and IBM Plex Sans; the selection criteria are x-height against the 24px compact floor, tabular numeral quality at 12 to 13px, Windows hinting at 100 and 125 per cent scaling, and a subset budget under 400KB; the pick is recorded as a decision entry before merge; mixed-script lines are bi-font by design and verified on one Chinese mail thread (the CJK fixture that CAND-1 adds) in both densities and both themes; tabular numerals on every count, timestamp, and duration.
- D3 one EmptyState component (glyph tile plus message plus optional primary action) replaces every bare empty state on the list pane, calendar, and people; loading skeletons and inline errors adopt the same family; the settings save error pattern generalises.
- D4 the folder pane gains per-account colour dots (eight defaults in rotation over an extensible token list, no settings UI in this slice) and the Unified inbox row with its unread badge; the Unified row is a visual placeholder that does not filter the list in this slice; the badge is live, summing the unread counts under account_folders[].folders[] in the mailbox-list snapshot, with a vitest asserting the sum against the multi-account fixture; no new data flow ships.
- D5 one AiChip component with provenance text, pending and approve states, and an always-explicit approve action; it serves the reading pane, the composer toolbar, and the digest; chips ship only where their behaviour is already live, and an approve action appears only where its behaviour exists.
- D6 motion tokens: a 100/120/140/160ms duration scale; --ease-spring belongs to the command palette alone; the token already exists in theme.css, and this slice adds no new consumer of it; row selection eases out at 120ms; prefers-reduced-motion drops everything.
- D7 the detection step takes the demo's shape: the trust decision is the headline, approve is the primary button; copy follows a-onboarding.html; the trust rules themselves are unchanged; the reframe is a recorded hypothesis (trust-first raises completion), falsified cheaply by first-run dogfood notes.
- Accepted expansion CAND-1 (taste, gate item): the fixture harness gains event injection (fixture surface_changed pushes and dispatch-driven snapshot swaps) with harness.test.ts coverage, so this slice's selected, reading, and modal states verify in the harness instead of only in the built app; docs/quality-workflow.md and its Chinese review copy update in the same change, and every spec amendment this review lands is mirrored into this spec's own Chinese review copy in the same commit.
- Verification: vitest green (existing 920 plus new), the Rust floor untouched and green, the gate green, /design-review per surface in both densities and both themes, one Chinese mail thread through the CJK fixture in both densities and both themes, and the light theme's first full audit, timeboxed: the slice fixes the defects it touches and records the rest as findings for the next slice.
- Known gaps stand: the font face is picked in implementation, B and C elements stay unshipped, the capabilities matrix does not move, high-contrast needs no density variant.
<!-- /autoplan-accepted:ceo -->

<!-- autoplan-accepted:design -->
- D1 gains the pane padding numbers: comfortable 12px, compact 8px.
- D2 gains the font-loading policy: font-display swap with size-adjust/ascent-override metric matching to the fallback stack; no layout shift on cold load is on the visual verification list.
- D3 gains the skeleton rule (row-height-matched bars following the density attribute, three per pane, shimmer), the success rule (settings saves stay silent on success), and canonical per-surface EmptyState content taken from the demo markup.
- D4 gains the Unified row's full contract: top of the folder pane above the first account group; inert with disabled styling, no hover affordance, and a "Unified inbox arrives with SP2p" tooltip; badge live and summing the snapshot's unread counts (partial data sums available accounts, no warning chrome, fixture case pinned); badge hierarchy aggregate, account, folder with existing counts kept. The dot: 8px round before the account name, the same eight colours in both themes, per-theme palette validation inside the light audit, collisions acceptable at eight accounts or fewer and documented beyond.
- D5 gains the five-state lifecycle (pending dashed, generating shimmer, approve accent border, approved resolves to plain confirmed text, failed amber with retry) verified through CAND-1 injection, and per-surface provenance copy ("from this thread" reading pane, "from your draft" composer, "from today's inbox" digest).
- D7 gains the falsification bar: the next five first-run dogfood completions carry notes on the detection step; abandonment or hesitation there counts against the hypothesis.
- Testing gains named checks: the calendar mini-month and folder pane in both densities (the 24px floor grows the mini-month cells and tree indentation), and no layout shift on cold load.
- The Unified row's inert-with-tooltip treatment is the provisional answer to a taste decision (alternatives: defer the row to SP2p, or ship it looking functional); it waits at the final gate.
<!-- /autoplan-accepted:design -->

<!-- autoplan-accepted:dx -->
- D1 gains the default: comfortable; an absent attribute behaves as comfortable; appearance.ts writes the attribute before the shell's first interactive paint; no density flash on first paint joins the visual verification list.
- D3 gains the pinned contracts EmptyState { glyph, message, action? } and AiChip { provenance, state, onApprove, onRetry }; the per-surface EmptyState content lives in a content map colocated with the components (the demos reference it); the inline error-copy standard (problem, cause when known, recovery action; the settings save error as the worked example).
- D4 gains: the inert row uses aria-disabled with normal pointer events so the tooltip fires; the palette collision and extension policy lives in the design-system reference D6 names.
- D6 gains: a short design-system reference (docs/design-system.md with its Chinese review copy) enumerating the slice's tokens and rules, the ease-spring reservation gains a comment at the token definition, and appearance attributes are concept-named and word-valued with data-font-size the grandfathered exception (provisional, a taste decision: the alternative is renaming data-font-size in this slice).
- The DX voice's eight findings are all accepted as spec amendments; the developer persona is the SP2m-onward implementer (and the contributing dogfooder); the magical moment is npm run dev:fixtures serving the real UI with real data in a plain browser, completed by CAND-1's event injection.
<!-- /autoplan-accepted:dx -->

<!-- autoplan-accepted:eng -->
- Sequencing recorded: CAND-1 first, then D1/D6 tokens, D3 components, D4, D5, D7, light audit last (capped at one working day).
- D1: the virtualized list's estimate derives from the active density; the large-list mid-session density toggle check (no overlap, no blank gaps); the risky cell (font-size large in both densities) runs on the mini-month and folder pane.
- D2: the subset budget is enforced by a size check in the xtask gate; the vendored face pins its upstream release and checksum with the OFL licence text beside the resource; the oldest supported webview verifies the metric overrides explicitly with the fallback recorded.
- D3: the content map is the single source of truth and the demos are a point-in-time snapshot of it; the reduced-motion contract covers the shimmer, with a named check.
- D4: the badge sum excludes junk and trash roles, per-mailbox counts with no dedup (fixture cases for the exclusion and the partial sum); the Unified row hides when account_folders.length < 2; the row stays focusable with the tooltip on focus through aria-describedby, tested.
- D6: rows size with min-height and no fixed-height text rows, stated in the design-system reference and checked per surface in the audit.
- CAND-1 gains its injection contract: explicit injection API, deep-clone pushes, unlisten and cleanup matching the Tauri bridge, dispatch-order application, the alias only under --mode fixtures, and a build-level assertion that the harness never lands in the production bundle; harness.test.ts pins all four.
- The first-paint ordering (data-density on the root before first render) gets a cheap vitest.
- The five dogfood notes land in docs/dogfood/ (outside the doc_sync mirror scope).
- The eng voice's fifteen findings are all accepted as spec amendments; the security posture is clean (no new IPC, no wire changes, no remote loads; the font is a vendored resource).
<!-- /autoplan-accepted:eng -->
## Review record

<!-- autoplan-accepted:ceo -->
- D1 density axis lands as one attribute-driven scale (comfortable 28px and compact 24px rows, 28 and 24px hit floors, pane padding, the message row's line switch), owned by appearance.ts beside data-font-size; the sub-24px targets (collapse chevrons, view tabs, rail icons, mini-month cells) move in the same change; row heights are floors: a larger font-size step grows the row rather than truncating its content; the General settings page gains the Density segmented control; vitest covers the axis switch and the control's persistence.
- D2 one bundled OFL variable font ships as an app resource ahead of the system stack in --font-ui; the alternatives are priced on the record (system stack only, bundled Latin plus system CJK, full CJK coverage) and option (b) is taken for aesthetic differentiation, recorded as such; the shortlist is Inter, Source Sans 3, and IBM Plex Sans; the selection criteria are x-height against the 24px compact floor, tabular numeral quality at 12 to 13px, Windows hinting at 100 and 125 per cent scaling, and a subset budget under 400KB; the pick is recorded as a decision entry before merge; mixed-script lines are bi-font by design and verified on one Chinese mail thread (the CJK fixture that CAND-1 adds) in both densities and both themes; tabular numerals on every count, timestamp, and duration.
- D3 one EmptyState component (glyph tile plus message plus optional primary action) replaces every bare empty state on the list pane, calendar, and people; loading skeletons and inline errors adopt the same family; the settings save error pattern generalises.
- D4 the folder pane gains per-account colour dots (eight defaults in rotation over an extensible token list, no settings UI in this slice) and the Unified inbox row with its unread badge; the Unified row is a visual placeholder that does not filter the list in this slice; the badge is live, summing the unread counts under account_folders[].folders[] in the mailbox-list snapshot, with a vitest asserting the sum against the multi-account fixture; no new data flow ships.
- D5 one AiChip component with provenance text, pending and approve states, and an always-explicit approve action; it serves the reading pane, the composer toolbar, and the digest; chips ship only where their behaviour is already live, and an approve action appears only where its behaviour exists.
- D6 motion tokens: a 100/120/140/160ms duration scale; --ease-spring belongs to the command palette alone; the token already exists in theme.css, and this slice adds no new consumer of it; row selection eases out at 120ms; prefers-reduced-motion drops everything.
- D7 the detection step takes the demo's shape: the trust decision is the headline, approve is the primary button; copy follows a-onboarding.html; the trust rules themselves are unchanged; the reframe is a recorded hypothesis (trust-first raises completion), falsified cheaply by first-run dogfood notes.
- Accepted expansion CAND-1 (taste, gate item): the fixture harness gains event injection (fixture surface_changed pushes and dispatch-driven snapshot swaps) with harness.test.ts coverage, so this slice's selected, reading, and modal states verify in the harness instead of only in the built app; docs/quality-workflow.md and its Chinese review copy update in the same change, and every spec amendment this review lands is mirrored into this spec's own Chinese review copy in the same commit.
- Verification: vitest green (existing 920 plus new), the Rust floor untouched and green, the gate green, /design-review per surface in both densities and both themes, one Chinese mail thread through the CJK fixture in both densities and both themes, and the light theme's first full audit, timeboxed: the slice fixes the defects it touches and records the rest as findings for the next slice.
- Known gaps stand: the font face is picked in implementation, B and C elements stay unshipped, the capabilities matrix does not move, high-contrast needs no density variant.
<!-- /autoplan-accepted:ceo -->


<!-- autoplan-baseline-edits:ceo {"sourceSha256":"a04677f4fab0bc3ec7f69d899d78fb901a8151fc1550610bdb82316392fc6ee0","replacements":[{"oldText":"finished. This slice ships it.","newText":"finished. This slice ships it. Two costs are paid on the record. The\nrejected alternative is per-surface accretion with a later consolidation:\ncheaper this month, but it re-litigates density and states on every\nsurface and lands SP2p's multi-account model with no visual language\nready. The pace cost is one slice of calendar time with no new\nuser-facing capability: this slice is enabling infrastructure for SP2m\nonward, not a differentiator, and the differentiated behaviour (AI\napprove, multi-account) stays on the critical path."},{"oldText":"  row heights (comfortable 28px, compact 24px), hit floors (28/24px, and\n  never below 24px anywhere), pane padding, and the message row's\n  three-line/one-line switch. The sub-24px targets the audits deferred\n  (collapse chevrons, view tabs, rail icons, mini-month cells) move to the\n  axis in the same change; the answer to the 14px/16px body question\n  becomes a user-facing setting, not a fixed value. The General settings\n  page gains the Density segmented control beside Font size.","newText":"  row heights (comfortable 28px, compact 24px), hit floors (28/24px, and\n  never below 24px anywhere), pane padding (comfortable 12px, compact\n  8px), and the message row's\n  three-line/one-line switch. The sub-24px targets the audits deferred\n  (collapse chevrons, view tabs, rail icons, mini-month cells) move to the\n  axis in the same change; the answer to the 14px/16px body question\n  becomes a user-facing setting, not a fixed value. Row heights are\n  floors: a larger font-size step grows the row rather than truncating\n  its content. The General settings\n  page gains the Density segmented control beside Font size."},{"oldText":"- **D2 (the typographic voice)**: one bundled variable font ships as an\n  app resource ahead of the system stack in `--font-ui`, keeping the\n  offline guarantee (the resource bundles with the app; the system stack\n  remains the fallback). Candidates are OFL faces; the face itself is\n  chosen in the implementation slice with a side-by-side against the demo\n  pages. Tabular numerals (`font-feature-settings: 'tnum'`) apply to\n  every count, timestamp, and duration.","newText":"- **D2 (the typographic voice)**: one bundled variable font ships as an\n  app resource ahead of the system stack in `--font-ui`. The alternatives,\n  priced: (a) system stack only, zero cost and zero voice; (b) a bundled\n  Latin face with the system CJK pairing, voice with a bi-font cost on\n  mixed-script lines; (c) a bundled face with full CJK coverage, the most\n  voice and the most weight. The slice takes (b), and the reason is\n  aesthetic differentiation, recorded as such. The shortlist: Inter,\n  Source Sans 3, and IBM Plex Sans, all OFL with true tabular numerals\n  and variable weights. The selection criteria: x-height against the\n  24px compact floor, tabular numeral quality at 12 to 13px, Windows\n  hinting at 100 and 125 per cent scaling, and a subset bundle budget\n  under 400KB. The pick is recorded as a decision entry (like 01bb418e)\n  before merge, with the demo pages as the comparison surface.\n  Mixed-script lines are bi-font by design (the bundled Latin face, then\n  the system CJK pairing), and the verification includes one Chinese\n  mail thread in both densities and both themes, through a CJK fixture\n  message the harness gains with CAND-1. The font loads with\n  `font-display: swap` and `size-adjust`/`ascent-override` metric\n  matching to the fallback stack, and no layout shift on cold load is\n  on the visual verification list. Tabular numerals\n  (`font-feature-settings: 'tnum'`) apply to\n  every count, timestamp, and duration."},{"oldText":"- **D3 (one state family)**: a single `EmptyState` component\n  (glyph tile + message + optional primary action, the demo's `.state`\n  pattern) replaces every bare empty state: the list pane's \"Select a\n  folder\", the calendar and people empties, and every surface's loading\n  skeleton and inline error adopt the same family. The settings save\n  error (specific, at the source, amber) is the error pattern, generalised.","newText":"- **D3 (one state family)**: a single `EmptyState` component\n  (glyph tile + message + optional primary action, the demo's `.state`\n  pattern) replaces every bare empty state: the list pane's \"Select a\n  folder\", the calendar and people empties, and every surface's loading\n  skeleton and inline error adopt the same family. The settings save\n  error (specific, at the source, amber) is the error pattern, generalised,\n  and its counterpart is recorded: success stays silent on settings\n  saves. Loading skeletons are row-height-matched bars that follow the\n  density attribute (28/24px), three bars per pane, with the shimmer the\n  demos show. Each surface's EmptyState content (glyph, message, action)\n  takes the demo markup as canonical; implementers do not re-derive it."},{"oldText":"- **D4 (the multi-account model)**: the folder pane gains per-account\n  colour dots (a fixed eight-colour account palette beside the accent\n  skins) and the Unified inbox row with its unread badge, ahead of SP2p's\n  All Accounts group; the visual language lands here, the behaviour there.","newText":"- **D4 (the multi-account model)**: the folder pane gains per-account\n  colour dots and the Unified inbox row with its unread badge, ahead of\n  SP2p's All Accounts group. The account palette is eight defaults\n  assigned in rotation over an extensible token list (no settings UI in\n  this slice); the dot is 8px round, set before the account name, the\n  same eight in both themes, with per-theme palette validation inside\n  the timeboxed light audit; rotation collisions are acceptable at eight\n  accounts or fewer and documented beyond. The Unified row sits at the\n  top of the folder pane above the first account group and is inert in\n  this slice: disabled styling with no hover affordance and a \"Unified\n  inbox arrives with SP2p\" tooltip, so it never trains a dead click.\n  Its badge is live, summing the `unread` counts under\n  `account_folders[].folders[]` in the mailbox-list snapshot (partial\n  data sums the available accounts with no warning chrome, pinned by a\n  fixture case), with a vitest asserting the sum against the\n  multi-account fixture; no new data flow ships. The badge hierarchy\n  reads aggregate, account, folder: account headers and folders keep\n  their existing unread counts."},{"oldText":"- **D5 (the AI grammar)**: every AI chip carries its provenance\n  (\"from this thread\"), its state (pending dashed, approve accent border),\n  and an always-explicit approve action, matching the sidecar's\n  fail-closed posture. One `AiChip` component serves the reading pane,\n  the composer toolbar, and the digest.","newText":"- **D5 (the AI grammar)**: every AI chip carries its provenance\n  (\"from this thread\" on the reading pane, \"from your draft\" in the\n  composer, \"from today's inbox\" in the digest), its state, and an\n  always-explicit approve action, matching the sidecar's\n  fail-closed posture. One `AiChip` component serves the reading pane,\n  the composer toolbar, and the digest. Chips ship only where their\n  behaviour is already live (the reading pane's summarise and draft\n  chips, the composer's autocomplete, the digest); an approve action\n  appears only where its behaviour exists. The lifecycle has five\n  states: pending (dashed), generating (shimmer within the dashed\n  frame), approve (accent border), approved (resolves to plain\n  confirmed text), and failed (amber border with a retry); every state\n  verifies in the harness through CAND-1's event injection."},{"oldText":"rule that `--ease-spring` belongs to the command palette alone; row\n  selection eases out at 120ms; everything drops to instant under\n  `prefers-reduced-motion`. No other animation is added in this slice.","newText":"rule that `--ease-spring` belongs to the command palette alone; the token\n  already exists in `theme.css`, and this slice adds no new consumer of it;\n  row selection eases out at 120ms; everything drops to instant under\n  `prefers-reduced-motion`. No other animation is added in this slice."},{"oldText":"  action is the primary button. Copy follows `a-onboarding.html`; the\n  trust rules themselves are unchanged.","newText":"  action is the primary button. Copy follows `a-onboarding.html`; the\n  trust rules themselves are unchanged. The reframe is a recorded\n  hypothesis (that trust-first raises completion), falsified cheaply:\n  the next five first-run dogfood completions carry notes on the\n  detection step, and abandonment or hesitation there counts against\n  the hypothesis."},{"oldText":"- The fixture harness serves at-rest snapshots only, so selected, reading,\n  and modal states verify in the built app, not the harness; harness\n  event injection is its own follow-up task.","newText":"- The fixture harness serves at-rest snapshots only at this slice's\n  start; CAND-1 resolves that in this slice (event injection with test\n  coverage), so selected, reading, and modal states verify in the\n  harness."},{"oldText":"- Visual verification: `/design-review` against the fixture harness per\n  surface, in both densities and both themes; the audit's deferred\n  findings close or stand recorded as intentionally answered.\n- The light theme gets its first full audit pass in this slice (both\n  audits so far ran dark-only; recorded at baseline).","newText":"- Visual verification: `/design-review` against the fixture harness per\n  surface, in both densities and both themes; the audit's deferred\n  findings close or stand recorded as intentionally answered. One Chinese\n  mail thread verifies the bi-font mix through the CJK fixture, in both\n  densities and both themes. The calendar's mini-month and the folder\n  pane render in both densities as named checks (the 24px floor grows\n  the mini-month's cells and the tree's indentation). No layout shift on\n  cold load with the font swap.\n- The light theme gets its first full audit pass in this slice (both\n  audits so far ran dark-only; recorded at baseline), timeboxed: the\n  slice fixes the density and token defects it touches and records the\n  rest as findings for the next slice."}]} -->


<!-- autoplan-baseline-edits:design {"sourceSha256":"e8f513fb8e1da85886e3da6e1e744ab330c72a86e2310f91bc8aa31d86922a24","replacements":[{"oldText":"  row heights (comfortable 28px, compact 24px), hit floors (28/24px, and\n  never below 24px anywhere), pane padding, and the message row's","newText":"  row heights (comfortable 28px, compact 24px), hit floors (28/24px, and\n  never below 24px anywhere), pane padding (comfortable 12px, compact\n  8px), and the message row's"},{"oldText":"  message the harness gains with CAND-1. Tabular numerals\n  (`font-feature-settings: 'tnum'`) apply to\n  every count, timestamp, and duration.","newText":"  message the harness gains with CAND-1. The font loads with\n  `font-display: swap` and `size-adjust`/`ascent-override` metric\n  matching to the fallback stack, and no layout shift on cold load is\n  on the visual verification list. Tabular numerals\n  (`font-feature-settings: 'tnum'`) apply to\n  every count, timestamp, and duration."},{"oldText":"  error (specific, at the source, amber) is the error pattern, generalised.","newText":"  error (specific, at the source, amber) is the error pattern, generalised,\n  and its counterpart is recorded: success stays silent on settings\n  saves. Loading skeletons are row-height-matched bars that follow the\n  density attribute (28/24px), three bars per pane, with the shimmer the\n  demos show. Each surface's EmptyState content (glyph, message, action)\n  takes the demo markup as canonical; implementers do not re-derive it."},{"oldText":"- **D4 (the multi-account model)**: the folder pane gains per-account\n  colour dots and the Unified inbox row with its unread badge, ahead of\n  SP2p's All Accounts group. The account palette is eight defaults\n  assigned in rotation over an extensible token list (no settings UI in\n  this slice). The Unified row is a visual placeholder: it does not\n  filter the list here (SP2p wires the behaviour), while its badge is\n  live, summing the `unread` counts under `account_folders[].folders[]`\n  in the mailbox-list snapshot, with a vitest asserting the sum against\n  the multi-account fixture; no new data flow ships.","newText":"- **D4 (the multi-account model)**: the folder pane gains per-account\n  colour dots and the Unified inbox row with its unread badge, ahead of\n  SP2p's All Accounts group. The account palette is eight defaults\n  assigned in rotation over an extensible token list (no settings UI in\n  this slice); the dot is 8px round, set before the account name, the\n  same eight in both themes, with per-theme palette validation inside\n  the timeboxed light audit; rotation collisions are acceptable at eight\n  accounts or fewer and documented beyond. The Unified row sits at the\n  top of the folder pane above the first account group and is inert in\n  this slice: disabled styling with no hover affordance and a \"Unified\n  inbox arrives with SP2p\" tooltip, so it never trains a dead click.\n  Its badge is live, summing the `unread` counts under\n  `account_folders[].folders[]` in the mailbox-list snapshot (partial\n  data sums the available accounts with no warning chrome, pinned by a\n  fixture case), with a vitest asserting the sum against the\n  multi-account fixture; no new data flow ships. The badge hierarchy\n  reads aggregate, account, folder: account headers and folders keep\n  their existing unread counts."},{"oldText":"- **D5 (the AI grammar)**: every AI chip carries its provenance\n  (\"from this thread\"), its state (pending dashed, approve accent border),\n  and an always-explicit approve action, matching the sidecar's\n  fail-closed posture. One `AiChip` component serves the reading pane,\n  the composer toolbar, and the digest. Chips ship only where their\n  behaviour is already live (the reading pane's summarise and draft\n  chips, the composer's autocomplete, the digest); an approve action\n  appears only where its behaviour exists.","newText":"- **D5 (the AI grammar)**: every AI chip carries its provenance\n  (\"from this thread\" on the reading pane, \"from your draft\" in the\n  composer, \"from today's inbox\" in the digest), its state, and an\n  always-explicit approve action, matching the sidecar's\n  fail-closed posture. One `AiChip` component serves the reading pane,\n  the composer toolbar, and the digest. Chips ship only where their\n  behaviour is already live (the reading pane's summarise and draft\n  chips, the composer's autocomplete, the digest); an approve action\n  appears only where its behaviour exists. The lifecycle has five\n  states: pending (dashed), generating (shimmer within the dashed\n  frame), approve (accent border), approved (resolves to plain\n  confirmed text), and failed (amber border with a retry); every state\n  verifies in the harness through CAND-1's event injection."},{"oldText":"  hypothesis (that trust-first raises completion), falsified cheaply by\n  first-run dogfood notes, not asserted as settled.","newText":"  hypothesis (that trust-first raises completion), falsified cheaply:\n  the next five first-run dogfood completions carry notes on the\n  detection step, and abandonment or hesitation there counts against\n  the hypothesis."},{"oldText":"  findings close or stand recorded as intentionally answered. One Chinese\n  mail thread verifies the bi-font mix through the CJK fixture, in both\n  densities and both themes.","newText":"  findings close or stand recorded as intentionally answered. One Chinese\n  mail thread verifies the bi-font mix through the CJK fixture, in both\n  densities and both themes. The calendar's mini-month and the folder\n  pane render in both densities as named checks (the 24px floor grows\n  the mini-month's cells and the tree's indentation). No layout shift on\n  cold load with the font swap."}]} -->

<!-- autoplan-baseline-edits:dx {"sourceSha256":"24904f52d49bf1cccbf041389d45675286bb99573cd58ac8538cc01acad9f7b3","replacements":[{"oldText":"  `appearance.ts` beside the existing font-size steps. One scale drives","newText":"  `appearance.ts` beside the existing font-size steps. Comfortable is the\n  default: an absent attribute behaves as comfortable, and\n  `appearance.ts` writes the attribute before the shell's first\n  interactive paint, so no density flash on first paint joins the visual\n  verification list beside the font check. One scale drives"},{"oldText":"  demos show. Each surface's EmptyState content (glyph, message, action)\n  takes the demo markup as canonical; implementers do not re-derive it.","newText":"  demos show. Each surface's EmptyState content (glyph, message, action)\n  lives in a content map colocated with the components, and the demo\n  markup references that map; implementers do not re-derive it. The\n  component contracts are pinned here: `EmptyState { glyph, message,\n  action? }` and `AiChip { provenance, state, onApprove, onRetry }`.\n  Inline error copy names the problem, states the cause when known, and\n  ends with the recovery action; the settings save error is the worked\n  example."},{"oldText":"  the timeboxed light audit; rotation collisions are acceptable at eight\n  accounts or fewer and documented beyond. The Unified row sits at the\n  top of the folder pane above the first account group and is inert in\n  this slice: disabled styling with no hover affordance and a \"Unified\n  inbox arrives with SP2p\" tooltip, so it never trains a dead click.","newText":"  the timeboxed light audit; rotation collisions are acceptable at eight\n  accounts or fewer, and the policy beyond (how a developer adds or\n  remaps a palette token in `theme.css`) lives in the design-system\n  reference D6 names. The Unified row sits at the\n  top of the folder pane above the first account group and is inert in\n  this slice: inert styling with `aria-disabled` and normal pointer\n  events, so the \"Unified inbox arrives with SP2p\" tooltip fires (a\n  genuinely disabled control would swallow them)."},{"oldText":"  `prefers-reduced-motion`. No other animation is added in this slice.","newText":"  `prefers-reduced-motion`. No other animation is added in this slice.\n  The tokens and rules this slice mints (the density scale, the duration\n  scale, the ease-spring reservation, the tabular-numerals rule, the\n  silent-success rule, the palette collision policy) are enumerated in a\n  short design-system reference (`docs/design-system.md` with its Chinese\n  review copy), and the ease-spring reservation gains a comment at the\n  token definition. Appearance attributes are concept-named and\n  word-valued (`data-density`); `data-font-size` is the grandfathered\n  exception, not a second precedent."}]} -->

<!-- autoplan-baseline-edits:eng {"sourceSha256":"6d5a05dd6d9ef51cfabeadd6cb1bf8c30fb126f4e060aa74e1395281a1fa53a5","replacements":[{"oldText":"  floors: a larger font-size step grows the row rather than truncating\n  its content. The General settings\n  page gains the Density segmented control beside Font size.","newText":"  floors: a larger font-size step grows the row rather than truncating\n  its content. The virtualized message list's estimate derives from the\n  active density (the fixed `estimateSize` follows the axis, or rows\n  re-measure with stale estimates and the scrollbar drifts). The General\n  settings page gains the Density segmented control beside Font size."},{"oldText":"  matching to the fallback stack, and no layout shift on cold load is\n  on the visual verification list. Tabular numerals","newText":"  matching to the fallback stack, and no layout shift on cold load is\n  on the visual verification list; the overrides are webview-dependent,\n  so the oldest supported webview verifies them explicitly, with the\n  fallback recorded (accept the swap shift there, or bump the floor).\n  The vendored face pins its exact upstream release and checksum, with\n  the OFL licence text beside the resource, both noted in the\n  design-system reference. The subset budget is enforced by a size\n  check in the xtask gate, or the budget is a wish. Tabular numerals"},{"oldText":"  lives in a content map colocated with the components, and the demo\n  markup references that map; implementers do not re-derive it. The","newText":"  lives in a content map colocated with the components, the single\n  source of truth; the demo markup is a point-in-time snapshot of that\n  map, and implementers do not re-derive it. The"},{"oldText":"  ends with the recovery action; the settings save error is the worked\n  example.","newText":"  ends with the recovery action; the settings save error is the worked\n  example. The reduced-motion contract covers everything, the skeleton\n  shimmer included, and a named check proves it."},{"oldText":"  Its badge is live, summing the `unread` counts under\n  `account_folders[].folders[]` in the mailbox-list snapshot (partial\n  data sums the available accounts with no warning chrome, pinned by a\n  fixture case), with a vitest asserting the sum against the\n  multi-account fixture; no new data flow ships. The badge hierarchy\n  reads aggregate, account, folder: account headers and folders keep\n  their existing unread counts.","newText":"  Its badge is live, summing the `unread` counts under\n  `account_folders[].folders[]` in the mailbox-list snapshot: every\n  folder except the junk and trash roles, per-mailbox counts with no\n  dedup needed (partial data sums the available accounts with no\n  warning chrome), pinned by fixture cases for the role exclusion and\n  the partial sum, and a vitest asserting the sum against the\n  multi-account fixture; no new data flow ships. The row hides when\n  `account_folders.length < 2` (one account makes it a duplicate),\n  pinned by a fixture case. The badge hierarchy\n  reads aggregate, account, folder: account headers and folders keep\n  their existing unread counts. The row stays focusable with the\n  tooltip on focus through `aria-describedby`, tested, so the keyboard\n  path is not a dead tab stop."},{"oldText":"  review copy), and the ease-spring reservation gains a comment at the\n  token definition. Appearance attributes are concept-named and","newText":"  review copy), and the ease-spring reservation gains a comment at the\n  token definition. Rows size with `min-height`; no fixed-height text\n  rows anywhere, a rule the reference states and the per-surface audit\n  checks line by line. Appearance attributes are concept-named and"},{"oldText":"  hypothesis (that trust-first raises completion), falsified cheaply:\n  the next five first-run dogfood completions carry notes on the\n  detection step, and abandonment or hesitation there counts against\n  the hypothesis.","newText":"  hypothesis (that trust-first raises completion), falsified cheaply:\n  the next five first-run dogfood completions carry notes on the\n  detection step, recorded in `docs/dogfood/` (a directory the doc_sync\n  gate does not mirror), and abandonment or hesitation there counts\n  against the hypothesis."},{"oldText":"- The light theme gets its first full audit pass in this slice (both\n  audits so far ran dark-only; recorded at baseline), timeboxed: the\n  slice fixes the density and token defects it touches and records the\n  rest as findings for the next slice.","newText":"- The light theme gets its first full audit pass in this slice (both\n  audits so far ran dark-only; recorded at baseline), timeboxed: the\n  slice fixes the density and token defects it touches, capped at one\n  working day, and records the rest as findings for the next slice."},{"oldText":"## 3. Testing\n\n- vitest per surface: the density axis switches every row height and hit\n  floor with one attribute; `EmptyState` renders its three parts; `AiChip`\n  renders provenance and both states; the settings page gains the Density\n  control and persists it through `appearance.ts`'s localStorage path.\n- Rust untouched: no intent, snapshot, or wire shape changes; the floor\n  (`cargo clippy --workspace --all-targets -- -D warnings`,\n  `cargo test --workspace`, the gate) must stay green without edits.\n- Visual verification: `/design-review` against the fixture harness per\n  surface, in both densities and both themes; the audit's deferred\n  findings close or stand recorded as intentionally answered. One Chinese\n  mail thread verifies the bi-font mix through the CJK fixture, in both\n  densities and both themes. The calendar's mini-month and the folder\n  pane render in both densities as named checks (the 24px floor grows\n  the mini-month's cells and the tree's indentation). No layout shift on\n  cold load with the font swap.","newText":"## 3. Testing\n\nThe landing order is recorded because verification depends on it:\nCAND-1 first (the harness and the CJK fixture), then the D1/D6 token\nwork, the D3 components, D4, D5, D7, and the light audit last.\n\nCAND-1's injection contract: the harness exposes an explicit injection\nAPI (a fixture event push with the surface name and payload), pushes\nare deep clones (never references into the recorded JSON), unlisten\nand cleanup semantics match the Tauri bridge's, and injected events\napply in dispatch order with no interleaving against real events\n(none exist in fixture mode). The alias applies only under\n`--mode fixtures`; a build-level assertion pins that the harness never\nlands in the production bundle. `harness.test.ts` pins all four.\n\n- vitest per surface: the density axis switches every row height and hit\n  floor with one attribute; `EmptyState` renders its three parts; `AiChip`\n  renders provenance and both states; the settings page gains the Density\n  control and persists it through `appearance.ts`'s localStorage path.\n  `data-density` sits on the document root before the first render\n  (a cheap ordering assertion).\n- Rust untouched: no intent, snapshot, or wire shape changes; the floor\n  (`cargo clippy --workspace --all-targets -- -D warnings`,\n  `cargo test --workspace`, the gate) must stay green without edits. The\n  gate also gains the font subset size check and the assertion that the\n  fixture harness never lands in the production bundle.\n- Visual verification: `/design-review` against the fixture harness per\n  surface, in both densities and both themes; the audit's deferred\n  findings close or stand recorded as intentionally answered. One Chinese\n  mail thread verifies the bi-font mix through the CJK fixture, in both\n  densities and both themes. The calendar's mini-month and the folder\n  pane render in both densities as named checks (the 24px floor grows\n  the mini-month's cells and the tree's indentation), and the risky\n  cell runs too: font-size large in both densities on both panes.\n  No layout shift on\n  cold load with the font swap. With the large-list fixture, toggling\n  Density mid-session and scrolling shows no overlap and no blank gaps."}]} -->

<!-- AUTONOMOUS DECISION LOG -->
## Decision Audit Trail

| # | Phase | Decision | Classification | Principle | Rationale | Rejected |
|---|-------|----------|----------------|-----------|-----------|----------|
| 1 | CEO | Mode: SELECTIVE EXPANSION | Mechanical | override | The autoplan route; hold scope plus cherry-picks | HOLD, REDUCTION, EXPANSION |
| 2 | CEO | One design-system slice, not piecemeal fixes, not a rewrite | Mechanical | P1+P5 | The audits diagnosed system-level gaps; piecemeal re-litigates per component, a rewrite is not justified by an A- shell | piecemeal per-finding fixes; full rewrite |
| 3 | CEO | CAND-1: harness event injection joins scope | Taste | P2 | Inside the slice's verification blast radius and under a day; without it selected, reading, and modal states verify only by hand in the built app | verify those states manually |
| 4 | CEO | Serif reading column stays out | Mechanical | user decision | Recorded in the spec's known gaps; the pick was A on all surfaces | add as an option |
| 5 | CEO | Command palette stays out | Mechanical | user decision | Same recorded gap | add as an option |
| 6 | CEO | Spec review loop round 1: six fixes applied (harness gap rewritten as resolved by CAND-1; D1 rows are floors under larger font steps; D4 badge sums existing snapshot counts; zh-CN spec mirror joins the doc deliverables; skips reframed as future picks) | Mechanical | P5 | Reviewer evidence; no behaviour changed, the documents now agree | leave the contradiction standing |
| 7 | CEO | Serif column and palette framed as deferred future picks, not rejections | Mechanical | P1 | The user may cherry-pick later; rejection forecloses, deferral does not | rejection framing |
| 8 | CEO | Native CEO voice (subagent-only; Codex not installed): 11 findings, all accepted as spec amendments | Mechanical | P5+P1 | Sequencing cost and the enabling-infrastructure sentence now on the record; D2 priced alternatives, shortlist, criteria, and a recorded pick gate; CJK bi-font with a real thread check; D4 names the snapshot field with a sum assertion and the Unified row as visual placeholder; D5 chips only where behaviour is live; D7 as falsifiable hypothesis; palette extensible; light audit timeboxed; direction unchanged | reject the voice's amendments |
| 9 | CEO | Density is a committed settings surface | Mechanical | P6 | The persisted key follows appearance.ts's existing naming; unknown values fall back to comfortable; the axis is contractual for SP2m onward | treat the axis as an experiment |

## CEO review output

### Dual voices consensus (outside unavailable, subagent-only)

| Dimension | Claude | Codex | Consensus |
|---|---|---|---|
| 1. Premises valid? | amended | n/a | N/A |
| 2. Right problem to solve? | yes | n/a | N/A |
| 3. Scope calibration correct? | yes | n/a | N/A |
| 4. Alternatives sufficiently explored? | amended | n/a | N/A |
| 5. Competitive/market risks covered? | yes | n/a | N/A |
| 6. 6-month trajectory sound? | yes | n/a | N/A |

Codex is not installed on this host; outside coverage is unavailable and
nothing here counts as clean-by-outside. The native voice's 11 findings
were all accepted as amendments (audit trail row 8).

### Reality rows

- REALITY: premise-table pass: every factual claim in the spec carries its receipt (docs/superpowers/specs/2026-10-13-ui1-evolved-iris-design.md:16-21, client/web/src/services/appearance.ts:46-94, client/web/src/services/__fixtures__/harness.ts:1-20) unverified=0
- REALITY: already-done pass: git log on main shows no prior density-axis or EmptyState work (commits 3d5ec59, c4dc26b); the kylins reskin (3d5ec59) predates and does not overlap
- REALITY: surface-check pass: no CLI commands, flags, or HTTP routes named in this spec (docs/superpowers/specs/2026-10-13-ui1-evolved-iris-design.md:101-118)
- REALITY: binding-decisions pass: decision 01bb418e (direction A) honored; no binding decision is contradicted
- REALITY: numbers pass: 920 tests re-measured by the spec reviewer full vitest run (103 files, client/web/package.json:6-11); 28/24px and 100/120/140/160ms are the demo measured values [measured: client/web/.gstack/designs/redesign/a-mail.html, c-mail.html, client/web/src/styles/theme.css:130-138]
- REALITY: deferred-asks pass: the spec own known gaps list what it leaves out and why (docs/superpowers/specs/2026-10-13-ui1-evolved-iris-design.md:120-135)
- REALITY: pr-count pass: one PR per wave, no ship-policy.json override (docs/quality-workflow.md:54-59); the slice lands as one PR

### Section reviews (implementation-ready depth)

- Section 1 (architecture): presentation-layer only: appearance.ts owns one new root attribute beside its existing three; theme.css carries the density scale; EmptyState and AiChip are leaf components; the folder pane reads fields already in its snapshot; the harness change is dev-only. No new coupling into the engine, the host, or the wire. Diagram: appearance.ts -> [data-density] -> theme.css scale -> components; harness fixture pushes -> surface_changed -> existing pullSurface path.
- Section 2 (error and rescue): font resource fails to load -> per-glyph fallback to the system stack (the stack never leaves --font-ui); unknown persisted density -> comfortable (the recorded migration policy); harness event injection misbehaves -> test-only, fails closed because pulls still answer fixtures; subset font missing a glyph -> per-glyph system fallback. No catch-alls introduced; no silent path: every rescue is visible in the rendered result.
- Section 3 (security): one new static asset class (the vendored OFL face, pinned by hash at import time); no new input, endpoint, or permission; the fixtures mode stays dev-only (`vite --mode fixtures`, never a production build flag).
- Section 4 (data flow and interaction): density switch mid-session is an attribute swap the CSS owns (no persisted-state migration); zero accounts makes the Unified badge sum 0 and the badge suppresses; a zero-count account keeps its dot; double-clicking the Density control is idempotent (same key, same value).
- Section 5 (code quality): the reuse ladder held at rung 1 twice (appearance.ts's existing attribute mechanism extended, ReadingPane's state pattern generalised into EmptyState rather than a parallel component built).
- Section 6 (tests): vitest per decision (axis switch, control persistence, EmptyState parts, AiChip states, badge sum against the multi-account fixture, harness event injection); no_claim=rendered fidelity beyond jsdom: the /design-review passes in both densities and both themes cover what the unit tests cannot; the mock is not a live check, recorded.
- Section 7 (performance): the subset font budget is under 400KB with font-display: swap; a density switch costs one relayout; no list virtualisation changes.
- Section 8 (observability): N/A beyond existing client logging; the D7 hypothesis's falsifier is dogfood notes recorded by hand, no telemetry added (the repo adds none here).
- Section 9 (deployment): atomic per release; the font joins the Tauri bundle resources; rollback is a revert; no persisted-schema migration (unknown values fall back).
- Section 10 (trajectory): reversibility 4/5 (all presentation; the persisted density key is the one lasting artifact); the axis is contractual for SP2m onward, recorded; palette extensible by token edit.
- Section 11 (design and UX): the slice is the design system itself; the state coverage map (loading, empty, error, success) is completed by D3; accessibility holds (24px floors, existing focus rings, unchanged contrast, the CJK bi-font check); responsive is N/A by product contract; recommendation recorded: run /design-review per surface after implementation, in both densities and themes.

### NOT in scope

- Serif reading column and command palette (deferred future picks; audit trail rows 4, 5, 7).
- A density variant for high-contrast mode (recorded: the flattened token blocks do not interact with the axis).
- The Unified row's filtering behaviour and the All Accounts group (SP2p).
- A bundled face with full CJK coverage (priced and rejected in D2 as more weight than the voice is worth).

### What already exists

- appearance.ts owns the root-attribute + localStorage mechanism the axis extends (lines 46-94).
- theme.css's token blocks and the @theme inline mapping (globals.css) the scale joins; --ease-spring already defined.
- ReadingPane's centred-icon state (the EmptyState pattern's origin) and the settings save error (the error pattern's origin).
- mailbox_list fixture's per-account folder unread counts (the Unified badge's data).
- The fixture harness and its test (CAND-1 extends them).

### Dream state delta

  CURRENT STATE                  THIS PLAN                        12-MONTH IDEAL
  tokens settled, five system    density, voice, states, motion,   every surface ships from one
  findings deferred, no AI       AI grammar, multi-account visual  system; slices cost content,
  visual grammar                 language, harness-verifiable      not design debates; audits
                                 states                            re-run clean in both themes

### Error and rescue registry

| Capability | Failure mechanism | Rescued? | Rescue action | User sees |
|---|---|---|---|---|
| Bundled font load | resource missing/corrupt | Y | per-glyph system fallback | the system face, never boxes |
| Persisted density | unknown/legacy value | Y | fall back to comfortable | comfortable density |
| Unified badge | snapshot without folders | Y | sum over empty set, badge suppressed | no badge |
| Harness injection (dev) | malformed push | Y | test fails loudly; pulls still answer fixtures | n/a (dev only) |
| Subset font glyph gap | glyph outside the subset | Y | per-glyph fallback in the stack | the fallback face for that glyph |

### Failure modes registry

| CODEPATH | FAILURE MODE | RESCUED? | TEST? | USER SEES? | LOGGED? |
|---|---|---|---|---|---|
| density axis | unknown persisted value | Y | Y | comfortable | n/a |
| font loading | missing resource | Y | visual pass | system face | n/a |
| unified badge | empty snapshot | Y | Y | no badge | n/a |
| harness injection | malformed event | Y | Y | test failure | console |

No CRITICAL GAP rows.

### Completion summary (CEO phase)

- Mode: SELECTIVE EXPANSION. System audit: clean tree, no TODOS.md, no stashes, no binding-decision conflicts.
- Step 0: 5 auto-decisions (audit rows 1-5) plus 4 amendment rounds (rows 6-9); one accepted cherry-pick (CAND-1, taste).
- Sections 1-11: no unresolved findings after the voice's 11 amendments; Section 11 ran (UI scope).
- Outside voice: codex not installed, coverage unavailable (subagent-only); spec review loop: 2 iterations, 7 issues found and fixed, score 9.
- Reality rows: 7/7 pass, 0 unverified. Lake Score: N/A (no coverage-scored questions asked of the user).
- Unresolved decisions: none open from this phase; one taste decision (CAND-1) waits at the final gate.

<!-- autoplan-accepted:design -->
- D1 gains the pane padding numbers: comfortable 12px, compact 8px.
- D2 gains the font-loading policy: font-display swap with size-adjust/ascent-override metric matching to the fallback stack; no layout shift on cold load is on the visual verification list.
- D3 gains the skeleton rule (row-height-matched bars following the density attribute, three per pane, shimmer), the success rule (settings saves stay silent on success), and canonical per-surface EmptyState content taken from the demo markup.
- D4 gains the Unified row's full contract: top of the folder pane above the first account group; inert with disabled styling, no hover affordance, and a "Unified inbox arrives with SP2p" tooltip; badge live and summing the snapshot's unread counts (partial data sums available accounts, no warning chrome, fixture case pinned); badge hierarchy aggregate, account, folder with existing counts kept. The dot: 8px round before the account name, the same eight colours in both themes, per-theme palette validation inside the light audit, collisions acceptable at eight accounts or fewer and documented beyond.
- D5 gains the five-state lifecycle (pending dashed, generating shimmer, approve accent border, approved resolves to plain confirmed text, failed amber with retry) verified through CAND-1 injection, and per-surface provenance copy ("from this thread" reading pane, "from your draft" composer, "from today's inbox" digest).
- D7 gains the falsification bar: the next five first-run dogfood completions carry notes on the detection step; abandonment or hesitation there counts against the hypothesis.
- Testing gains named checks: the calendar mini-month and folder pane in both densities (the 24px floor grows the mini-month cells and tree indentation), and no layout shift on cold load.
- The Unified row's inert-with-tooltip treatment is the provisional answer to a taste decision (alternatives: defer the row to SP2p, or ship it looking functional); it waits at the final gate.
<!-- /autoplan-accepted:design -->
| 10 | DESIGN | Native design voice (subagent-only; Codex not installed): 14 findings, 13 auto-fixed as spec text, 1 taste | Mechanical | P5 | Structural specification gaps closed: Unified row interaction contract, chip five-state lifecycle, font-loading policy, skeleton rule, padding numbers, dot spec, badge hierarchy, falsification bar, named density checks | leave the gaps to implementer invention |
| 11 | DESIGN | Unified row treatment: inert with disabled styling and an SP2p tooltip | Taste | P5 | The reviewer's recommended middle path: it preserves the tease honestly without training a dead click, unlike looking functional (worst) or deferring the row entirely | ship it looking functional; defer the row to SP2p |

## Design review output

### Litmus scorecard (evidence: the twelve approved demo pages)

| Check | Claude | Codex | Consensus |
|---|---|---|---|
| 1. Brand unmistakable in first screen? | YES | n/a | N/A |
| 2. One strong visual anchor? | YES | n/a | N/A |
| 3. Scannable by headlines only? | YES | n/a | N/A |
| 4. Each section has one job? | YES | n/a | N/A |
| 5. Cards actually necessary? | N/A (panes are layout) | n/a | N/A |
| 6. Motion improves hierarchy? | YES | n/a | N/A |
| 7. Premium without decorative shadows? | YES | n/a | N/A |

Hard rejections triggered: none. Codex unavailable (not installed).

### Passes (0-10 before fixes, after)

- Pass 1 (information architecture): 6 → 9. The Unified row's position, inert contract, and the aggregate-account-folder badge hierarchy are now specified (F1, F2).
- Pass 2 (interaction states): 5 → 9. The chip's five-state lifecycle, the skeleton rule, the partial-data badge rule, and the silent-success rule are now specified (F3, F4, F6, F7).
- Pass 3 (user journey): 6 → 8. The dead-click journey break closes with the inert treatment (F8); the onboarding hypothesis has a falsification bar (F9).
- Pass 4 (AI slop risk): 9 → 9. No slop patterns: the slice is token-driven throughout, no gradients, no card mosaics, no icon tiles; the demos are the evidence.
- Pass 5 (design system alignment): 7 → 9. No DESIGN.md exists (flagged as a gap for /design-consultation); the plan calibrates against theme.css's tokens, and the padding, dot, and density knock-on numbers are now specified (F10, F11, F12).
- Pass 6 (responsive and accessibility): 7 → 9. The 24px floors carry named rendering checks; the CJK bi-font check and the no-layout-shift font load join verification; responsive is N/A by the desktop product contract, recorded.
- Pass 7 (unresolved decisions): 14 raised, 13 resolved as spec text, 1 provisional (the Unified row's inert treatment, taste, at the gate).

### Completion summary (design phase)

- Overall design score (lowest of passes 1-6): 5/10 → 9/10.
- Approved mockups: the twelve demo pages under client/web/.gstack/designs/redesign/ with the four comparison boards; direction A approved through the board flow (decision 01bb418e).
- Decisions made: 13 spec amendments plus 1 provisional taste decision.
- Decisions deferred: none open from this phase; the taste item waits at the final gate.
- NOT in scope additions: deferring the Unified row to SP2p entirely (considered, rejected for the inert treatment, provisional); a DESIGN.md export (deferred to /design-consultation).

<!-- autoplan-accepted:dx -->
- D1 gains the default: comfortable; an absent attribute behaves as comfortable; appearance.ts writes the attribute before the shell's first interactive paint; no density flash on first paint joins the visual verification list.
- D3 gains the pinned contracts EmptyState { glyph, message, action? } and AiChip { provenance, state, onApprove, onRetry }; the per-surface EmptyState content lives in a content map colocated with the components (the demos reference it); the inline error-copy standard (problem, cause when known, recovery action; the settings save error as the worked example).
- D4 gains: the inert row uses aria-disabled with normal pointer events so the tooltip fires; the palette collision and extension policy lives in the design-system reference D6 names.
- D6 gains: a short design-system reference (docs/design-system.md with its Chinese review copy) enumerating the slice's tokens and rules, the ease-spring reservation gains a comment at the token definition, and appearance attributes are concept-named and word-valued with data-font-size the grandfathered exception (provisional, a taste decision: the alternative is renaming data-font-size in this slice).
- The DX voice's eight findings are all accepted as spec amendments; the developer persona is the SP2m-onward implementer (and the contributing dogfooder); the magical moment is npm run dev:fixtures serving the real UI with real data in a plain browser, completed by CAND-1's event injection.
<!-- /autoplan-accepted:dx -->
| 12 | DX | Native DX voice (subagent-only; Codex not installed): 8 findings, all accepted as spec amendments | Mechanical | P1+P5 | Default density and first paint pinned; component contracts written; error-copy standard adopted; canonical content gets a permanent home; design-system reference docs page joins deliverables; tooltip contradiction resolved with aria-disabled | leave the defaults and contracts to per-surface invention |
| 13 | DX | Appearance attribute naming: grandfather data-font-size, concept-named word-valued from here | Taste | P5 | Renaming a persisted key is a breaking change for every existing user for zero user-visible gain; the convention stops the next inconsistency instead | rename data-font-size in this slice |

## DX review output

### Developer persona

Who: the SP2m-onward implementer, and the contributing dogfooder. Context: they consume the density axis, the EmptyState and AiChip components, and the fixture harness to build and verify the next slices. Tolerance: one reading of the design-system reference before their first surface; they abandon to per-surface invention when a contract is missing. Expects: pinned component contracts, canonical content in a permanent home, and a harness that shows the states they must verify.

### Developer empathy narrative

I open the spec to build my surface. The axis is one attribute, the components have pinned contracts, and the content map tells me what my empty state says; I never invent. I run npm run dev:fixtures and the real app renders in a plain browser with real data; CAND-1's injection shows me the selected, reading, and modal states my surface must survive. The design-review loop verifies my work in both densities and both themes. Where a rule exists, I find it in the design-system reference, not in folklore.

### Journey map

| Stage | Developer does | Friction | Status |
|---|---|---|---|
| Discover | reads the spec + design-system reference | rules once lived only in the spec | fixed (D6 reference) |
| Install | clones, stage-cli placeholder, sidecar build | two manual prerequisites | ok (documented in CLAUDE.md and AGENTS.md) |
| Hello world | npm run dev:fixtures in a plain browser | none after CAND-1 | fixed (CAND-1) |
| Real usage | puts a surface on the axis, drops EmptyState and AiChip | contracts were unpinned | fixed (D3 contracts) |
| Debug | gate plus vitest plus design-review loop | named checks for density knock-ons | ok |
| Upgrade | persisted density key with unknown-value fallback | absent-attribute behaviour was undefined | fixed (D1 default) |

### DX scorecard

| Dimension | Score | Note |
|---|---|---|
| Getting started | 8/10 | TTHW for an implementer: one reference read plus one dev command |
| API/CLI/SDK | 8/10 | axis and component contracts pinned; astra-cli surface unchanged |
| Error messages | 8/10 | the error-copy standard is now recorded with a worked example |
| Documentation | 8/10 | the design-system reference joins the spec and the quality workflow |
| Upgrade path | 8/10 | comfortable default, unknown-value fallback, grandfathered key rule |
| Dev environment | 9/10 | the harness plus injection completes the loop |
| Community | 5/10 | solo stage, GPL, no community surface yet; honest score, N/A by stage |
| DX measurement | 7/10 | design baselines, spec-review metrics, dogfood notes for D7 |
| Overall | 8/10 | mode: DX POLISH; product type: contributor/developer tooling |

TTHW: implementer hello world (axis + EmptyState + AiChip on one surface) under 5 minutes of reading plus one command; competitive tier. Magical moment: npm run dev:fixtures, completed by CAND-1.

### DX implementation checklist

- [x] Default density named with absent-attribute behaviour
- [x] Component contracts pinned (EmptyState, AiChip)
- [x] Canonical content in a permanent home (content map)
- [x] Error-copy standard with a worked example
- [x] Design-system reference with Chinese review copy in deliverables
- [x] Naming convention for appearance attributes recorded
- [x] Magical moment delivered via the fixture harness plus CAND-1

<!-- autoplan-accepted:eng -->
- Sequencing recorded: CAND-1 first, then D1/D6 tokens, D3 components, D4, D5, D7, light audit last (capped at one working day).
- D1: the virtualized list's estimate derives from the active density; the large-list mid-session density toggle check (no overlap, no blank gaps); the risky cell (font-size large in both densities) runs on the mini-month and folder pane.
- D2: the subset budget is enforced by a size check in the xtask gate; the vendored face pins its upstream release and checksum with the OFL licence text beside the resource; the oldest supported webview verifies the metric overrides explicitly with the fallback recorded.
- D3: the content map is the single source of truth and the demos are a point-in-time snapshot of it; the reduced-motion contract covers the shimmer, with a named check.
- D4: the badge sum excludes junk and trash roles, per-mailbox counts with no dedup (fixture cases for the exclusion and the partial sum); the Unified row hides when account_folders.length < 2; the row stays focusable with the tooltip on focus through aria-describedby, tested.
- D6: rows size with min-height and no fixed-height text rows, stated in the design-system reference and checked per surface in the audit.
- CAND-1 gains its injection contract: explicit injection API, deep-clone pushes, unlisten and cleanup matching the Tauri bridge, dispatch-order application, the alias only under --mode fixtures, and a build-level assertion that the harness never lands in the production bundle; harness.test.ts pins all four.
- The first-paint ordering (data-density on the root before first render) gets a cheap vitest.
- The five dogfood notes land in docs/dogfood/ (outside the doc_sync mirror scope).
- The eng voice's fifteen findings are all accepted as spec amendments; the security posture is clean (no new IPC, no wire changes, no remote loads; the font is a vendored resource).
<!-- /autoplan-accepted:eng -->
| 14 | ENG | Native eng voice (subagent-only; Codex not installed): 15 findings, all accepted as spec amendments | Mechanical | P5+P1 | Sequencing recorded with CAND-1 first; the virtualizer estimate, badge-sum rule, harness-injection contract, font-budget enforcement, webview verification, min-height invariant, and first-paint assertion are all pinned | leave the contracts implicit |

## Eng review output

### Reality rows (eng)

- REALITY: premise-table pass: every new claim was grounded by the eng voice against the tree (client/web/src/services/appearance.ts:46-94, client/web/src/styles/theme.css:130,337-350, client/web/src/services/__fixtures__/harness.ts:1-20, client/web/src/services/surfaces.ts:38-48, client/web/src/components/layout/MessageList.tsx:1-30, client/web/src/features/settings/GeneralDetail.tsx:1-40) unverified=0
- REALITY: already-done pass: no prior density axis, EmptyState, or injection work in git log (commits 3d5ec59, c4dc26b, 1b5c2d0)
- REALITY: surface-check pass: the only commands named are the documented gate, vitest, and dev:fixtures invocations (CLAUDE.md:35-52)
- REALITY: binding-decisions pass: decision 01bb418e honored; the slice changes no contract (docs/capabilities.md:1-10)
- REALITY: numbers pass: 920 tests re-measured by full vitest run (client/web/package.json:6-11); the 400KB font budget is a stated bound, not a measurement [estimated]
- REALITY: deferred-asks pass: known gaps list what the slice leaves out (docs/superpowers/specs/2026-10-13-ui1-evolved-iris-design.md:180-197)
- REALITY: pr-count pass: one PR per wave (docs/quality-workflow.md:54-59)

### Scope challenge and sections

Complexity gate: more than 8 planned files and 2 new components trip the gate; the override never reduces (P2), and the original arrangement stands (P5): one slice, tokens first per the recorded landing order. Scope accepted as-is.

- Section 1 (architecture): attribute-driven tokens, colocated contracts, no Rust or wire movement; the one cross-cutting integration (the virtualizer's estimate) is now named and pinned. Diagram: appearance.ts -> [data-density] -> theme.css scale -> components; MessageList's estimateSize now reads the same axis. 1 issue found (F2), accepted.
- Section 2 (code quality): reuse held at rung 1 (appearance.ts, ReadingPane's state pattern); the content map is the single source of truth (F1 resolved); no DRY violations found beyond it.
- Section 3 (tests): the diagram covers the axis switch, the Density control, EmptyState, AiChip's five states, the badge sum with role exclusion and the single-account hide, the injection API, the first-paint ordering, and the named visual checks; every row has a vitest or a named harness check; the Test Plan Artifact is on disk.
- Section 4 (performance): the subset budget is gate-enforced; font-display swap plus metric matching; the density switch costs one relayout with the large-list check named. No N+1 equivalents.

### Failure modes registry

| CODEPATH | FAILURE MODE | RESCUED? | TEST? | USER SEES? | LOGGED? |
|---|---|---|---|---|---|
| density axis | unknown persisted value | Y | Y | comfortable | n/a |
| density switch on 10k rows | stale virtualizer estimate | Y | named check | momentary re-measure | n/a |
| font loading | missing/corrupt resource | Y | visual pass | system face | n/a |
| metric overrides | unsupported webview | Y | named step | swap shift there, recorded | n/a |
| unified badge | partial/stale account data | Y | fixture case | sum of available | n/a |
| harness injection (dev) | malformed push or prod leak | Y | harness.test.ts + build assertion | test failure | console |

No CRITICAL GAP rows.

### Consensus (outside unavailable, subagent-only)

| Dimension | Claude | Codex | Consensus |
|---|---|---|---|
| 1. Architecture sound? | yes | n/a | N/A |
| 2. Test coverage sufficient? | yes (after F8/F9/F10) | n/a | N/A |
| 3. Performance risks addressed? | yes | n/a | N/A |
| 4. Security threats covered? | yes | n/a | N/A |
| 5. Error paths handled? | yes | n/a | N/A |
| 6. Deployment risk manageable? | yes | n/a | N/A |

### Completion summary (eng phase)

- Scope Challenge: scope accepted as-is; complexity gate tripped and settled on the original arrangement.
- Reality rows: 7/7 pass, 0 unverified.
- Architecture: 1 issue (virtualizer estimate), accepted. Code quality: 0 open. Tests: diagram plus artifact, 4 contract gaps closed (F8/F9/F10). Performance: 0 open.
- Outside voice: codex not installed, unavailable (subagent-only).
- Unresolved decisions: none open from this phase; two taste decisions (CAND-1, the Unified row treatment) wait at the final gate.
