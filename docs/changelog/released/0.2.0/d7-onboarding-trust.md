# Onboarding names the trust decision (D7)

Platforms: desktop
Bump: minor

The account setup's recommendation step now leads with "We found your
settings" and names where the discovery came from. When a lookup crossed
an unencrypted hop, the step says so plainly, naming the hop and stating
that the mail servers themselves stay encrypted end to end, and the
approval is the primary button ("Approve and continue") instead of a
checkbox gating a disabled one. "Enter servers by hand" opens the manual form on bare
defaults, so untrusted settings never reach it that way. The manual form
assures the password goes to the system keychain, never to a log file.
The trust rules themselves are unchanged.
