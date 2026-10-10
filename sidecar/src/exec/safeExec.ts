// SPDX-License-Identifier: GPL-3.0-only
/**
 * safe_exec: the ONLY way the agent side of the sidecar reaches astra-cli.
 * Ported from the kylins sidecar's safe_exec, trimmed to the Astra CLI
 * contract. Four hard rules, all enforced here:
 *
 * 1. No shell, ever. execFile with an argv ARRAY; argv[0] is the LOGICAL
 *    prefix checked against the allowlist (astra-cli), the binary actually
 *    executed is opts.cliPath and it receives argv.slice(1).
 * 2. Scrubbed child environment: PATH plus every ASTRA_* variable of this
 *    process (the host's passthrough seam), plus the injected
 *    ASTRA_DATA_DIR and ASTRA_CORRELATION_ID. This process's model API keys
 *    and host internals must not leak into the child. ASTRA_DATA_DIR is how
 *    the CLI finds the app data dir (engine store and AI ledger); without it
 *    the CLI would fall back to the platform per-user directory. A falsy
 *    dataDir still spawns, with a loud stderr warning (degrades, never
 *    hard-fails: the CLI has its own resolution chain).
 * 3. Loud config errors. cliPath comes from the init envelope; the host
 *    sends the empty string when it cannot resolve the binary, so the guard
 *    is a falsiness check. This throw is the resolver's loud-error contract.
 * 4. The child's cwd is the data dir, so any relative path a CLI verb
 *    touches resolves inside the app's data, never the sidecar's cwd.
 *
 * Exit codes pass through: a non-zero exit does NOT throw; the exitCode
 * field carries it and stderr rides along, so the model can read the CLI's
 * error envelope. Only spawn-level failures reject.
 */
import { execFile } from 'node:child_process';

export interface SafeExecOpts {
  /** Absolute path to the astra-cli binary (from init; falsy = unresolved). */
  cliPath: string | null;
  /** Allowed logical argv[0] prefixes; the factory default is astra-cli. */
  allowlist: string[];
  /** App data dir from init: the child cwd and the ASTRA_DATA_DIR env. */
  dataDir?: string | null;
  /** The turn's correlation id, injected as ASTRA_CORRELATION_ID. */
  correlationId?: string | null;
}

export interface SafeExecResult {
  stdout: string;
  stderr: string;
  exitCode: number;
}

/** Per-stream cap before the truncation marker is appended. */
export const MAX_OUTPUT_LENGTH = 50 * 1024;
const TRUNCATION_MARKER = '[truncated]';
/** execFile's own backstop, well above the manual cap so WE do the
 *  truncating (a maxBuffer kill loses the exit code and most of the output). */
const EXECFILE_MAX_BUFFER = 8 * 1024 * 1024;

function truncate(stream: string): string {
  if (stream.length <= MAX_OUTPUT_LENGTH) return stream;
  return stream.slice(0, MAX_OUTPUT_LENGTH) + TRUNCATION_MARKER;
}

/** The scrubbed child environment: PATH, this process's ASTRA_* variables
 *  (the host passthrough seam), and the injected ASTRA_DATA_DIR and
 *  ASTRA_CORRELATION_ID. Nothing else survives. */
export function buildChildEnv(
  opts: Pick<SafeExecOpts, 'dataDir' | 'correlationId'>,
): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { PATH: process.env.PATH };
  for (const [key, value] of Object.entries(process.env)) {
    if (key.startsWith('ASTRA_') && value !== undefined) env[key] = value;
  }
  if (opts.dataDir) {
    env.ASTRA_DATA_DIR = opts.dataDir;
  } else {
    // Degraded, not fatal: the CLI falls back to its own resolution chain.
    // Loud, so a wrong-database symptom stays diagnosable.
    process.stderr.write(
      'safe_exec: WARNING dataDir was not delivered by the init envelope; ' +
        'the CLI will fall back to its own data-dir resolution and may open the wrong database\n',
    );
  }
  if (opts.correlationId !== undefined && opts.correlationId !== null) {
    if (typeof opts.correlationId === 'string' && opts.correlationId.length > 0) {
      env.ASTRA_CORRELATION_ID = opts.correlationId;
    } else {
      // A provided-but-invalid correlation id is a caller bug: drop it loudly
      // (the argv flag carries the id too; an empty env value could only
      // confuse an auditor).
      process.stderr.write(
        'safe_exec: WARNING correlationId was provided but is not a non-empty string; ' +
          'ASTRA_CORRELATION_ID is NOT injected\n',
      );
    }
  }
  return env;
}

export function runSafeExec(argv: string[], opts: SafeExecOpts): Promise<SafeExecResult> {
  const argv0 = argv[0];
  if (argv0 === undefined || !opts.allowlist.includes(argv0)) {
    return Promise.reject(new Error(`safe_exec: prefix not allowed: ${argv0 ?? ''}`));
  }
  if (!opts.cliPath) {
    return Promise.reject(new Error('safe_exec: cliPath not configured'));
  }
  const cliPath = opts.cliPath;
  const env = buildChildEnv(opts);
  return new Promise<SafeExecResult>((resolve, reject) => {
    execFile(
      cliPath,
      argv.slice(1),
      {
        env,
        // The child lives in the app data dir; undefined keeps this
        // process's cwd when the host delivered none.
        cwd: opts.dataDir || undefined,
        maxBuffer: EXECFILE_MAX_BUFFER,
        // NO shell option: execFile never consults one, keep it that way.
      },
      (error, stdout, stderr) => {
        if (error && typeof error.code !== 'number') {
          // Spawn-level failure (binary missing, maxBuffer kill): no exit
          // code exists to pass through, so this rejects loudly.
          reject(new Error(`safe_exec: failed to run ${argv0}: ${error.message}`));
          return;
        }
        resolve({
          stdout: truncate(stdout),
          stderr: truncate(stderr),
          exitCode: error ? (error.code as number) : 0,
        });
      },
    );
  });
}
