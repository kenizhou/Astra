#!/bin/sh
# Seeder for the Astra Mail Stalwart harness: the custom folders and the three notes
# the gated list test reads.
#
# Called by entrypoint.sh once the server is a full, bootstrapped server with the
# harness accounts present. Pure curl over the real wire protocols the engine's client
# will use: IMAP for the folders and the idempotency search, SMTP submission for the
# notes. No Rust provider client and no extra binary.
#
# Transport (Stalwart v0.16 default): IMAP is implicit TLS on 993 and SMTP submission
# is implicit TLS on 465; `-k` accepts the server's self-signed test certificate on
# both, which never touches a host trust store.
#
# Idempotent: every folder create tolerates ALREADYEXISTS and every note is submitted
# only when the INBOX does not already hold its subject, so a re-run converges on the
# same state.
set -eu

ASTRA1="astra1@example.test"
ASTRA2="astra2@example.test"
PW="${HARNESS_USER_PW:-astra-harness}"
IMAPS="imaps://127.0.0.1:993"
SMTPS="smtps://127.0.0.1:465"

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

# Whether the INBOX already holds a note with the subject: the idempotency check for
# the seeded mail. IMAP SEARCH SUBJECT matches substrings, and the subjects below are
# unique, so a found message is the note itself.
inbox_has_subject() { # user  subject
  curl -sk --user "$1:$PW" --url "$IMAPS/INBOX" \
    --request "SEARCH SUBJECT \"$2\"" 2>/dev/null | grep -q 'SEARCH [0-9]'
}

# Submits one note from astra2 to astra1 through the harness's own SMTP submission
# port. The message is genuinely delivered: Stalwart runs it through its delivery
# path into astra1's INBOX, the same way real mail arrives, which is what the gated
# list test then reads through the product's own sync. CRLF line endings throughout,
# as RFC 5321 wants.
send_note() { # subject  date  body
  printf 'From: Astra Two <%s>\r\nTo: %s\r\nSubject: %s\r\nDate: %s\r\n\r\n%s\r\n' \
    "$ASTRA2" "$ASTRA1" "$1" "$2" "$3" \
    | curl -sk --user "$ASTRA2:$PW" --mail-from "$ASTRA2" --mail-rcpt "$ASTRA1" \
        --url "$SMTPS" -T - >/dev/null 2>&1
}

# Delivers one note unless its subject is already in the INBOX.
seed_note() { # subject  date  body
  if inbox_has_subject "$ASTRA1" "$1"; then
    log "note '$1' already delivered"
    return 0
  fi
  if ! send_note "$1" "$2" "$3"; then
    log "FAILED to submit note '$1'"
    exit 1
  fi
  # A whole second after each delivery, so the delivery instants the message list
  # orders by are distinct: the Date headers pin the content, the spacing pins the
  # order.
  sleep 1
}

# Three pinned notes: distinct subjects and Date headers (delivered oldest first), all
# inside the three-month sync window the app's first pass reads. The gated list test
# asserts exactly these subjects.
log "seeding the harness notes"
seed_note "Harness note one" \
  "Mon, 5 Oct 2026 09:00:00 +0000" \
  "First note delivered through the harness submission port."
seed_note "Harness note two" \
  "Tue, 6 Oct 2026 09:05:00 +0000" \
  "Second note delivered through the harness submission port."
seed_note "Harness note three" \
  "Wed, 7 Oct 2026 09:10:00 +0000" \
  "Third note delivered through the harness submission port."
