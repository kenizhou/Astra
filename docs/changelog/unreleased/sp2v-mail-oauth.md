The IMAP step of account setup now asks the mail server what it
accepts before it asks you for anything. Where the server offers an
OAuth sign-in it can be discovered from the standards (its own
autoconfig's issuer, else the well-known documents of your email
domain), the step shows a "Sign in with your provider" button as the
primary action, naming the provider, with the password field demoted
behind a "Use a password instead" control (never removed); where the
server takes a token but only from applications its provider registered
in advance, an honest sentence says so beside the password; and where
it offers no OAuth, or never answers, the password form stands exactly
as before, never an error. Signing in registers this install with the
provider, stores the grant with no password anywhere, and connects both
the IMAP and the SMTP legs over a token resolved fresh for every dial.
An account whose sign-in expires shows the expired-sign-in row naming
the refused credential, with reconnecting the account as its remedy.

The whole leg is proven offline over scripted servers, and the live
harness now pins the real thing: the server's capability line on both
legs (IMAP and the submission port) read through the engine's own
probes under the product's certificate acceptance walk, and the
discovery's fail-soft when no issuer resolves. The full browser sign-in
against a real identity provider remains the recorded wait, exactly as
the JMAP leg's does.

Platforms: desktop
Bump: minor
