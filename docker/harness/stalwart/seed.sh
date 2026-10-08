#!/bin/sh
# Folder seeder for the Astra Mail Stalwart harness.
#
# Called by entrypoint.sh once the server is a full, bootstrapped server with the
# harness accounts present. Pure curl over the real wire protocol the engine's client
# will use: IMAP. No Rust provider client and no extra binary.
#
# Transport (Stalwart v0.16 default): IMAP is implicit TLS on 993; `-k` accepts the
# server's self-signed test certificate, which never touches a host trust store.
#
# Idempotent: every folder create tolerates ALREADYEXISTS, so a re-run converges on the
# same state.
set -eu

ASTRA1="astra1@example.test"
ASTRA2="astra2@example.test"
PW="${HARNESS_USER_PW:-astra-harness}"
IMAPS="imaps://127.0.0.1:993"

log() { printf '[seed] %s\n' "$1"; }

# curl over implicit-TLS IMAP for `user`, selecting `mailbox` and issuing `command`.
imap_cmd() { # user  mailbox  command
  curl -sk --user "$1:$PW" --url "$IMAPS/$2" --request "$3" >/dev/null 2>&1
}

log "waiting for IMAP to accept a login for $ASTRA1"
i=0
until imap_cmd "$ASTRA1" INBOX "NOOP"; do
  i=$((i + 1))
  [ "$i" -gt 60 ] && {
    log "IMAP never became ready"
    exit 1
  }
  sleep 1
done

# One custom folder per account, so the folder list a dial binds has a row that is
# neither a standard folder nor the INBOX. Standard folders (Sent, Drafts, Trash, Junk)
# are whatever the server provisions for a fresh account; this script does not force
# them.
log "creating the custom folders"
if ! imap_cmd "$ASTRA1" INBOX "CREATE Projects"; then
  log "Projects for $ASTRA1 already exists"
fi
if ! imap_cmd "$ASTRA2" INBOX "CREATE Archive2"; then
  log "Archive2 for $ASTRA2 already exists"
fi
