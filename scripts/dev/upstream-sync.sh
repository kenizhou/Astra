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
GH_REPO="kenizhou/Astra"
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

# 2. The revision anchors. CUR: vendor-sync's last `vendor:` commit message
# (a 40-hex anywhere in subject or body), refs-lock baseline as the floor.
# MERGED: main's own last vendored revision, so a branch pushed with no PR
# standing (a night the PR step failed) still converges to an open PR
# instead of no-opping past its own unmerged work. Anchoring here, not on
# last_ported, so an open sync PR neither resends a stale digest nor skips
# a revision.
git -C "$WT" fetch origin --quiet
# CUR greps only the `vendor: upstream` commits: a port commit touching
# vendor/engine also becomes "the last commit on the path", and its message
# carries no hash, so an unfiltered read falls to the baseline.
CUR=$(git -C "$WT" log --grep='^vendor: upstream ' -1 --format=%B -- vendor/engine | grep -oE '[0-9a-f]{40}' | head -1 || true)
[ -z "$CUR" ] && CUR="$BASELINE"
# MERGED is refs-lock's `last_ported` read from ORIGIN/MAIN's copy (the
# local checkout can sit stale behind a merge): the contract's own record
# of what main holds. Message-derived anchors fail under a squash-merged
# sync PR (no 40-hex to read), and tree-exact anchors fail by design
# (main's vendor/engine carries Astra's in-tree edits).
MERGED=$(git -C "$WT" show origin/main:refs/refs-lock.toml | awk '/name = "email-calendar-sync-engine"/{f=1} f && /^last_ported = /{print; exit}' | cut -d'"' -f2)
[ -z "$MERGED" ] && MERGED="$BASELINE"
OPEN_PR=$(gh pr list -R "$GH_REPO" --head vendor-sync --state open --json number --jq '.[0].number' 2>/dev/null || true)
if ! git -C "$UPSTREAM" cat-file -e "$CUR^{commit}" 2>/dev/null; then
    die "current vendored rev $CUR is not a commit upstream knows (history rewrite?)"
fi

# A landed sync the lock table has not recorded yet: no open PR, the branch
# tip IS an ancestor of main (a merge commit made it one), and last_ported
# still names an older revision. The advance is the merge's own bookkeeping
# (the reviewed content is already on main), so it commits straight to main
# as the contract's step 7; without it the next night would re-PR the same
# range. The commit builds without a checkout (this worktree cannot hold
# main; the primary checkout owns it): a temporary index over origin/main,
# the one blob replaced, commit-tree, push.
if [ "$DRY_RUN" -eq 0 ] && [ -z "$OPEN_PR" ] && [ "$MERGED" != "$CUR" ] \
   && git -C "$WT" merge-base --is-ancestor vendor-sync origin/main 2>/dev/null; then
    TMPIDX=$(mktemp)
    trap 'rm -f "$TMPIDX"' EXIT
    git -C "$WT" show origin/main:refs/refs-lock.toml \
        | sed "s/^last_ported = \"$MERGED\"/last_ported = \"$CUR\"/" > "$TMPIDX.content"
    BLOB=$(git -C "$WT" hash-object -w "$TMPIDX.content")
    rm -f "$TMPIDX.content"
    # A no-op advance (the sed matched nothing) commits nothing: an empty
    # bookkeeping commit on main is noise, never bookkeeping.
    OLD_BLOB=$(git -C "$WT" rev-parse "origin/main:refs/refs-lock.toml")
    if [ "$BLOB" != "$OLD_BLOB" ]; then
        GIT_INDEX_FILE="$TMPIDX" git -C "$WT" read-tree origin/main
        GIT_INDEX_FILE="$TMPIDX" git -C "$WT" update-index --cacheinfo "100644,$BLOB,refs/refs-lock.toml"
        TREE=$(GIT_INDEX_FILE="$TMPIDX" git -C "$WT" write-tree)
        COMMIT=$(git -C "$WT" commit-tree "$TREE" -p origin/main \
            -m "vendor: advance last_ported to ${CUR:0:12}")
        git -C "$WT" push --quiet origin "$COMMIT:main"
        log "the sync merged; advanced last_ported to $CUR"
    else
        log "the sync merged but last_ported already stands; nothing to advance"
    fi
    rm -f "$TMPIDX"
    exit 0
fi

BASE="$CUR"
if [ "$NEW" = "$CUR" ]; then
    if [ "$MERGED" = "$CUR" ] || [ -n "$OPEN_PR" ]; then
        log "already at $NEW; nothing to do"
        exit 0
    fi
    # The branch already carries what upstream holds and main lacks it: the
    # standing pile. Open the PR for what main holds..now, with no landing
    # to redo (the worktree is already at NEW).
    BASE="$MERGED"
    log "branch already at $NEW; opening the standing PR main lacks"
