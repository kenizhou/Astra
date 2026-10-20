# SP2q plan: the composer finishing

Spec: docs/superpowers/specs/2026-10-17-sp2q-composer-design.md. Branch
`sp2q-composer` from main at the docs commit. Each task: reds first, the
implementation, the battery, a conventional commit.

### Task 1: the core and the wire (D1's wire half, D3's seeding, D4's
preference, D5's prefill shape)

**Files:** documentWire.ts's editable-run rule and marks (the three marks
both ways, web); crates/astra-app (the signature store: a per-account
assignment in settings/preferences, the seeding rule on open, the
untouched-swap rule on a From change; the default send account
preference; the account rows' sending address in the shell snapshot; the
mailto prefill shape over the core parser, additive on the wire).

- [ ] Tests first; commit `feat: seed the composer finishing in the core`.

### Task 2: the host leg (D5's entry)

**Files:** client/desktop/src (the mailto deep-link listener beside the
OAuth one, the single-instance argument forwarding, the compose-open
emission; the reading pane's mailto clicks routed internally, web).

- [ ] Tests first; commit `feat: open the composer from a mailto link`.

### Task 3: the surfaces (D1's toolbar, D2, D3's editor, D4's picker, D5's
paste)

**Files:** client/web/src/features/composer/ (the toolbar's three
controls; the link popover with edit, remove, and the typed-address
recogniser; the send-time degradation sentence; the From picker; the
paste handler); the settings feature (the signature editor with
per-account assignment; the default send account control).

- [ ] Tests first; commit `feat: finish the composer surfaces`.

### Task 4: the live proofs + the docs

**Files:** the gated live suite extensions (a formatted send round trips;
a signature-bearing send round trips; a link round trips with the refused
scheme degrading); docs/signatures.md + zh (created);
docs/composer-security.md extended (links, paste, mailto) + zh; the
capabilities rows; the fragment.

- [ ] Commit `feat: prove the composer finishing live and record it`.

### Task 5: close-out

- [ ] Full battery; branch `sp2q-composer` pushed; PR (squash expected); ledger cleanup.

## Self-review notes

- Spec coverage: the wire and store halves (T1), the entry (T2), the
  toolbar, popover, picker, editor, and paste (T3), live + docs (T4).
- Interface chain: the core shapes (T1) -> the entry (T2) -> the surfaces
  (T3) -> live + docs (T4).
- Risks: the editable-run widening must not unfreeze Table/Quote/Signature
  blocks (they stay retained); the untouched-swap rule needs a dirty flag
  on the seeded signature's span; the recogniser must never fire inside a
  link run; the default-account preference must not capture reply/forward
  requests; the mailto path must not regress the single-instance argv
  forwarding the OAuth flow rides.
