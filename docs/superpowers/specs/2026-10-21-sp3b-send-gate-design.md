# SP3b design: the send gate and the compose transform

**Status.** Decided 2026-10-21. Slices branch from `sp3a-plugin-platform`
(PR #31) until it merges; this slice stacks on its contracts. Source: the
approved plugin plan's Track A phase 2, narrowed to the send path
(`message.decode` moves to SP3c with the reading banner it feeds).

**Scope.** Every send a submit starts passes the registered `before_send`
handlers before it reaches the engine's queue, and every composition's
rendered bodies pass the registered `composer.transform` handlers before
the gate. A send a handler blocks is never queued: it stands in the outbox
surface as a held send naming the plugin and the reason, backed by the
draft the autosave already persists. What does not ride: `message.decode`
and the reading-pane banner (SP3c), the plugin-to-host call direction (no
consumer yet), per-use send approval UX (the distribution slice's consent
dialog), and re-gating ops queued before a handler activated (the engine
owns the queue; recorded below).

## D1: the gate is a port, decided before the queue

The engine owns the queue and its submission scheduling; astra-app cannot
veto an op inside a drain. So the gate runs where the app still owns the
decision: in every submit arm, after the draft builds and before
`submit_mail` queues it. A new port, `BeforeSendGate`, takes the built
composition (account, recipients, subject, both bodies) and answers one
verdict: `Allow`, `PromptUser { plugin, reason }`, or `Block { plugin,
reason }`. The host (desktop) installs it when any activated plugin serves
`before_send`; absent the port, submits cost nothing. Ops queued before a
handler activated are not retro-gated: the engine owns their scheduling,
and the honest record of that limit lives in Known gaps.

## D2: held sends stand in the outbox surface, not the engine queue

A blocked (or timed-out, or transform-failed) send is never queued, so no
drain can ever leak it: there is no plaintext fallback by construction. The
send stands as a **held row** in `SendingSnapshot` (`held: Vec<HeldSend>`,
each naming the plugin, the reason, and the mode), and the composition
remains a saved draft, so nothing is lost on quit. A quit mid-gate queues
nothing; sending again re-runs every handler, which is the plan's
re-run-on-relaunch rule for the pre-queue gate. The held row offers retry
(the gate re-runs on the still-held composition) and withdraw (the held
entry drops; the draft stays in Drafts). `AwaitingPluginVerdict` is the
in-flight shape of the same row: visible from the moment the gate starts,
naming the plugin being awaited, resolving to held or gone on the verdict.

## D3: modes are declared, block is the default

The manifest gains `before_send_mode`: `block`, `soft_block`, or
`prompt_user` (required when `before_send` is declared). `block` cannot
send until the handler approves, and an unavailable handler blocks (the
default, matching kylins' no-plaintext-fallback). `soft_block` blocks on a
violation but fails open when the worker is unavailable. `prompt_user`
surfaces the handler's reason as a Send Anyway / Deny decision the user
answers through an intent; the answer is never remembered silently. Timeouts
follow Outlook's shape: the held row names the awaited plugin from the
first second (the ~5s note), and a ~5min cap fails a block-mode handler
closed and fails a soft-block handler open. One handler per plugin per
composition, and auto-activation costs the `admin` grant (the centralized-
deployment rule), checked by the desktop gate before it ever calls.

## D4: transforms fail closed, before the gate

`composer.transform` handlers (the `compose_edit` tier) run after the draft
builds and before the gate: the returned HTML and plain legs replace the
rendered ones. A transform that errors or is unavailable does **not** send
the untransformed mail (that would be the plaintext fallback kylins refuses
on purpose): the send holds with the transform's name as the reason, exactly
a blocked gate's row. Transforms run in manifest order; each sees the
previous one's output.

## D5: the desktop gate aggregates over the registry

`client/desktop/src/plugin/gate.rs` implements both ports over the
registry's activated workers: it calls each serving plugin's worker
(`before_send` / `composer.transform` as `object_call`s), applies the
per-plugin timeout and its declared mode, and folds the answers into the
single verdict the app asked for (block beats prompt beats allow; first
block wins the reason). A plugin that never answers inside the cap is
unavailable for that call. The approval answer rides a new command,
`plugin_gate_answer`, carrying the hold id and the user's choice.

## D6: the proofs

Unit reds pin every verdict path in astra-app (blocked never queues, held
row names the plugin, prompt answer resumes or drops, transform order and
fail-closed, no gate installed costs nothing). The desktop gate's tests
drive the real echo plugin in-process over duplex streams (the worker
host's custom spawn path, exactly what it exists for), with echo answering
`before_send` allow and echoing transforms. The live proof rides the docker
harness: a scripted veto gate in the gated live suite shows astra1's send
never reaching astra2 until the veto lifts. The stub-gate smoke the plan
asks for is the echo-driven desktop test plus this live veto.

## Known gaps recorded, not hidden

- Ops queued before a `before_send` handler activates are not retro-gated;
  per-op holds inside a drain need engine support the pinned revision does
  not have.
- Held rows are session state; the draft behind them is durable, and the
  held entry itself rebuilds from a resend.
- Plugin-initiated calls (a worker reading the AOM on its own) have no
  envelope yet; every call this slice needs is host-initiated, so the gap
  costs nothing today and lands when a consumer needs it.
- The 5s "taking longer" note is the held row itself naming the awaited
  plugin from the first second; a separate timed banner is polish, deferred.
- `message.decode` and its banner are SP3c; the UI descriptor model that
  renders plugin-sourced banners lands there with it.
