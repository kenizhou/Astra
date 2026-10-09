#!/usr/bin/env bash
# Lifecycle for the Astra Mail local test harness (docker/harness): a seeded Stalwart
# mail server the gated live tests dial. A thin, human-runnable wrapper over
# `docker compose` so the harness is one command away.
#
#   scripts/dev/harness.sh up       # start the stack; blocks until healthy
#   scripts/dev/harness.sh down     # stop and remove the containers
#   scripts/dev/harness.sh reset    # wipe the volumes, reseed from empty
#   scripts/dev/harness.sh env      # print the exports the gated tests read
#
# TLS ROUTE (the recorded decision, also in docker/harness/compose.yaml): the harness
# serves IMAP over implicit TLS on 127.0.0.1:2143 with a self-signed certificate, and
# the gated live test pins a CertificateException for it through the product's own
# acceptance path. A no-TLS loopback IMAP port was considered and not taken: Astra's
# account layer dials TLS only (ConnectionSecurity is implicit TLS or STARTTLS, and
# ImapConnections is typed over a TLS stream), so a plaintext port is unreachable
# through the connect path the test must exercise; the mail-calendar harness this ports
# from publishes no plaintext IMAP port either.
#
# The gated live test skips unless ASTRA_LIVE_HARNESS=1, so an offline `cargo test`
# stays green. Against a running harness (whose seed also delivers three pinned notes
# to astra1's INBOX through the harness's own SMTP submission port, the mail the
# gated list test reads):
#
#   scripts/dev/harness.sh up
#   eval "$(scripts/dev/harness.sh env)"
#   cargo test -p astra-app tests_live_harness -- --nocapture
#
# JMAP ROUTE (the recorded decision, also in docker/harness/compose.yaml): the
# harness also publishes Stalwart's built-in HTTP listener (JMAP + management) on
# 127.0.0.1:2080 as plain HTTP. The gated JMAP-detection test cannot resolve
# https://example.test, so it rebases the well-known probe onto that origin through
# the detection crate's dev-harness `well_known_base_override`, which also waives
# the probe's HTTPS requirement for the one hop; no host trust store is touched.
#
# POSIX-safe constructs only (no arrays, no `[[ ]]`): the script runs under any sh,
# with bash 5 the floor per AGENTS.md.

set -eu

HARNESS_ROOT="$(cd "$(dirname "$0")/../.." && pwd)/docker/harness"

die() { printf 'harness: %s\n' "$1" >&2; exit 1; }
info() { printf 'harness: %s\n' "$1"; }

# Docker is the one external thing the harness needs; fail with a clear message rather
# than a stack trace from deeper in compose.
require_docker() {
  if ! command -v docker >/dev/null 2>&1; then
    cat >&2 <<'EOF'
harness: Docker is required to run the mail harness, and was not found on PATH.
harness: install Docker Desktop, start it, then re-run:
harness:   scripts/dev/harness.sh up
EOF
    exit 1
  fi
  if ! docker info >/dev/null 2>&1; then
    cat >&2 <<'EOF'
harness: Docker is installed but its daemon is not running.
harness: start Docker Desktop, then re-run:
harness:   scripts/dev/harness.sh up
EOF
    exit 1
  fi
}

# The exports the gated live tests read. Feed them to a shell with:
#   eval "$(scripts/dev/harness.sh env)"
show_env() {
  printf 'export ASTRA_LIVE_HARNESS=1\n'
  printf 'export ASTRA_HARNESS_IMAP_HOST=127.0.0.1\n'
  printf 'export ASTRA_HARNESS_IMAP_PORT=2143\n'
  printf 'export ASTRA_HARNESS_IMAP_USER=astra1@example.test\n'
  printf 'export ASTRA_HARNESS_IMAP_PASSWORD=astra-harness\n'
  printf 'export ASTRA_HARNESS_SMTP_PORT=2025\n'
  printf 'export ASTRA_HARNESS_JMAP_URL=http://127.0.0.1:2080\n'
}

usage() {
  printf 'usage: scripts/dev/harness.sh up|down|reset|env\n'
  printf '  up      start the stack (bootstrap + seed; blocks until healthy)\n'
  printf '  down    stop and remove the containers\n'
  printf '  reset   wipe the volumes and reseed from empty\n'
  printf '  env     print the exports the gated live tests read\n'
}

cmd="${1:-up}"

case "$cmd" in
  up)
    require_docker
    info "starting the harness (bootstrap + seed; blocks until healthy)"
    (cd "$HARNESS_ROOT" && docker compose up -d --wait)
    info "harness healthy: IMAP 127.0.0.1:2143 (implicit TLS), SMTP submission 127.0.0.1:2025, JMAP 127.0.0.1:2080"
    info "run the gated test with: eval \"\$(scripts/dev/harness.sh env)\""
    ;;
  down)
    require_docker
    info "stopping the harness"
    (cd "$HARNESS_ROOT" && docker compose down)
    ;;
  reset)
    require_docker
    info "resetting the harness (wiping volumes and reseeding)"
    (cd "$HARNESS_ROOT" && docker compose down -v && docker compose up -d --wait)
    info "harness healthy on a clean slate"
    ;;
  env)
    show_env
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    die "unknown command '$cmd' (up|down|reset|env)"
    ;;
esac
