# Watch-replies plan

Spec: docs/superpowers/specs/2026-11-14-watch-replies-design.md. Branch
`watch-replies` from main at the spec commit. Each task: reds first, implement,
battery, one conventional commit. One contract change: a new docs/watch-replies.md
with its zh mirror in the same commit, the capabilities row moves with its zh
mirror, a changelog fragment lands, and porting-backlog.md moves watch-replies into
the shipped list. No engine change, no AI bump. The member-key re-resolution is in
v1 (the derived-thread re-keying trap). The no-focus-suppression + consume-at-drain
are escalated reference deviations.

### Task 1: the watch store + the core verbs + the check + the preference (D1, D2)

**Files:** a new crate crates/astra-watches (Cargo.toml + src/lib.rs + src/store.rs
+ src/migrations/0001_init.sql, the astra-reminders shape; watch_replies table
[account_id, thread_id, member_key, created_at, PK(account_id,thread_id)]; watch/
unwatch/list, one-shot consume = unwatch, malformed-skip) + store tests;
crates/astra-app/src/watch_replies.rs (install_watch_store + watch_thread/
unwatch_thread [resolve like categories.rs resolve_thread_key + member-key capture]
+ list_watches + check_watched_replies(batch) async post-pass [preference gate,
group by account, one mail_by_keys per account for batch keys + watch member keys,
match resolved-against-resolved, build notice, consume, fail-open]) + hook tests;
the watch_replies_enabled preference (preferences/mod.rs field + state.rs projection
+ persist, SetWatchRepliesEnabled SettingsIntent arm [settings/mod.rs at cap
reclaim], the SettingsBlock wire field); the App slot (lib.rs at cap reclaim).

- [ ] Confirm at the hook site: the astra-reminders store shape, categories.rs
      resolve_thread_key (:158-180), list_rows::thread_key (:312-317), mail_by_keys
      (engine-api reads.rs:280), mail_on_threads (reads.rs:261), the
      notifications_enabled preference pattern, the dispatch fakes (FakeProvider +
      deliver + RefreshMail + drain). Reds first (store: one-shot/idempotence/list-
      order/account-scoping/migration-race; hook: watch + reply fires, the RE-KEYING
      REGRESSION [reply msgid sorts BEFORE original], preference off -> no notice +
      watch NOT consumed, unwatched -> nothing, second reply after consume ->
      nothing, fail-open, provider-threaded leg); then the store + core + preference.
      Commit `feat: the watch store and reply check`.
- [ ] astra-watches/astra-app NOT cap-exempt (lib.rs/settings/mod.rs at cap reclaim).
      Newtypes. The member-key re-resolution is load-bearing (derived threads re-key
      on merge; both sides resolve through the engine at check time). No engine
      write. Existing tests stay green.

### Task 2: the host hook + the reply notice (D3 host)

**Files:** the boot install (boot.rs assemble_state opens watches.sqlite +
install_watch_store, the categories pattern, no timer, WATCHES_STORE_FILE const); a
WatchPort supertrait (app_state/watch_port.rs, trait AppPort: WatchPort) + each
impl's child block (app_state/port, commands_fakes/port, showcase/app); a new
commands/watch_replies.rs (watch_set/watch_clear/watch_list, commands.rs at cap)
registered in lib.rs; the inside-pass hook in HostDelivery::pass (delivery.rs:274-277,
after the drain, gated on notifications_enabled) calling check_watched_replies +
NotifyBridge::deliver_replies; notify.rs deliver_replies (the deliver_reminders
shape, no focus suppression, title "Reply received", the reply event REPLY_EVENT +
with_reply_history + windowed wiring).

- [ ] Reds first (a notify-bridge test: deliver_replies honours enabled, no focus
      suppression, the reply event; the command routing over the fakes); then the
      boot + supertrait + commands + hook + notice. Commit
      `feat: the watch reply hook and notice`.
- [ ] client/desktop NOT cap-exempt: the WatchPort supertrait keeps app_state.rs at
      the cap (E0119 distinct traits); commands.rs at cap (child module). The sync
      NewMailQueue commit fold UNTOUCHED (the hook is the async post-pass). The drop
      order untouched (no timer, no AppState field). Existing tests stay green.

### Task 3: the web surface + contract + live proof (D3 web, D4)

**Files:** a new client/web/src/services/watchReplies.ts (invoke wrappers, never
ipc.ts); features/watch-replies/watchStore.ts (zustand watch-set seeded from
watch_list, refreshed on toggle) + watchMenu.ts (Watch replies/Unwatch, single row,
batch disabled, the blockSenderMenu pattern) + tests; the rowMenu.ts wiring (at cap,
the builder import + array line); events/replyFlow.ts + replyEvents.ts (the
reminderFlow clone) + App.tsx mount feeding notificationHistoryStore; the
General-settings watch_replies_enabled toggle (GeneralDetail.tsx); surfaces.ts if
needed; docs/watch-replies.md + zh; docs/capabilities.md row + zh;
docs/changelog/unreleased/watch-replies.md (Platforms: desktop, Bump: minor);
docs/porting-backlog.md + zh; crates/astra-app/src/tests_live_harness/
watch_replies_proof.rs (submit an original, watch via the core, deliver a reply with
In-Reply-To set [reply msgid sorts first, proving re-key survival over the real
wire], run the pass + check, assert one "Reply received" notice + watch row gone +
second reply nothing + both messages in INBOX unmoved, withdraw) +
tests_live_harness.rs registration.

- [ ] Reds first (the menu label flip + batch disabled, the service, the settings
      toggle, the reply-event mount; the offline-skipping gated live proof); then the
      web + docs. Commit `feat: the watch replies surface and contract`.
- [ ] client/web not exempt: rowMenu.ts at cap (the watchMenu builder); ipc.ts at cap
      (services/watchReplies.ts). vitest traps: act() mid-test, mock the service
      whole. Comments present tense; no em/en dashes; British English; source files
      end with a newline. docs/zh mirrors same commit.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix sidecar
      test; npx tsc --noEmit && npx vitest run (client/web); cargo run -p xtask
      -- gate. If the plugin tests fail with AlreadyGranted, clean the stale
      /tmp/astra-desktop-plugin-* scratch dirs (Windows pid-recycle flake) and
      re-run. WATCH DISK SPACE (clean the cargo incremental cache if it balloons).
- [ ] Controller arms the live proof on a clean harness (scripts/dev/harness.sh
      reset, then up + env; cargo test -p astra-app
      tests_live_harness::watch_replies_proof -- --test-threads=1), then unset.
- [ ] Push the branch; PR (squash expected) ESCALATING the no-focus-suppression +
      consume-at-drain deviations in the body; CI watch; squash merge, delete the
      branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's store + D2's core/check/preference (T1); D3's host hook/notice
  (T2); D3's web + D4's contract + live proof + battery + PR (T3).
- Risks: the derived-thread RE-KEYING trap (the member-key re-resolution must be in
      v1; both sides resolve through the engine at check time; the re-keying
      regression test is the killer test); the post-pass hook (the sync commit fold
      untouched); the cap pressure (lib.rs/settings/mod.rs/commands.rs/app_state.rs
      at cap -> reclaim/supertrait; rowMenu.ts at cap -> builder); the escalated
      no-focus-suppression + consume-at-drain deviations. Overflow folding /
      background-lane only / stale member link / device-local / no CLI/badge/list/
      multi-select (recorded gaps).
- No eval/pipeline bump (not AI). No engine change. Live-proof lesson: reset the
      harness first; the reply msgid sorts first to prove re-key survival; clean
      stale plugin scratch dirs if the battery flakes; ensure trailing newlines.