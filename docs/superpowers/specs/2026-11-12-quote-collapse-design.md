# Quote collapse design: folding the quoted original

**Status.** Decided 2026-11-12. Slices branch from main at this commit.

**Scope.** The next kylins addition: collapse the quoted original in the
reading pane, so a reply or forward opens on the new words and the quoted
tail folds behind a "Show quoted text" toggle. It ports kylins's
quote-collapse shape (a pure post-sanitisation string transform emitting a
native details/summary); an addition, not mail-calendar parity debt. It
lives entirely on the web reading path: no sanitiser change, no wire type,
no intent, no model-facing change, so no eval and no pipeline bump.

## D1: the transform

A new pure module `client/web/src/components/reading-pane/collapseQuoted.ts`
(kylins's is thirty lines): `collapseQuoted(html)` returns the html with the
trailing quoted chain folded into
`<details class="astra-quote"><summary>Show quoted text</summary>...`
(no `open` attribute, so the default state is collapsed; the toggle is the
browser's native details/summary, which needs no script).

Detection, mirroring kylins and widened for Gmail:

- A trailing chain of one or more `<blockquote>...</blockquote>` blocks
  anchored at the very end of the html (case-insensitive, non-greedy).
- The immediately preceding attribution line, a trailing `<p>` or `<div>`
  holding `On ... wrote:`, folded in with the chain.
- A trailing `.gmail_quote` block as an alternative quote shape (the class
  survives Astra's sanitiser, and Gmail forwards are common).

Two safety no-ops, kylins's own: no trailing quote means the html rides
unchanged; a body that is entirely one quote is never collapsed (there is
nothing new to read). The injected markup is a fixed constant string: no
sender-derived attribute value enters it, and the fragment is wrapped, never
reparsed or re-encoded (the hostile-verbatim contract holds).

## D2: the wiring

The transform runs in `MessageHtml` at the composition point, before
`wrapBodyDocument`, on the gated html (the remote-image placeholders intact).
`enableRemoteImages` then runs on opt-in as today; the fold only wraps, so
the `data-astra-src` and `data-astra-style-url` tokens are untouched and the
existing rewrite still finds them inside the details. The blocked-image
count affordance is unaffected.

It is deliberately NOT inside `wrapBodyDocument`: `PrintWindow` calls that
wrapper directly and must keep the full body (kylins prints the full body
too). Both the reading pane and the pop-out window get the fold for free
through `ReadingView`/`MessageHtml`.

The frame stays scriptless by contract (`sandbox="allow-same-origin"`,
`default-src 'none'`); details/summary is the only interactive collapse that
works there, and the CSP and sandbox constants do not move. The Rust
sanitiser (ammonia) needs no change: the fold is app-owned markup injected
after sanitisation. (For the record: ammonia allows details/summary and the
class attribute, but not `input` nor the `open` attribute, so a checkbox
hack or a default-expanded fold through the Rust pass would not work; this
slice relies on neither.)

## D3: the contract

`docs/reading-zoom.md` is the wrapper contract doc where body transforms are
decided; the slice adds a short quote-collapse section there (the detection,
the default-collapsed fold, the print exemption, the verbatim guarantee) and
mirrors it to zh. A new Mail-area capabilities row lands beside the reading
rows, with its zh mirror. The porting-backlog "quote collapse" entry resolves
with the slice. A changelog fragment lands.

## Known gaps recorded, not hidden

- One fold for the whole trailing chain (kylins's own shape); per-quote
  collapse of multiple separate blocks, and mid-body quotes, are out.
- Yahoo/Outlook marker lines and non-English attributions are out; the
  detection is blockquote, the English "On ... wrote:" line, and gmail_quote.
- No persisted expanded state: the details state lives in the frame document
  and resets with the message, like the remote-image opt-in.
- Print keeps the full body (the fold is reading-only).
- No settings preference to disable the fold yet.
- A pre-existing dangling reference: capabilities.md cites
  `rendering-security.md`, which was never created; the security posture
  lives in the sanitise.rs module doc, the MessageHtml header, and
  reading-zoom.md. Noted here, not fixed by this slice (a doc-hygiene pass).

## Test-first

Every behaviour lands with its red, over the pure-transform pin pattern
(MessageHtml.test.tsx pins wrapBodyDocument and enableRemoteImages byte for
byte). The reds: a trailing blockquote chain is folded into a collapsed
details; the attribution line folds inside; a gmail_quote tail folds; a
mid-body blockquote and a quote-free body ride unchanged; a whole-body quote
is never collapsed; the remote-image placeholders survive the fold and the
opt-in rewrite still restores srcs inside the details; the hostile fragment
rides verbatim (no re-encoding). A MessageHtml component test pins that the
srcdoc carries the details and that print (wrapBodyDocument alone) does not.
No live model proof (pure UI); the offline battery is the proof.