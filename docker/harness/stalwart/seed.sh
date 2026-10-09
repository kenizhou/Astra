#!/bin/sh
# Seeder for the Astra Mail Stalwart harness: the custom folders, the three notes
# the gated list test reads, and the one rich note the gated reading test opens.
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
# Idempotent: every folder create tolerates ALREADYEXISTS and every note is seeded
# only when its mailbox does not already hold its subject, so a re-run converges on the
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

# Whether `mailbox` already holds a message with the subject: the idempotency check
# for the seeded mail. IMAP SEARCH SUBJECT matches substrings, and the subjects below
# are unique, so a found message is the note itself.
folder_has_subject() { # user  mailbox  subject
  curl -sk --user "$1:$PW" --url "$IMAPS/$2" \
    --request "SEARCH SUBJECT \"$3\"" 2>/dev/null | grep -q 'SEARCH [0-9]'
}

inbox_has_subject() { # user  subject
  folder_has_subject "$1" INBOX "$2"
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

# The rich note the gated reading test opens: subject, Date header, and the inline
# part's Content-ID, all pinned.
RICH_SUBJECT="Harness rich note"
RICH_DATE="Thu, 8 Oct 2026 09:15:00 +0000"
RICH_CID="rich-inline@example.test"

# A 70-byte 1x1 RGBA PNG as a fixed base64 literal: every chunk CRC and the inflated
# scanline were verified when the literal was pinned, so the inline image's bytes are
# the same on every run. Carried as a literal rather than generated, so the seed needs
# no tooling beyond printf, grep, sed, and tr.
PNG_B64="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=="

# The rich note itself: hand-built MIME, whole-message printable ASCII with CRLF line
# ends as RFC 5321 wants. A multipart/mixed holds a multipart/related (an html body
# referencing one remote image and one inline cid: image, plus the png part the cid
# resolves to) and one text/plain attachment.
rich_note() {
  printf 'From: Astra Two <%s>\r\nTo: %s\r\nSubject: %s\r\nDate: %s\r\n' \
    "$ASTRA2" "$ASTRA1" "$RICH_SUBJECT" "$RICH_DATE"
  printf 'MIME-Version: 1.0\r\n'
  printf 'Content-Type: multipart/mixed; boundary="astra-mix"\r\n\r\n'
  printf -- '--astra-mix\r\nContent-Type: multipart/related; boundary="astra-rel"\r\n\r\n'
  printf -- '--astra-rel\r\nContent-Type: text/html; charset="utf-8"\r\n\r\n'
  printf '<p>The rich harness note body.</p>\r\n'
  printf '<img src="https://astra-invalid.example/px.png" alt="remote pixel">\r\n'
  printf '<img src="cid:%s" alt="inline pixel">\r\n' "$RICH_CID"
  printf -- '--astra-rel\r\nContent-Type: image/png\r\nContent-ID: <%s>\r\n' "$RICH_CID"
  printf 'Content-Disposition: inline; filename="rich-inline.png"\r\n'
  printf 'Content-Transfer-Encoding: base64\r\n\r\n%s\r\n' "$PNG_B64"
  printf -- '--astra-rel--\r\n'
  printf -- '--astra-mix\r\nContent-Type: text/plain; charset="utf-8"\r\n'
  printf 'Content-Disposition: attachment; filename="notes.txt"\r\n'
  printf 'Content-Transfer-Encoding: 7bit\r\n\r\n'
  printf 'The attachment payload of the rich harness note.\r\n'
  printf -- '--astra-mix--\r\n'
}

# The INBOX's UIDs whose subject matches, one per line: the addresses a move names.
# `UID SEARCH` answers `* SEARCH 4 5`; the sed keeps the numbers, the tr splits them.
inbox_uids_for_subject() { # subject
  curl -sk --user "$ASTRA1:$PW" --url "$IMAPS/INBOX" \
    --request "UID SEARCH SUBJECT \"$1\"" 2>/dev/null \
    | sed -n 's/^\* SEARCH //p' | tr -d '\r' | tr ' ' '\n' | grep -v '^$'
}

# Files the rich note into Projects. Recorded route: SMTP submission into the INBOX
# (the same real delivery path the three notes take, so Stalwart runs the message
# through its delivery pipeline), then IMAP UID MOVE (RFC 6851) into Projects. The
# move empties the INBOX of the rich note entirely, so the INBOX's three-note
# whole-equality contract stays intact: the rich note lives in Projects and only there.
move_rich_note_to_projects() {
  for uid in $(inbox_uids_for_subject "$RICH_SUBJECT"); do
    curl -sk --user "$ASTRA1:$PW" --url "$IMAPS/INBOX" \
      --request "UID MOVE $uid Projects" >/dev/null 2>&1
  done
}

# Seeds the rich note unless Projects already holds it. A copy an interrupted run left
# in the INBOX is moved rather than resubmitted, so the note is never duplicated; both
# ends are verified once the move has run.
seed_rich_note() {
  if folder_has_subject "$ASTRA1" Projects "$RICH_SUBJECT"; then
    log "rich note already in Projects"
    return 0
  fi
  if [ -z "$(inbox_uids_for_subject "$RICH_SUBJECT")" ]; then
    log "submitting the rich note"
    if ! rich_note | curl -sk --user "$ASTRA2:$PW" --mail-from "$ASTRA2" \
        --mail-rcpt "$ASTRA1" --url "$SMTPS" -T - >/dev/null 2>&1; then
      log "FAILED to submit the rich note"
      exit 1
    fi
  else
    log "rich note was delivered but never moved; moving it"
  fi
  # The submission is acknowledged before the INBOX copy is queryable; poll briefly
  # rather than assume.
  i=0
  until [ -n "$(inbox_uids_for_subject "$RICH_SUBJECT")" ]; do
    i=$((i + 1))
    [ "$i" -gt 15 ] && {
      log "the rich note never appeared in the INBOX"
      exit 1
    }
    sleep 1
  done
  move_rich_note_to_projects
  if ! folder_has_subject "$ASTRA1" Projects "$RICH_SUBJECT"; then
    log "FAILED to file the rich note into Projects"
    exit 1
  fi
  if inbox_has_subject "$ASTRA1" "$RICH_SUBJECT"; then
    log "FAILED: the rich note is still in the INBOX"
    exit 1
  fi
}

log "seeding the rich note into Projects"
seed_rich_note
