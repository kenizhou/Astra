A `mailto:` link now opens the composer prefilled. The OS hands the link
over on every road a mail app sees: a click with Astra Mail running, a
click that starts it, a click that reaches a second launch, and a
`mailto:` address inside a message you are reading. One parse serves them
all: the five fields RFC 6068 lets a client honour (To, Cc, Bcc, subject,
body) seed the composer; every other header is dropped, and a link that
names a sender is not trusted. The composition opens from your default
send account (the first account until you set one) in a pop-out window,
titled by its subject, and sends as plain mail exactly as a hand-typed
new message does. A `mailto:` address in the reading pane opens the
composer instead of the system mail app; every other kind of link still
goes to the browser.

Platforms: desktop
Bump: minor
