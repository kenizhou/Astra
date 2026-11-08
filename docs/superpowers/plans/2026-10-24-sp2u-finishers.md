# SP2u plan: the calendar and contacts finishers

Spec: docs/superpowers/specs/2026-10-24-sp2u-finishers-design.md. Branch
`sp2u-finishers` from main at the docs commit. Each task: reds first,
the implementation, the battery, a conventional commit.

### Task 1: the core (D1's participants crossing, D2's rule crossing +
formatter, D4's shape preference)

**Files:** crates/astra-app/src (the detail read gains the
participants and the full rule; the app-side rule formatter with its
honest fallback; the calendar shape preference: view kind + anchor,
additive, written by the navigation, read at the boot seed);
astra-viewmodel (the detail's wire shapes); the protocol; fakes.

- [ ] Tests first; commit `feat: feed the finisher core`.

### Task 2: the surfaces

**Files:** client/web/src (the dock's attendee list + the editor's
read-only copy; the rule sentence replacing the minimal line; the
banner geometry in rowLayout/TimeGrid + the month's banner-aware
overflow; the shape restore in calendarStore + the navigation write;
the Avatar column on MessageRow with its responsive rule).

- [ ] Tests first; commit `feat: surface the finishers`.

### Task 3: the live proofs + the docs

**Files:** the gated live suite extensions (a seeded event with
attendees and a rule rendering both over the harness; a seeded
contact's photo standing on the sender's row); docs/avatars.md + zh
(created); docs/calendar.md + contacts.md extended + zh; the
capabilities rows (74, 77, 82, 85, 106) final; the fragment.

- [ ] Commit `feat: prove the finishers live and record it`.

### Task 4: close-out

- [ ] Full battery; branch `sp2u-finishers` pushed; PR (squash expected); ledger cleanup.

## Self-review notes

- Spec coverage: the crossings + formatter + preference (T1), every
  surface (T2), live + docs (T3).
- Interface chain: the core shapes (T1) -> the surfaces (T2) -> live +
  docs (T3).
- Risks: the participants' wire size (cap the list honestly; a meeting
  of hundreds folds with a count); the rule formatter must never word
  a rule it does not fully understand (the fallback pinned); the
  banner geometry against the lane's existing overlap packing; the
  shape preference's anchor staleness (a date from a past launch
  restores exactly, the honest shape); the avatar column's cost on
  huge lists (the existing cache + the responsive hiding).
