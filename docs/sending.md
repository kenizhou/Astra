# Sending: the Astra Mail contract

**Scope.** What happens from the moment Send is pressed: the durable outbox, the
rule that a message is never sent twice, what an unconfirmed delivery means, the
actions a queued send offers, the sender's own copy, and the reply and forward
chain that answers mail with mail. Binding on every surface that ships a composer
or an outbox. Ported in substance from mail-calendar's `sending.md`, scoped to
what Astra Mail ships today; the gaps are recorded at the end, never left silent.

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
  account to send from, no resolvable recipient, a document that will not
  render, or an attachment whose bytes the host did not resolve. A message
  silently missing its files is worse than one not sent. A mail-only account
  is the honest exception: the submit check asks only whether the account has
  a provider, so its send is queued, the first drain settles it Failed, and
  it stands in the outbox with its payload.

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
  user made themselves (Cancel, Mark as sent) is not read as a delivery, while
  a refused removal gives its mark back: the host's refusal at once, the
  engine's, because the send went in flight under the click, on the snapshot
  that follows, so the send's later genuine delivery still counts.
- **The retry of a standing unfiled-copy question is not shipped** and is recorded
  as a gap below.

## Replying and forwarding

Answering mail reuses the whole contract above: the reading pane's Reply, Reply
all, or Forward dispatches `Intent::Compose`, the core reads the original's
headers through the engine and publishes one prefilled composition on the
`Surface::ComposeRequest` snapshot, and the composer's Send submits it as
`Intent::SubmitRichReply` or `Intent::SubmitRichForward` through the same durable
outbox a new mail rides. Nothing about the queue, the never-send-twice
discipline, or the sender's copy differs because the message answers another.

**The prefill is computed once, core-side.** The snapshot carries the whole
composition (kind, account, recipients, subject, threading header, quoted
document), so every surface opens the same one and a host's own re-derivation
can never disagree with the send path:

- **Recipients.** A reply's `To` is the original's `Reply-To` when it has any,
  else its `From`, included even when that is the user's own address (a reply to
  your own Sent copy still has a recipient). Beyond that primary, the account's
  own identity is excluded from everything a prefill derives. A reply all
  preserves the original's layout: its other `To` recipients join `To`, its `Cc`
  stays `Cc`, duplicates folded case-insensitively in first-seen order. A plain
  reply leaves `Cc` to the user. A forward names no one: its recipients are the
  user's to choose. `Bcc` is never derived; it is always the user's own addition.
- **Subject.** `Re:` for replies, `Fwd:` for forwards, prefixed once and never
  doubled when the original already carries its own. The prefix is not
  localised: a translated one groups nothing.
- **The quote.** The original's body as plain text (its plain part when there is
  one, else the sanitised HTML read as text) under an attribution: the one-line
  `On <date>, <sender> wrote:` a reply shows, and the labelled
  `From`/`Sent`/`To`/`Cc`/`Subject` header block a forward's style shows, both
  carried so a surface can flip the style without a rebuild. Rich nested-HTML
  quoting is the rich-paste slice's, recorded below.

**Threading is resolved at submit, not carried from the snapshot.** A reply's
`In-Reply-To` is the original's own `Message-ID`, the header the snapshot
prefilled and a saved draft resumes with; its `References` chain is the
original's own chain followed by the original itself, re-resolved from the store
at the moment of the send, so the reply joins the conversation it answers
whatever the composer held. A forward carries the same chain with no
`In-Reply-To`, because a forward continues a conversation without answering a
message. A reply whose original cannot be re-resolved still sends, threading by
its header alone (the parent itself the chain, never another conversation's);
a forward cannot, because the original is the only source of the conversation it
belongs to, and a forward without it is a new message pretending to be one: it
sends nothing and queues nothing.

**The fields the wire can carry.** The reply submit carries `To` and `Cc` and no
`Bcc`; the forward submit carries `To` alone. The composer scopes its fields to
the same shapes: a reply offers Cc but no Bcc, a forward offers neither, and a
plain composition (new mail, a resumed draft) keeps every field. Nothing is left
editable that would silently fail to cross the wire.

**The composition is session state.** The request slot, and the original it was
computed from, live in the running core. A reply saved as a draft and resumed
after an app restart opens as a plain composition and sends as plain mail; the
resume reopens the saved words as plain text, so the quoted original comes back
flattened, without its quote block. Within one session the threading header
survives the composer's own autosaves until the send spends it. The same session
scoping has one recorded nuance: a forward submitted on the raw wire without a
fresh `Compose` dispatch in the same session threads under the source of
whatever request last published; the shipped UI cannot reach that state, because
every forward it sends follows its own `Compose` dispatch.

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
| Desktop | Composer Send button; outbox panel over the status bar with the state word, subject, and arrival instant per row; the actions the table above lists; the send-result toast; the reading pane's Reply, Reply all, and Forward, hydrating the composer from the published request |

The live proofs: the gated harness suite submits a pinned message from astra1 to
astra2 through the engine's real send path, reads it back in the recipient's INBOX
through the product's own sync and reading pipeline, finds the sender's copy in
their Sent folder, and withdraws both copies so the harness keeps its seeded
shape. Two further suites walk the answer chain over the same wire: one replies
to a freshly delivered note and pins the delivery's `In-Reply-To` against the
original's own `Message-ID` (its `References` chain holding the original), the
other forwards under the `Fwd:` subject and pins the chain with no
`In-Reply-To`; both empty the queue and withdraw every copy, leaving astra1's
INBOX the three seeded notes and astra2 empty.

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
- **A resumed reply degrades to plain mail.** The compose request is session
  state and the draft save carries no threading, so a reply resumed after an app
  restart sends as a new message, its quote flattened back in as plain words.
  Within the composition's own session the threading header rides every
  autosave until the send spends it.
- **An in-session raw-wire forward without a fresh Compose threads under a stale
  source.** The original a forward re-resolves is the one the last published
  request was computed from, cleared only by a restart or a fresh `Compose`;
  the shipped UI always pairs the two, so no surface reaches it.
- **A refused forward loses the typed words.** When the original has moved or
  been deleted between opening and sending, the submit refuses (nothing sent,
  nothing queued) and the composer closes silently, so the typed words are
  lost; making the refusal observable so the pane keeps the words is the
  recorded follow-up.
- **The quote is invisible in the trimmed editor.** The editor holds the quoted
  original as retained content below the typed words, round-tripping
  byte-identical through every save and send, but does not render it; rich quote
  rendering arrives with the rich-paste slice.
- **The quote style is a constant.** Every quote opens in the indented style;
  the account preference and the per-message override arrive with the settings
  slice.
- **A forward does not carry the original's attachments.** The quote is the
  body's text alone; re-sending received files needs the attachments staging
  flow, so a forward's document opens with nothing attached.
