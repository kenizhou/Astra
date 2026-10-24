# SP2t plan: the platform batch

Spec: docs/superpowers/specs/2026-10-24-sp2t-platform-design.md. Branch
`sp2t-platform` from main at the docs commit. Each task: reds first, the
implementation, the battery, a conventional commit.

### Task 1: the core (D1's toggle, D2's consent, D5's language, D6's
latch)

**Files:** crates/astra-app/src (the preferences: debug_enabled,
analytics_consented, language, zone_offered; the settings blocks and
rows; the analytics event recorder writing the AI ledger only under
consent; the device-zone offer's latch and the accept-into-setting
path); the protocol shapes; fakes.

- [ ] Tests first; commit `feat: feed the platform core`.

### Task 2: the host legs (D1's logger and commands, D4's registration
and state, D4's share)

**Files:** client/desktop/src (the rotating file logger installed at
boot; the log_tail and log_export commands; the default-mail runtime
registration behind the offer; the window-state persistence over the
app data dir; the share-target entry handing files into the compose
prefill channel).

- [ ] Tests first; commit `feat: land the platform host legs`.

### Task 3: the surfaces

**Files:** client/web/src (the Diagnostics category: viewer, copy for
support, export, the debug toggle; the Privacy category with the
statement and the switches; the language picker and the dates following
it; the device-zone offer UI; the default-mail setting; the settings
and About copy as the first localised set; the showcase flag's gate if
D7 ships).

- [ ] Tests first; commit `feat: surface the platform batch`.

### Task 4: the proofs + the docs

**Files:** the proofs (the log rotating under a burst over the headless
harness; the consented ledger rows observed only under consent; the
export's bytes; the language driving the dates); docs/logging.md +
analytics.md + os-integration.md + timestamps.md + zh copies (created);
the capabilities rows (62, 63, 120, 121, 122, 123, 124, 125); the
fragment.

- [ ] Commit `feat: prove the platform batch live and record it`.

### Task 5: close-out

- [ ] Full battery; branch `sp2t-platform` pushed; PR (squash expected); ledger cleanup.

## Self-review notes

- Spec coverage: the preferences and the recorder (T1), the logger and
  the OS legs (T2), the categories and the localisation beginning
  (T3), proofs + docs (T4).
- Interface chain: the core state (T1) -> the host legs (T2) -> the
  surfaces (T3) -> proofs + docs (T4).
- Risks: the file logger must honour the log contract from its first
  line (no secrets, no content, no paths — the tests grep the file for
  the forbidden classes); the analytics recorder must be unreachable
  without consent (pin the negative); the language preference must not
  touch the calendar's zone; the device-zone offer must never override
  a picked zone (pin); the window-state file is not a preference (the
  core preferences stay the one document; the host owns its own state
  file); D7 may be recorded as the shortfall if the leg outgrows the
  batch.
