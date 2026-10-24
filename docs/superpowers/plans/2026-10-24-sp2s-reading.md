# SP2s plan: the reading batch

Spec: docs/superpowers/specs/2026-10-24-sp2s-reading-design.md. Branch
`sp2s-reading` from main at the docs commit. Each task: reds first, the
implementation, the battery, a conventional commit.

### Task 1: the core (D4's report intent, D1's window feed)

**Files:** crates/astra-app/src (the ReportMessage intent onto the
engine's outbox-mediated verb with the capability gate and the verdict
mapping; the reading snapshot's fields the window pulls; the
not-spam return path); the protocol shapes; fakes' report legs.

- [ ] Tests first; commit `feat: feed the reading core`.

### Task 2: the host legs (D2's save and print, D1's window)

**Files:** client/desktop/src (the eml_save command over the engine
source read and the SP2p save seam; the print command or the
print-window approach you judge honest — document the choice);
the message window's capability entry beside the composer's.

- [ ] Tests first; commit `feat: land the reading host legs`.

### Task 3: the surfaces

**Files:** client/web/src (the MessageWindow root + the row menu's Open
in New Window; the overflow menu's Save as .eml and Print; the zoom
control and the srcdoc wrapper with the reflow clamp; the Junk and Not
spam verbs; the swipe handlers + the direction settings + the undo
toast); the reading and settings surfaces' tests.

- [ ] Tests first; commit `feat: surface the reading batch`.

### Task 4: the live proofs + the docs

**Files:** the gated live suite extensions (a .eml export matching the
received source; a junk report landed and observed on the real server;
the fixed-width reflow at two widths); docs/reading-window.md +
reading-zoom.md + reporting.md + zh copies (created);
docs/reading-actions.md extended + zh; the capabilities rows; the
fragment.

- [ ] Commit `feat: prove the reading batch live and record it`.

### Task 5: close-out

- [ ] Full battery; branch `sp2s-reading` pushed; PR (squash expected); ledger cleanup.

## Self-review notes

- Spec coverage: D4's intent + D1's feed (T1), D2 + D1's host halves
  (T2), every surface (T3), live + docs (T4).
- Interface chain: the core shapes (T1) -> the host legs (T2) -> the
  surfaces (T3) -> live + docs (T4).
- Risks: the window's two-readers-one-store mark-read (pin the
  single write); the srcdoc wrapper must not weaken the CSP or the
  remote-image gate; the report's idempotency key per intent; the
  swipe/scroll/drag axis conflicts (the touch pointer owns the
  horizontal, the virtualiser the vertical, the mouse the drag); the
  print document's sanitisation reuse (never a second implementation).
