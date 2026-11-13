# SP4c-3b design: CRL fetching

**Status.** Decided 2026-11-13. Slices branch from main at this commit.

**Scope.** The second leg of SP4c-3: populate the CRL cache so the S/MIME
receive verify can check revocation. The cache (upsert_crl / get_crl /
list_crls / prune_stale_crls) and the chain's CRL consumption (AstraCrlChecker
hard-fails revoked, soft-fails stale/unchecked, verifies the CRL signature)
already ship; what is missing is the distribution-point extraction and the
fetcher. This closes the recorded revocation gap (crypto-receive.md). No
engine change; no eval / pipeline bump (the model-facing surface is
unchanged). The W9 password flow (SP4c-3c) follows as its own slice.

## D1: the distribution-point extraction

A helper in astra-crypto-smime/src/cert.rs beside cert_summary_of_der (the
SP4c-2 rule: CMS/cert DER parsing stays in the smime crate, never the store):
extract the CRL distribution-point URLs from a cert (x509-parser's
crl_distribution_points → GeneralName::URI). Extract from the signer leaf and
the intermediates. Only http(s) URLs are admissible (a DP URL is
attacker-influenced, it comes from the cert).

## D2: the fetcher port + the receive hook

A `CrlFetcher` port in astra-crypto-store (the SendCryptoPolicy precedent:
the store defines the port, the host implements; the store never depends on
astra-app). The host implementation (client/desktop) does a bounded HTTP GET
(reqwest, a short timeout, a response-size cap, http(s) only) and upserts the
DER. The store stays offline-testable with a fake fetcher.

The hook in receive_smime's verify path: before verify_with_context, extract
the DP URLs from the leaf + intermediates; for each, use the cached CRL if
fresh, else fetch (fail-soft) and upsert; then list_crls feeds the chain
validation. A fetch failure degrades to unchecked, NEVER fails the message
open (the chain already soft-fails). The fetch is bounded so it cannot stall
the reading pipeline; the chain validates as-of signing time, so a freshly
fetched CRL is still checked against the signing instant.

Security: the DP URL is untrusted input (http(s) only, timeout, size cap);
a forged CRL from a malicious URL fails the chain's CRL-signature check
(AstraCrlChecker verifies against the issuing CA). Privacy: fetching a CRL
reveals to the CA which certs are being validated (a tracking vector),
recorded as a known consideration.

## D3: the contract

docs/crypto-receive.md gains the CRL-fetching note (the DP extraction, the
fail-soft fetcher port, the revocation states now reachable, the privacy
consideration) and mirrors it to zh; the recorded "CRL cache empty, revocation
unchecked" gap is updated. The capabilities S/MIME row's note names the
revocation checking landing. A changelog fragment lands.

## Known gaps recorded, not hidden

- A fetch failure degrades to unchecked (fail-soft), never blocks the open.
- The CRL freshness window and the prune cadence are the cache's existing
  policy; a background prefetch is a follow-up (the fetch here is bounded and
  inline).
- The privacy consideration (CRL fetch reveals validation to the CA) is
  recorded; an ocsp/stapling alternative is a follow-up.
- The W9 password flow (SP4c-3c) is a separate slice.

## Test-first

Every behaviour lands with its red. The extraction tests (a cert with a DP
extension yields the URL; no DP yields nothing; a non-http(s) URL is
rejected). The fetcher tests (a fake fetcher: a fresh cache is used without a
fetch; a stale/missing CRL triggers a fetch + upsert; a fetch failure degrades
to unchecked, never fails). The receive hook tests (a revoked cert with a
served CRL → chain_valid=false, revocation_state=revoked; a fetch failure →
unchecked). ONE gated live harness proof: a hand-built CRL fixture (a new
builder in testing.rs; the CA builders already carry cRLSign) served by a
local HTTP test server; seed the CA chain, revoke the sender leaf serial,
serve the CRL, open the mail; assert revocation_state=revoked, chain_valid=
false, the revoked banner; the fetch-failure negative → unchecked; withdraw
copies. No live model proof; the offline battery is the proof.