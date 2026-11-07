Each account now carries an outgoing name in the core: where the provider
already knows one (Gmail, JMAP, Microsoft) the account adopts it when it
connects, a name set on the accounts page is stored beside the account and
becomes the name every sent message carries on its From line, and where
the provider lets the account holder change the server's copy the change
is pushed back best-effort, with a refusal shown on the account's row
while the local name stands. The accounts page's per-account rows gain
the name, whether it is writable, read-only, or local to this app, and
whether it still needs asking once.

The vendor commerce family (the vendor account sign-in, the subscription
view, the next-device account list) is recorded as decided out: the
reference code is licence-barred from reuse in another product
(LicenseRef-Allodia-1.0 §4.2) and the service behind it is the vendor's
own, so those rows are marked N/A in the capabilities list rather than
left standing as unstarted. The identity round trip (the read of the
account's own identity, the write landing and reading back, the adoption
at the join) is proven live against the harness's JMAP.

Platforms: desktop
Bump: minor
