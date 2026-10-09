# Sending: the Astra Mail contract

**Scope.** What happens from the moment Send is pressed: the durable outbox, the
rule that a message is never sent twice, what an unconfirmed delivery means, the
actions a queued send offers, and the sender's own copy. Binding on every surface
that ships a composer or an outbox. Ported in substance from mail-calendar's
`sending.md`, scoped to what Astra Mail ships today; the gaps are recorded at the
end, never left silent.

**Principle.** *Delivering a message and keeping the sender's copy of it are two
different operations, and a client must never let the second one fail in silence.*
A Sent copy is how a person checks that a message really left; losing one without
saying so is worse than most failures that do interrupt, because nothing later
recovers it.

## Never send twice

The one irreversible act in a mail client is putting a message in front of its
recipients a second time. Every rule below bends to it.

- **The send is queued before any server is asked.** `Intent::SubmitRichMail`
  renders the composer's document through the composer crate's deterministic
  renderer, mints a fresh `Message-ID`, and hands the message to the engine's
  `submit_mail`, which records the op (idempotent by that `Message-ID`) before the
  provider is touched. Whatever ends the process at any point after, the message is
  neither lost nor delivered twice.
- **The engine records its hand-over at the point of no return**: the last byte
  that could deliver the message. Everything before it (connecting, signing in,
  transmitting headers) is safe to repeat; a send cut off before it is retried, and
  one cut off after it may already be with its recipients and awaits the user's
  answer instead.
- **A send that may have been delivered is never sent again unasked.** Reconnecting,
  Send now, and Cancel are all refused on it. Its copy appearing in a Sent folder
  resolves it; otherwise the user does, with **Mark as sent** or **Send again**.
- **The store refuses an answer on a send that is not waiting for one**, so a row
  that changed state under the click cannot send twice: the confirmation pair is
  answered exactly once, by the row that asked.
- **A submit that cannot build a message sends nothing and queues nothing**: no
  account to send from, no provider with a submission leg, no resolvable recipient,
  a document that will not render, or an attachment whose bytes the host did not
  resolve. A message silently missing its files is worse than one not sent.

## The durable queue

**A message the app accepted is the user's, and it does not evaporate because a
server was not answering.** A send whose failure is worth retrying stays in the
outbox, a durable queue the engine owns, and goes out when it can.

- **Queued is not failed.** Telling someone a queued send failed invites them to
  write the message a second time, and then the first one arrives too. The four
  states a queued send reads as are `Queued`, `Sending`, `Unconfirmed`, and
  `Failed`, and only `Failed` means nothing will retry it.
- **Which failures queue is the engine's decision, not a surface's**: a retryable
  class parks the op and everything else settles it. The app asks the queue whether
  the message is still there rather than re-reading the error, so there is one
  rule, in the one place that owns it.
- **A queued send is on screen the moment it queues, offline included.** The queue
  lives in the store, not on a server, so being offline is no reason not to show
  it, and offline is the ordinary reason a send queues in the first place. Every
  account's sends publish through one `Surface::Sending` snapshot, on every
  transition of the queue: a send joining it, leaving it, or changing state.
- **The queue drains on gestures, not on a timer**: the end of every sync pass,
  every submit, and the user pressing Send now. A timer would wake a dead network
  on a battery. The fourth signal, the device coming back online, arrives with the
  reconnect work and is recorded as a gap below.
- **Only sends appear in the queue.** A queued draft save or keyword change is a
  write on a message that already lives in a folder, and the folder is where its
  owner will look for it. Those drain in the background and are never shown as
  unsent mail.
- **A refused send is kept.** A refusal no retry fixes settles the send as not
  sent, and the queue keeps it, payload and all, until the user sends it again or
  withdraws it. Reconnecting never sends it again: it hurries what is waiting, not
  what was refused.
- **Every step leaves a log line** (counts and ids only: a message is the user's,
  and nothing logs its content, its recipients, or its subject).

### What a queued send offers

| State | The panel says | Offers |
|---|---|---|
| Queued | Queued | Send now · Cancel |
| Sending | Sending | nothing: it cannot be called back |
| Unconfirmed | Unconfirmed | Mark as sent · Send again |
| Failed | Failed | Send again · Cancel |

| Action | Intent | What it does | Refused when |
|---|---|---|---|
| **Send now**, **Send again** on a refused send | `SendNow` | Clears the backoff, or puts a refused send back in the queue, and drains in the same gesture | In flight, or awaiting confirmation |
| **Cancel** | `Cancel` | Withdraws it so it is never delivered | Same |
| **Mark as sent** | `ConfirmSent` | Settles it as delivered; it leaves the queue | On a send not awaiting confirmation |
| **Send again** on an unconfirmed send | `ConfirmNotSent` | Puts it back in the queue and drains at once: the one way a message that may have been sent is ever attempted again | Same |

