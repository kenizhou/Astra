#!/usr/bin/env bash
# The vendor-sync driver (docs/upstream-sync.md, "Engine updates"). Nightly
# at 22:00 via Task Scheduler; --dry-run prints what it would do and changes
# nothing. Idempotent: a night with no upstream movement exits 0 as a no-op,
# and a sync PR left open is updated, never duplicated.
#
# The loop: fetch upstream; read the vendored revision from vendor-sync's own
# last `vendor: upstream <rev>` commit message (never from refs-lock alone, so
# an open sync PR neither resends a stale digest nor skips a revision); no-op
# when equal; otherwise git-archive the new revision into the vendor-sync
# worktree, verify the tree is byte-pristine by blob hash, commit, push, and
# open (or update) the `vendor-sync -> main` PR with a classified digest.
#
# The script never merges main into vendor-sync, never force-pushes, and any
# failure exits non-zero into the log for morning review.
set -euo pipefail

REPO="D:/Projects/mailclient/Astra"
WT="D:/Projects/mailclient/Astra-vendor-sync"
DRY_RUN=0
[ "${1:-}" = "--dry-run" ] && DRY_RUN=1

log() { echo "upstream-sync: $*"; }
die() { echo "upstream-sync: ERROR: $*" >&2; exit 1; }

# Resolve the upstream checkout from refs/refs-lock.toml (root + the engine
# row's path).
LOCK="$REPO/refs/refs-lock.toml"
[ -f "$LOCK" ] || die "refs/refs-lock.toml not found at $LOCK"
ROOT=$(grep '^root = ' "$LOCK" | head -1 | cut -d'"' -f2)
ENGINE_PATH=$(awk '/name = "email-calendar-sync-engine"/{f=1} f && /^path = /{print; exit}' "$LOCK" | cut -d'"' -f2)
BASELINE=$(awk '/name = "email-calendar-sync-engine"/{f=1} f && /^baseline = /{print; exit}' "$LOCK" | cut -d'"' -f2)
[ -n "$ROOT" ] && [ -n "$ENGINE_PATH" ] && [ -n "$BASELINE" ] || die "refs-lock.toml: engine row unreadable"
UPSTREAM="$ROOT/$ENGINE_PATH"
[ -d "$UPSTREAM/.git" ] || die "upstream checkout not found at $UPSTREAM"
# A worktree's .git is a pointer file, not a directory.
[ -e "$WT/.git" ] || die "vendor-sync worktree not found at $WT (run the PR 2 setup in docs/upstream-sync.md)"

# 1. Fetch and resolve the target revision.
git -C "$UPSTREAM" fetch origin --quiet
NEW=$(git -C "$UPSTREAM" rev-parse origin/main)

# 2. The current vendored revision: vendor-sync's last `vendor:` commit
# message (a 40-hex anywhere in subject or body), refs-lock baseline as the
# floor. Anchoring here, not on last_ported, so an open sync PR neither
# resends a stale digest nor skips a revision.
CUR=$(git -C "$WT" log -1 --format=%B -- vendor/engine | grep -oE '[0-9a-f]{40}' | head -1 || true)
[ -z "$CUR" ] && CUR="$BASELINE"
if ! git -C "$UPSTREAM" cat-file -e "$CUR^{commit}" 2>/dev/null; then
    die "current vendored rev $CUR is not a commit upstream knows (history rewrite?)"
fi

if [ "$NEW" = "$CUR" ]; then
    log "already at $NEW; nothing to do"
    exit 0
fi
git -C "$UPSTREAM" merge-base --is-ancestor "$CUR" "$NEW" || die "$CUR is not an ancestor of $NEW (upstream force-push?)"

COUNT=$(git -C "$UPSTREAM" rev-list --count "$CUR..$NEW")
log "sync range $CUR -> $NEW ($COUNT commits)"

