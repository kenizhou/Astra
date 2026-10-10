A rare timing window could let the agent panel miss the list of models the
agent sidecar reported at startup: the relay marked the sidecar ready
before delivering the ready notice, so a listener reacting to the ready
state could read the notice too early. The notice now lands before the
state flips, so anything that waits for readiness always sees what the
handshake reported.

Platforms: desktop
Bump: none
