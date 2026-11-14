S/MIME receive now checks certificate revocation. Opening an encrypted
and signed S/MIME mail reads the CRL distribution points from the
signer's certificate chain and fetches the lists over a bounded HTTP
request (a short timeout, a size cap, http(s) only, no redirects). A
list that revokes the signer fails the chain and shows the revoked
banner; the words still render. A failed fetch or a missing list
degrades to unchecked and never blocks the open. Fetching a CRL reveals
to the CA which certificates are being validated; a privacy-preserving
alternative is a follow-up.

Platforms: desktop
Bump: minor