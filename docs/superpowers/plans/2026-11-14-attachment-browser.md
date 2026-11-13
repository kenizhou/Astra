# Attachment browser plan

Spec: docs/superpowers/specs/2026-11-14-attachment-browser-design.md. Branch
`attachment-browser` from main at the spec commit. Each task: reds first,
implement, battery, one conventional commit. One contract change: a new
docs/attachment-browser.md with its zh mirror in the same commit, the capabilities
row moves with its zh mirror, a changelog fragment lands, and porting-backlog.md
moves the attachment browser into the shipped list. No engine change. The
fetch-on-miss sweep is an escalated reference deviation (the spec states it; the
PR escalates it).

### Task 1: the core aggregation + view-model + command (D1, D2)

**Files:** a new crates/astra-app/src/attachment_browser.rs (impl App
attachment_browser(filter) -> AttachmentBrowserView: per account search_mail
has_attachment:true cap 200 -> mail_by_keys -> message_attachments -> flatten
parts beside source metadata; per-message best-effort, per-account honest error;
rows + coverage report; sender + mime-prefix filters) + sibling tests; a new
crates/astra-viewmodel/src/attachment_browser.rs (the entry: part id u32/file_name/
media_type/size + MessageKey account+key + subject/from/sent_at; the coverage
report; re-export from lib.rs); the App slot/method (lib.rs at the cap, reclaim doc
lines); a new client/desktop/src/commands/attachment_browser.rs (commands.rs at the
cap) through a new AppPort method, registered in lib.rs; the web wire types +
wrapper in services/attachments.ts (never ipc.ts).

- [ ] Confirm at the hook site: search_mail (engine-api reads.rs:33), mail_by_keys
      (:273), message_attachments (body.rs:169), MailRow metadata (search_index/
      mail.rs:53), the search.rs account-handle pattern (:101), AttachmentRow
      (astra-viewmodel/reading.rs:77), save_attachment (commands/attachments.rs:133,
      App::attachment_content). Reds first (core: seed multipart sources over the
      fake provider, assert flattened rows + source refs + newest-first + the cap +
      coverage report + per-account error honesty + sender/mime filters; command:
      malformed ref refused, the pull answers the view); then the core + view-model
      + command. Commit `feat: the attachment browser aggregation`.
- [ ] astra-app/astra-viewmodel/client/desktop NOT cap-exempt: lib.rs at the cap
      (reclaim doc lines, no growth beyond 500), commands.rs at the cap (child
      module). Newtypes across the boundary; the part id stays the message-scoped
      u32. No engine change (fetch-on-miss via message_attachments). Existing tests
      stay green.

### Task 2: the modal + entry + contract + live proof (D3)

**Files:** a new client/web/src/features/attachment-browser/
(AttachmentBrowser.tsx + attachmentBrowserStore.ts + pure grouping/chip helpers) +
tests, copying the ShortcutsHelp overlay pattern (zustand isOpen, null while
closed, useDialogFocus, backdrop/Escape, role=dialog aria-modal), mounted in the
AppShell tail beside CommandPalette; rows grouped by sender (newest-first, count +
total size), mime chips (All + top buckets, image/* -> Images, application/pdf ->
PDF, from a mime-unfiltered snapshot), sender prefilter banner + Clear, coverage
footer + Refresh, empty state; Save As reuses saveAttachment, open-source-message
dispatches intents.openMessage + closes the modal; a palette command (the
saved-searches shape, commands.ts); docs/attachment-browser.md + zh;
docs/capabilities.md row + zh; docs/changelog/unreleased/attachment-browser.md
(Platforms: desktop, Bump: minor); docs/porting-backlog.md + zh;
crates/astra-app/src/tests_live_harness/attachment_browser_proof.rs (submit a note
with an attachment, sync, browse via the core method, assert the row + byte read,
withdraw both copies) + tests_live_harness.rs registration.

- [ ] Reds first (the pure grouping/chip table tests; the view tests; the
      offline-skipping gated live proof); then the modal + entry + docs. Commit
      `feat: the attachment browser surface and contract`.
- [ ] client/web not exempt: new files in features/attachment-browser/; ipc.ts at
      the cap (services/attachments.ts); AppShell.tsx 281 / commands.ts 344 room.
      The modal is NOT a fourth view (ShellView unchanged, the rail untouched).
      Preview stays a placeholder. vitest traps: act() mid-test, mock the service
      whole. Comments present tense; no em/en dashes; British English; source files
      end with a newline (CI rustfmt stricter). docs/zh mirrors same commit.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix sidecar
      test; npx tsc --noEmit && npx vitest run (client/web); cargo run -p xtask
      -- gate. If the plugin tests fail with AlreadyGranted, clean the stale
      /tmp/astra-desktop-plugin-* scratch dirs (Windows pid-recycle flake) and
      re-run. WATCH DISK SPACE (clean the cargo incremental cache if it balloons).
- [ ] Controller arms the live proof on a clean harness (scripts/dev/harness.sh
      reset, then up + env; cargo test -p astra-app
      tests_live_harness::attachment_browser_proof -- --test-threads=1), then unset.
- [ ] Push the branch; PR (squash expected) ESCALATING the fetch-on-miss reference
      deviation in the body; CI watch; squash merge, delete the branch, sync main;
      ledger cleanup.

## Self-review notes

- Spec coverage: D1's aggregation + D2's view-model/command (T1); D3's modal + entry
  + contract + live proof + battery + PR (T2).
- Risks: the escalated fetch-on-miss reference deviation (kylins is cache-only;
      documented + escalated, cache-only parity a follow-up); the has_attachment
      guess coverage caveat; the N-body-fetch cost (bounded by the 200 cap, stated);
      the at-cap lib.rs/commands.rs (reclaim + child module); the no-persistent-index
      difference. Preview placeholder, no ContactDetail prefilter/date-bounds/Clear,
      CLI verb deferred (recorded gaps).
- No eval/pipeline bump (model-facing unchanged). Live-proof lesson: reset the
      harness first; clean stale plugin scratch dirs if the battery flakes; ensure
      trailing newlines (CI rustfmt).