# TODOS

Deferred work recorded by the /autoplan review of the UI1 Evolved Iris spec
(2026-10-13). Each item names its context and where to start.

## Serif reading column (direction B element)

- **What:** a Georgia reading column at 62ch with 1.72 leading for the reading pane.
- **Why:** direction B's strongest element; the user picked A on all surfaces but may cherry-pick later.
- **Pros:** the reading experience is the product's heart; the demo proved the pattern.
- **Cons:** a second typographic voice to maintain; conflicts with the bundled font decision if both land.
- **Context:** demo at `client/web/.gstack/designs/redesign/b-mail.html`; the spec's known gaps record it as a future pick.
- **Priority:** P3. **Depends on:** the UI1 slice landing (the bundled font).

## Command palette (direction C element)

- **What:** the command palette with its shortcut model; `--ease-spring` is reserved for it.
- **Why:** kylins' palette is an unscheduled addition the porting backlog names; C's demo proved the shape.
- **Pros:** keyboard-first flows; the shortcut model is already designed.
- **Cons:** a full surface with its own state and shortcut registry; large.
- **Context:** demo at `client/web/.gstack/designs/redesign/c-mail.html`; the ease-spring reservation is recorded in `theme.css` and the design-system reference.
- **Priority:** P3. **Depends on:** an owner pick (the backlog's kylins additions are unscheduled by design).

## DESIGN.md export

- **What:** a normative DESIGN.md (open format, front-matter tokens) exported from the implemented token system.
- **Why:** no DESIGN.md exists; the reviews calibrated against `theme.css` directly, which works but is not the normative artifact the skills read first.
- **Pros:** one normative design-system document; the detectors and reviewers calibrate faster.
- **Cons:** a second source of truth to keep in step with `theme.css`.
- **Context:** `/design-consultation` produces it; the UI1 design-system reference (`docs/design-system.md`) is its seed.
- **Priority:** P2. **Depends on:** the UI1 slice landing.

## Harness event injection beyond CAND-1

- **What:** richer injection (scripted sequences, multi-surface flows) if CAND-1's contract proves out.
- **Why:** CAND-1 covers single pushes; complex flows (reply prompt, agent approval) may want scripts.
- **Pros:** every modal and flow becomes auditable in the harness.
- **Cons:** scope growth in a dev tool; only build if the design-review loop asks for it.
- **Context:** `client/web/src/services/__fixtures__/harness.ts` and its injection contract in the UI1 spec.
- **Priority:** P3. **Depends on:** CAND-1.
