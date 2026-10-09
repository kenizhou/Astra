#!/bin/sh
# Self-bootstrapping entrypoint for the Astra Mail Stalwart harness.
#
# Stalwart v0.16 has no configuration file: a fresh server boots into "bootstrap mode",
# is configured through its management API, and restarts to come up as a full server.
# This wrapper drives that sequence with `stalwart-cli apply` (Stalwart's own CLI, built
# into the image by ./Dockerfile), then creates the harness accounts' custom folders, so
# `docker compose up` yields an identical, ready server every time:
#
#   1. start the server (bootstrap mode if the store is empty),
#   2. complete setup (no ACME, no DKIM) and restart into a full server,
#   3. apply the harness configuration below and restart, since Stalwart reads it at
#      startup,
#   4. seed the custom folders and the three harness notes (IMAP and SMTP over TLS),
#   5. write a readiness marker and run the server in the foreground.
#
# It is idempotent: every plan is an `upsert`, which converges on a warm data volume
# rather than duplicating, and the seeder tolerates folders and notes that already
# exist.
set -eu

CONFIG="${STALWART_CONFIG:-/etc/stalwart/config.json}"
HTTP="http://127.0.0.1:8080"
ADMIN_PW="${HARNESS_ADMIN_PW:-astra-harness-admin}"
MARKER="/var/lib/stalwart/.harness-ready"

log() { printf '[harness] %s\n' "$1"; }

# The CLI caches the server's schema under $HOME, and the image's user has no home
# directory.
cli() {
  HOME=/tmp stalwart-cli --no-color --url "$HTTP" --user admin --password "$ADMIN_PW" "$@"
}

# Applies the NDJSON plan on stdin, stopping the entrypoint (and so the container) on
# any failure.
apply() {
  if ! out=$(cli apply --quiet --stdin 2>&1); then
    log "FAILED to apply a configuration plan: $out"
    exit 1
  fi
  log "$out"
}

start_server() {
  stalwart --config "$CONFIG" &
  SRV=$!
}

stop_server() {
  kill "$SRV" 2>/dev/null || true
  wait "$SRV" 2>/dev/null || true
}

wait_http() {
  i=0
  until curl -sf "$HTTP/healthz/live" >/dev/null 2>&1; do
    i=$((i + 1))
    [ "$i" -gt 90 ] && {
      log "server HTTP never became ready"
      return 1
    }
    sleep 1
  done
}

restart_server() {
  log "restarting: $1"
  stop_server
  start_server
  wait_http
}

trap 'stop_server; exit 0' TERM INT

rm -f "$MARKER"
log "starting Stalwart"
start_server
wait_http

# Only a server in bootstrap mode has the Bootstrap object.
if cli get Bootstrap singleton >/dev/null 2>&1; then
  log "completing first-run bootstrap"
  apply <<'EOF'
{"@type":"update","object":"Bootstrap","value":{"defaultDomain":"example.test","requestTlsCertificate":false,"generateDkimKeys":false}}
EOF
  restart_server "into a full server"
else
  log "store already bootstrapped; skipping setup"
fi

# The harness domain and its two accounts. The anonymous rate limit goes because every
# request a test sends comes from one address: a few runs in a row pass the default and
# get `429`, which reads as a server that refuses sign-ins.
log "creating the harness domain and accounts"
apply <<'EOF'
{"@type":"upsert","object":"Domain","matchOn":["name"],"value":{"domain":{"name":"example.test"}}}
{"@type":"upsert","object":"Account","matchOn":["name"],"value":{"astra1":{"@type":"User","name":"astra1","domainId":"#domain","description":"Astra One","credentials":{"0":{"@type":"Password","secret":"astra-harness"}},"roles":{"@type":"User"}}}}
{"@type":"upsert","object":"Account","matchOn":["name"],"value":{"astra2":{"@type":"User","name":"astra2","domainId":"#domain","description":"Astra Two","credentials":{"0":{"@type":"Password","secret":"astra-harness"}},"roles":{"@type":"User"}}}}
{"@type":"update","object":"Http","value":{"rateLimitAnonymous":null}}
EOF

restart_server "to read the accounts above"

log "seeding the harness folders and notes"
/bin/sh /harness/seed.sh

touch "$MARKER"
log "harness ready"

wait "$SRV"