fi
git -C "$UPSTREAM" merge-base --is-ancestor "$BASE" "$NEW" || die "$BASE is not an ancestor of $NEW (upstream force-push?)"

COUNT=$(git -C "$UPSTREAM" rev-list --count "$BASE..$NEW")
log "sync range $CUR -> $NEW ($COUNT commits)"

# 3. The classified digest of the range.
DOCS=0; TESTS=0; CICD=0; DEPS=0; BEHAVIOUR=0
SECURITIES=""
while IFS= read -r line; do
    case "$line" in
        commit\ *)
            subject="${line#commit }"
            short="${subject%% *}"
            # Word boundaries on the short acronyms: a bare `rce` pattern
            # matches inside the word "source" and held clean syncs hostage.
            if echo "$subject" | grep -iqE 'secur|vuln|\bcve[- ]|\brce\b|\bxss\b|\bcsrf\b|inject|overflow|forgery|spoof'; then
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
done < <(git -C "$UPSTREAM" log --format='commit %h %s' --name-only "$BASE..$NEW")

DIGEST=$(cat <<EOF
## Engine sync ${BASE:0:7}..${NEW:0:7} ($COUNT commits)

### Classification
- docs only: $DOCS file(s)
- tests only: $TESTS file(s)
- ci/docker: $CICD file(s)
- dependencies (Cargo.toml / Cargo.lock): $DEPS file(s)
- behaviour: $BEHAVIOUR file(s)

### Commits
$(git -C "$UPSTREAM" log --oneline "$BASE..$NEW")

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

# 4-6 land the new revision; the standing-pile path (the branch already at
# NEW) skips them: there is nothing to land, and a re-archive of the same
# revision commits nothing.
if [ "$NEW" != "$CUR" ]; then
    # 4. Land the new revision in the vendor-sync worktree, conversion off
    # (the pristine rule from the vendor import: archive with
    # core.autocrlf=false, vendor/** marked -text).
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
    git -C "$WT" commit --quiet -m "vendor: upstream $NEW $(date +%F)" -m "$(git -C "$UPSTREAM" log --oneline "$BASE..$NEW")"
    git -C "$WT" push --quiet origin vendor-sync
    log "pushed vendor-sync at $NEW"
fi

# 7. Open the PR, or leave the open one to its updated diff. Every gh call
# names the repository: the scheduled shell starts in no repository at all,
# and gh's own git discovery fails there (the bug that left five revisions
# pushed with no PR). A range with no security classification opens READY
# and arms auto-merge (a merge commit, never squash: the branch tip must
# become an ancestor of main or the conflict-surfacing mechanism dies); a
# security-classified range opens as a draft and waits for morning review,
# exactly the 22:00 contract's rule.
EXISTING=$(gh pr list -R "$GH_REPO" --head vendor-sync --state open --json number --jq '.[0].number' 2>/dev/null || true)
if [ -n "$EXISTING" ]; then
    log "sync PR #$EXISTING is open and now carries $NEW"
    PR="$EXISTING"
else
    if [ -n "$SECURITIES" ]; then
        gh pr create -R "$GH_REPO" --base main --head vendor-sync \
            --title "vendor: engine sync ${BASE:0:7}..${NEW:0:7}" \
            --body "$DIGEST" --draft
        log "opened the sync PR vendor-sync -> main as a draft (security classified; held for review)"
        exit 0
    fi
    gh pr create -R "$GH_REPO" --base main --head vendor-sync \
        --title "vendor: engine sync ${BASE:0:7}..${NEW:0:7}" \
        --body "$DIGEST"
    PR=$(gh pr list -R "$GH_REPO" --head vendor-sync --state open --json number --jq '.[0].number' 2>/dev/null || true)
    log "opened the sync PR vendor-sync -> main as #$PR"
fi
# Auto-merge arms only a clean-classified PR (a security range reaching here
# as an existing non-draft PR keeps its standing state for the morning).
if [ -z "$SECURITIES" ] && [ -n "$PR" ]; then
    DRAFT=$(gh pr view -R "$GH_REPO" "$PR" --json isDraft --jq '.isDraft' 2>/dev/null || echo true)
    if [ "$DRAFT" = "false" ]; then
        gh pr merge -R "$GH_REPO" "$PR" --auto --merge >/dev/null 2>&1 || \
            log "auto-merge could not arm on #$PR (morning review takes it)"
        log "auto-merge armed on sync PR #$PR (merge commit, green gate)"
    fi
fi
