#!/usr/bin/env bash
# The SP1 vertical slice check: the whole loop end to end against the local harness,
# with the real desktop binary and no window.
#
#   scripts/dev/slice-check.sh
#
# What it proves, in order:
#   1. the harness is up (Docker required; harness.sh owns the friendly abort when
#      it is missing or asleep)
#   2. `astra-desktop --headless-slice add` boots the host over a scratch data
#      directory, dials astra1@example.test over real TLS, accepts the harness's
#      self-signed certificate through the product's own exception path, adds the
#      account, dispatches refresh_mail, and prints a folder tree holding INBOX and
#      the custom folder Projects
#   3. `--headless-slice list` restarts the binary over the same directory and the
#      tree comes back from the keyring and the engine store without re-adding
#
# The script owns the harness for its duration: the stack comes down on exit however
# the run ends. The scratch directory is removed the same way, and the list phase
# takes the stored credential back out of the OS secure store itself.
#
# POSIX-safe constructs only (no arrays, no `[[ ]]`): the script runs under any sh,
# with bash 5 the floor per AGENTS.md.

set -eu

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HARNESS="$ROOT/scripts/dev/harness.sh"
BIN_DIR="$ROOT/target/debug"

say() { printf 'slice-check: %s\n' "$1"; }
fail() { printf 'slice-check: FAIL: %s\n' "$1" >&2; exit 1; }

# The binary the build produces, .exe on Windows.
slice_binary() {
  case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*) printf '%s/astra-desktop.exe' "$BIN_DIR" ;;
    *) printf '%s/astra-desktop' "$BIN_DIR" ;;
  esac
}

# The harness must be up before anything else runs: its start-up aborts with the
# friendly Docker message when Docker is missing or its daemon is asleep.
"$HARNESS" up

SLICE_DIR="$(mktemp -d)"
finish() {
  rc=$?
  "$HARNESS" down >/dev/null 2>&1 || true
  rm -rf "$SLICE_DIR"
  exit "$rc"
}
trap finish EXIT

say "building the desktop binary"
(cd "$ROOT" && cargo build -p astra-desktop)

BIN="$(slice_binary)"
[ -x "$BIN" ] || fail "the desktop binary is not at $BIN"

# The harness connection details, the same exports the gated live tests read, plus
# the scratch data directory both phases share. The path is normalised to Windows
# form under an MSYS shell: a native exe does not reliably read an MSYS path out of
# an environment variable.
eval "$("$HARNESS" env)"
DATA_DIR="$SLICE_DIR/data"
if command -v cygpath >/dev/null 2>&1; then
  DATA_DIR="$(cygpath -w "$DATA_DIR")"
fi
export ASTRA_SLICE_DATA_DIR="$DATA_DIR"

# Runs one phase, its tree JSON captured beside the narration the binary prints.
run_phase() { # phase  json-out
  say "phase $1: running the headless slice"
  if ! "$BIN" --headless-slice "$1" >"$2"; then
    fail "phase $1 did not complete (the diff is in the narration above)"
  fi
}

# The script's own assertion, beside the binary's: the tree JSON holds the reserved
# INBOX and the custom folder the harness seeds.
assert_tree() { # json-file  phase
  for folder in INBOX Projects; do
    if ! grep -q "\"$folder\"" "$1"; then
      fail "phase $2: the tree JSON holds no $folder folder"
    fi
  done
  say "phase $2: the tree holds INBOX and Projects"
}

run_phase add "$SLICE_DIR/add-tree.json"
assert_tree "$SLICE_DIR/add-tree.json" add

run_phase list "$SLICE_DIR/list-tree.json"
assert_tree "$SLICE_DIR/list-tree.json" list

say "PASS: the add run and the restart both held the folder tree"
