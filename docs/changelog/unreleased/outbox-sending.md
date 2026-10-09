The composer has a Send button. Pressing it queues the message in a durable
outbox before any server is asked, so a send the app does not survive is neither
lost nor delivered twice, and delivers it at once when the account can take it.
The sender's copy is filed in their Sent folder, and the words you typed are
taken out of the draft they were autosaved into.

Sends that cannot go out yet are kept, not failed. The outbox panel, over the
status bar, lists every account's waiting sends with what became of each, and a
quiet toast says when one goes. A waiting send can be sent now or withdrawn; a
send the server refused stays until you send it again or withdraw it; a send
whose delivery could not be confirmed offers only your answer: mark it as sent,
or send it again.

Platforms: desktop
Bump: minor
