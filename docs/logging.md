# Logging and diagnostics: the Astra Mail contract

**Scope.** The windowed run's production file log, the log contract every
line holds, and the Diagnostics settings category that renders, copies,
and exports it. Binding on every surface that logs in the product's name
and on every host shape that ships a diagnostics view. The headless
runner keeps its own stderr logger and is out of scope here.

**Principle.** *A diagnostic that cannot start never fails the app over
it.* The logger opens best effort, a file that cannot open leaves the
run unlogged with the reason on stderr, and a rotation that refuses
leaves the logger dark. Diagnostics observe the product; they never
gate it.

## The file log

The windowed host installs a rotating file log at boot, before any line
the boot itself logs: `astra.log` in the app data directory, beside the
stores. It is hand-rolled, the headless runner's stderr logger's
sibling, because a diagnostic pulls no logging framework behind it.

- **The rotation.** Five files stand at the most: the current
  `astra.log` beside `astra.log.1` through `astra.log.4`. When the
  current file holds something and the next line would carry it past
  one mebibyte, the rotation shifts: the handle closes first (Windows
  refuses to rename an open file), the oldest rotated file goes, the
  rest shift one number up, the current file becomes `.1`, and a fresh
  current opens carrying the rotation notice as its first line. A
  chatty run costs the disk five mebibytes and never more.
- **The line shape.** One line is the epoch stamp, the level word,
  then the message: `1792848440 INFO: boot: the app core is up over
  2 account(s)`. The stamp is the std clock's unix seconds.
- **The level.** The level is read at install from the persisted
  debug preference: info unless the Diagnostics toggle stands, debug
  when it does. The preference is read directly from the preferences
  file, because the core's own copy stands only after the assembly,
  which itself logs. A change therefore lands with the next launch,
  the honest wording the toggle's note states: *takes effect the next
  time Astra Mail starts*.
- **One installer a process gets.** The logger is installed for the
  process's life; a logger already installed stays the one the run
  keeps.

## The log contract

No secrets, no mail content, no repository paths: the contract the
codebase already holds, kept from the file's first line. The logger's
own lines (the install notice, the rotation notice) carry none, and
every host callsite follows the discipline. The contract is proven
over the real boot's own written bytes: the proof greps the file the
real launch wrote for secret markers (`password`, `secret`, `token`,
`bearer`), mail addresses, repository path shapes, path separators,
and drive letters, and none may cross.

## The Diagnostics category

The settings screen's Diagnostics page renders the log and its two
controls, every control reflecting the pulled truth, never local
optimism:

- **The viewer.** The Read button pulls the current file's last lines
  through the host's `log_tail` command, a screenful (200 lines) by
  default, rendered monospace with the newest last. The tail caps a
  runaway ask at 1000 lines, answers the whole file when it holds
  fewer than the ask, and answers nothing when no file stands. No
  path crosses beyond the lines themselves.
- **Copy for support.** The copy takes the tail (pulled fresh when
  the viewer stands empty) beside the version line, and the privacy
  note under the controls names what logs never carry: *The log
  carries no message content, no credentials, and no file paths.*
- **Export.** The export asks the host's save dialog over the whole
  current file under the log's own name (`astra.log`), writes its
  exact bytes, and answers the typed outcome family the attachment
  saves answer: saved with the path, cancelled, or failed with the
  reason. A cancelled dialog writes nothing; a failed read never
  opens the dialog.
- **The debug toggle.** The persisted switch whose change lands with
  the next launch, as its note states.

The export records its own product event (`log_exported` with the byte
count) through the analytics recorder, so the event lands only while
consent stands (analytics.md).

## Known gaps

- The logger keeps no trace level and no per-target filter: two
  levels stand, info and debug, one toggle.
- The tail renders the current file only; the rotated files are the
  export's and the disk's own business.