**An unconfirmed row offers only the user's answer.** The message may already be
in front of its recipients, so no surface offers Send now or Cancel on it, and its
Send again sends `ConfirmNotSent`, never `SendNow`. The two Send again buttons read
the same because to the user they are the same request; the intent is what keeps
the unconfirmed one from going out on any other path. Unconfirmed is a warning,
never a failure and never a wait: "couldn't send" invites a second copy, and
"waiting to send" promises an attempt that will not happen on its own.

**Once Send is pressed the outbox is the message's one place.** The submit carries
the composer's current document, not its stored draft, and a successful send takes
the composition's autosaved draft away with it, so two copies of one message never
stand to be sent. A send that never reached the queue leaves the draft, because
nothing else holds the words.

## The sender's copy

On IMAP/SMTP, delivering and filing really are two round trips: SMTP dials fresh
per send, and the copy is filed by a separate `APPEND` into the account's Sent
folder. The engine files the copy after delivery and retries the filing on a fresh
connection, made idempotent by searching Sent for the message's own `Message-ID`
first.

- **An unfiled copy never fails the send.** The mail has reached its recipients
  and must never be re-sent; the outcome says the copy is missing instead, and the
  app logs it as a warning rather than swallowing it.
- **The outcome is never worded as a failed send.** A send that delivered and filed
  ends quietly; the send-result toast watches the queue and says "Message sent"
  when a send leaves it without the user's own hand removing it. A removal the
  user made themselves (Cancel, Mark as sent) is not read as a delivery, and a
  removal the host refused is put back, so the send's later genuine delivery still
  counts.
- **The retry of a standing unfiled-copy question is not shipped** and is recorded
  as a gap below.

## Rules

1. **Never word an unconfirmed or unfiled send as a failure.** The one action that
   causes real harm here is sending a message twice, and every word choice is
   measured against inviting it.
2. **The engine owns both edges of every queue decision**: what is due, whether a
   failure parks or settles, and whether an op can be withdrawn or answered at
   all. A surface mirrors what the snapshot holds and never re-derives a queue
   rule; a refusal is log words, not a silent no-op.
3. **Actions name their row, never its position**: every outbox action carries the
   queued send's account and op id, so a row that moved under the click cannot act
   on its old neighbour.
4. **Send is disabled while a submit is in flight and while no recipient is set**,
   so one gesture cannot queue two sends, and the pre-queue refusals publish
   nothing a surface would have to undo.
5. **A send's failure surface keeps the words.** A typed error keeps the composer
   open with its document intact; the queue is the record of what was attempted,
   never the place the words are recovered from.

## What ships today

| Surface | Ships |
|---|---|
| Desktop | Composer Send button; outbox panel over the status bar with the state word, subject, and arrival instant per row; the actions the table above lists; the send-result toast |

The live proof: the gated harness suite submits a pinned message from astra1 to
astra2 through the engine's real send path, reads it back in the recipient's INBOX
through the product's own sync and reading pipeline, finds the sender's copy in
their Sent folder, and withdraws both copies so the harness keeps its seeded
shape.

## Known gaps

- **Edit is not shipped.** Moving a queued send back into Drafts needs the
  staging-directory flow of the attachments slice, so a refused send is sent again
  or withdrawn, not edited. The intent family is recorded as the deferral.
- **Send again on an unconfirmed send does not ask first.** Mail-calendar's
  contract puts a confirming question in front of the one action whose mistake
  lands in other people's inboxes; Astra Mail's panel sends `ConfirmNotSent`
  directly. The question lands with the dialog pass.
- **The unfiled-copy question has no surface.** A delivered send whose Sent copy
  could not be filed is logged, and the toast cannot distinguish it from a clean
  send, so the standing question and its repair are absent until the corresponding
  surface ships.
- **No reachability-driven drain.** The queue drains on sync passes, submits, and
  Send now; the device coming back online neither drains the queue nor clears each
  op's backoff, because no reconnect signal is wired yet.
- **A send interrupted by app close is proven by the engine's contract and the
  unit fakes, not by the live harness.** Killing a process mid-send inside a test
  is a harness ability that does not exist yet; the recovery path
  (`recover_outbox` at start-up) runs on every real boot.
- **The sender's name is the account's address.** No per-account outgoing name is
  asked or stored yet, so mail goes out as the bare address the account dials
  with; the From picker and its default arrive with their slice.
- **Reply and forward do not prefill.** This contract covers new mail composed in
  the composer; quoting and sender prefill arrive with the reply slice.
