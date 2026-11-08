# The send gate and the compose transform (SP3b)

Every send passes the registered before_send handlers before it queues, and
every composition's rendered bodies pass composer.transform handlers before
that. A blocked send is never queued: it stands held in the sending snapshot
naming the plugin and reason, backed by the durable draft, with retry,
withdraw, and the prompt-user Send Anyway/Deny answer as outbox intents.
Modes are manifest-declared (block default, fail closed; soft_block fails
open; prompt_user asks); auto-activation costs the admin grant. Proven live
against the docker harness with a scripted veto, and by the echo plugin over
the real worker protocol.

Platforms: desktop
Bump: minor
