// The stub astra-cli used by the sidecar tests. safeExec spawns the cliPath
// binary (node in tests) with the argv tail after the nominal prefix token
// and cwd pinned at the data dir; node resolves the first positional (the
// verb word, e.g. `folders`) as this file because the tests copy it under
// that name into the data dir. It echoes the invocation it saw as one JSON
// object on stdout: the verb word that named the script, the argv after it,
// the ASTRA_* env subset, and the cwd. Tests assert the exact argv grammar,
// the scrubbed environment, and the cwd.
const fs = require('node:fs');
const path = require('node:path');
const script = path.basename(process.argv[1] ?? '');
const args = process.argv.slice(2);
fs.appendFileSync(
  'stub-captures.jsonl',
  JSON.stringify({
    argv: [script, ...args],
    data_dir: process.env.ASTRA_DATA_DIR ?? null,
    correlation_id: process.env.ASTRA_CORRELATION_ID ?? null,
    cwd: process.cwd(),
  }) + '\n',
);
process.stdout.write(
  JSON.stringify({
    script,
    argv: args,
    env_keys: Object.keys(process.env).sort(),
    data_dir: process.env.ASTRA_DATA_DIR ?? null,
    correlation_id: process.env.ASTRA_CORRELATION_ID ?? null,
    cwd: process.cwd(),
  }),
);
process.exit(0);
