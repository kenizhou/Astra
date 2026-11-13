# SP4d-1b design: the CLI key verbs and sidecar virtual tools

**Status.** Decided 2026-11-13. Slices branch from main at this commit.

**Scope.** The key-management desktop arc shipped (SP4d-1 import, SP4d-2
review/promote), but the agent-first CLI has no key verbs and the sidecar no key
tools. This slice adds five CLI verbs (keys list / collected / import / promote /
reject) plus the matching sidecar virtual permission tools, completing the
agent-facing key-management surface. PUBLIC material only: private-key import
stays desktop (the vault master key is OS-keyring-resident and the CLI contract
forbids opening the keyring; no safe passphrase transport exists). The CLI calls
the store directly (its lean precedent), replicating the app's promote/reject
orchestration and pinning both copies by tests. No engine change; no eval /
pipeline bump.

## D1: the five CLI verbs

A new cli/src/verbs/keys.rs (the knowledge.rs shape) opens crypto.sqlite lazily
(data_dir.join("crypto.sqlite"), the same dir as astra.sqlite; CryptoStore::open's
5s busy timeout shares the file with the host). The CLI gains the
astra-crypto-store path dependency (+ the testing dev-feature). Account resolution
reuses account_id (the AccountId newtype). The verbs:

- keys list --account (read): both standards merged, public view (no private
  material), a shaped subset (fingerprint, email, standard, key_type, origin,
  has_private, timestamps).
- keys collected --account (read): the staged rows, both standards, newest first.
- keys import --account (--file | --plain | stdin) [--standard] (mutating):
  import_public_cert / _as; the default unverified trust comes free; secret-bearing
  input is refused by the store with a pointer to the desktop private path.
- keys promote --account --standard --fingerprint [--email] [--verified]
  (mutating): replicate the app's email-fallback (pre-read before promote drains
  staging) + promote_collected_key + record_promote_trust. The trust write is
  load-bearing (promote alone leaves the resolver Undecided).
- keys reject --account --id --standard --fingerprint [--email] (mutating):
  remove_collected_key + a rejected decision with the peer-email fallback.

Each verb answers the full scaffolding: the Verb variant + name() + mutating()
(import/promote/reject only) + parser arm + SUBVERBS + the two execute arms
(early-return before the engine opens + the unreachable arm) + arg_preview
(shapes only, never caller text) + completions descriptor + PROBES. --json rides
the existing envelope (no schema bump); --preview ships for the three mutating
verbs (the action builder, nothing written). FK edge: import/promote for an
account the app never registered fails with a clear runtime error (the CLI cannot
self-register without the account email); list/collected/reject are unaffected
(no FK).

## D2: the sidecar virtual tools + the host gating

mailTools.ts is at the cap (487/500): a new sidecar/src/tools/keyTools.ts holds
the five tools, and the shared runGated / execCli / buildCliArgv / defineMailTool
extract to a tools/gated.ts (freeing mailTools.ts headroom). Each tool runGated ->
buildCliArgv(['keys', subverb], flags). The host default_policy gains five rows:
list/collected auto (read-only), import/promote/reject confirm (mutating,
needs-approval; none send-class, promote/reject change the send resolver's future
decisions which is exactly why they gate at confirm). shaped_arg_preview gains
five shape arms. The three pins move together: policy_tests.rs (the exact rows),
permissions_tests.rs (the preview shapes), the sidecar map test (the exact tool
names + verb/subverb). confirm_preview stays a recorded gap (ship confirm like the
six existing writes; the preview-on-card leg is its own cluster).

## D3: the contract

docs/cli.md gains the five verb rows + the gating map rows + the matrix counts
(the verb count and "ten tools" -> fifteen); the capabilities.md row moves; a
changelog fragment lands (Platforms: desktop, Bump: minor). The zh mirrors move
in the same commit.

## Known gaps recorded, not hidden

- Private-key import via CLI is blocked: the vault master key is OS-keyring-
  resident (vault.rs) and the CLI contract forbids opening the keyring
  (docs/cli.md); no safe passphrase transport precedent exists. Private import
  stays desktop; keys import fed a TSK refuses with a pointer to the desktop.
- confirm_preview (the preview-on-card leg) stays a recorded gap; this slice
  ships confirm.
- A CLI-level gated live proof is new ground (none exists today); the offline
  battery is this slice's proof, and the import/promote/reject mechanisms are
  already live-proven through the product surface in SP4d-1 / SP4d-2. A spawned
  CLI live proof is a follow-up.
- The token registry (cryptoki, openpgp-card) and the native security UI are
  heavier follow-ups.

## Test-first

Every behaviour lands with its red. CLI verb tests (the knowledge_tests pattern:
drive run() with a temp data dir + a seeded crypto.sqlite via register_account +
the test master key; assert the exit code, the envelope data, preview-writes-
nothing, the audit row per invocation, secret-bearing input refused, the
FK-unregistered error). Sidecar tool tests (the stub-cli pattern: a keys stub
copy, the exact argv grammar, the approval envelope level/args, approve/reject/
abort, the map pin). The three host/sidecar pins move together. The offline
battery is the proof.