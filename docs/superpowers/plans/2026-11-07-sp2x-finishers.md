# SP2x plan: the surface finishers

Spec: docs/superpowers/specs/2026-11-07-sp2x-finishers-design.md. Branch
`sp2x-finishers` from main at the docs commit. Each task: reds first, the
implementation, the battery, a conventional commit.

### Task 1: the small core rows (D1's verb, D2's fetch, D3's timer)

**Files:** crates/astra-app/src (the delete_message verb over MailEdit::delete
through edit_row, the intent, the boot's per-account spawn joining the
calendar refresh body directly); client/desktop/src (the periodic timer beside
the delivery loop: the five-minute tick, the connected gate, the env override
with the limit); fakes.

- [ ] Tests first; commit `feat: feed the finisher core`.

### Task 2: the drafts reopen (D4)

**Files:** crates/astra-app/src/drafts (the by-key resume_draft beside the
latest-entry with the three gates; DraftResume gaining the carried-parts
field in the forward's own shape); client/desktop/src/commands/drafts.rs (the
by-key command); client/web/src (the two row-menu verbs over the command, the
pre-attached rows rendering in the forward's shape); the wire.

- [ ] Tests first; commit `feat: reopen a draft row into the composer`.

### Task 3: the resize and the showcase (D5, D6)

**Files:** client/web/src/features/composer (the rendered inline picture; the
resize overlay's pointer gesture writing width_px); client/desktop/src (the
debug-gated showcase flag: the command layer answering the recorded
snapshots, the screen picker env); the fixtures' frozen dates.

- [ ] Tests first; commit `feat: resize the picture and boot the showcase`.

### Task 4: the proofs + the docs

**Files:** the gated live proofs (the delete verb observed over the harness;
the boot fetch filling the calendar; the drafts reopen round trip); docs
(drafts.md, sending.md or composer-security.md for the resize, debugging.md
or a new showcase doc) + zh; the capabilities rows (the six); the fragment.

- [ ] Commit `feat: prove the finishers live and record it`.

### Task 5: close-out

- [ ] Full battery; branch `sp2x-finishers` pushed; PR (squash expected); ledger cleanup.

## Self-review notes

- Spec coverage: the verb + fetch + timer (T1), the reopen (T2), the resize
  + showcase (T3), proofs + docs (T4).
- Risks: the permanent delete is irreversible (the confirm's sentence must
  say so, and the verb never rides a batch loop); the boot fetch must not
  fire the calendar surface signal (call the body, not the intent); the
  periodic timer joins the process lifetime (the AppState drop-order trap);
  the by-key resume's gates must name ordinary mail (a row key can name it);
  the carried rows must never re-download the bytes (the part-id carry); the
  resize overlay must live outside the content (chrome never serializes into
  the message); the showcase gate must be debug-build-only and never touch
  the shipped alias.