# 3. The classified digest of the range.
DOCS=0; TESTS=0; CICD=0; DEPS=0; BEHAVIOUR=0
SECURITIES=""
while IFS= read -r line; do
    case "$line" in
        commit\ *)
            subject="${line#commit }"
            short="${subject%% *}"
            if echo "$subject" | grep -iqE 'secur|vuln|cve[- ]|rce|xss|csrf|inject|overflow|forgery|spoof'; then
                SECURITIES="$SECURITIES\n- $subject"
            fi
            ;;
        docs/*|*.md|*.MD) DOCS=$((DOCS+1)) ;;
        */tests/*|*test*|*/fixtures/*) TESTS=$((TESTS+1)) ;;
        .github/*|docker/*|scripts/ci/*) CICD=$((CICD+1)) ;;
        Cargo.toml|Cargo.lock|*/Cargo.toml) DEPS=$((DEPS+1)) ;;
        "") ;;
        *) BEHAVIOUR=$((BEHAVIOUR+1)) ;;
    esac
done < <(git -C "$UPSTREAM" log --format='commit %h %s' --name-only "$CUR..$NEW")

DIGEST=$(cat <<EOF
## Engine sync ${CUR:0:7}..${NEW:0:7} ($COUNT commits)

### Classification
- docs only: $DOCS file(s)
- tests only: $TESTS file(s)
- ci/docker: $CICD file(s)
- dependencies (Cargo.toml / Cargo.lock): $DEPS file(s)
- behaviour: $BEHAVIOUR file(s)

### Commits
$(git -C "$UPSTREAM" log --oneline "$CUR..$NEW")

### Review notes
- Sync PRs merge with a merge commit, never squash (docs/upstream-sync.md).
- On merge, advance \`last_ported\` in refs/refs-lock.toml.
- The dependencies bucket includes Cargo.toml/Cargo.lock: check
  rusqlite/libsqlite3-sys movements, which can break astra-ai in the same build.
- Pre-merge protocol: Astra battery + xtask gate + live harness serial, the
  engine battery inside \`vendor/engine\`, all with CARGO_BUILD_BUILD_DIR
  pointed at a private directory.
EOF
)
if [ -n "$SECURITIES" ]; then
    DIGEST="$DIGEST
- SECURITY-classified subjects (held for review, never auto-merged):
$SECURITIES"
fi

if [ "$DRY_RUN" -eq 1 ]; then
    log "dry run; nothing changed"
    echo "$DIGEST"
    exit 0
fi

# 4. Land the new revision in the vendor-sync worktree, conversion off (the
# pristine rule from the vendor import: archive with core.autocrlf=false,
# vendor/** marked -text).
git -C "$WT" checkout --quiet vendor-sync
git -C "$WT" pull --ff-only --quiet
rm -rf "$WT/vendor/engine"
mkdir -p "$WT/vendor/engine"
git -c core.autocrlf=false -C "$UPSTREAM" archive "$NEW" | tar -x -C "$WT/vendor/engine"

# 5. Pristine proof by blob hash (never working-tree bytes: autocrlf
# materialises CRLF for extensions .gitattributes does not pin).
git -C "$WT" add -A vendor/engine
git -C "$WT" ls-files -s vendor/engine | awk '{print $2"\t"$4}' | sed 's|\tvendor/engine/|\t|' | sort -k2 > /tmp/sync-astra-vendor.txt
git -C "$UPSTREAM" ls-tree -r "$NEW" --format='%(objectname)%x09%(path)' | sort -k2 > /tmp/sync-upstream-tree.txt
if ! diff -q /tmp/sync-astra-vendor.txt /tmp/sync-upstream-tree.txt > /dev/null; then
    diff /tmp/sync-astra-vendor.txt /tmp/sync-upstream-tree.txt | head -10 >&2 || true
    die "the landed tree is not byte-pristine with $NEW (blob hashes differ); refusing to commit"
fi

# 6. Commit and push.
git -C "$WT" commit --quiet -m "vendor: upstream $NEW $(date +%F)" -m "$(git -C "$UPSTREAM" log --oneline "$CUR..$NEW")"
git -C "$WT" push --quiet origin vendor-sync
log "pushed vendor-sync at $NEW"

# 7. Open the PR, or leave the open one to its updated diff.
EXISTING=$(gh pr list --head vendor-sync --state open --json number --jq '.[0].number' 2>/dev/null || true)
if [ -n "$EXISTING" ]; then
    log "sync PR #$EXISTING is open and now carries $NEW"
else
    gh pr create --base main --head vendor-sync \
        --title "vendor: engine sync ${CUR:0:7}..${NEW:0:7}" \
        --body "$DIGEST" --draft
    log "opened the sync PR vendor-sync -> main"
fi
