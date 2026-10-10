// Stages the release-built astra-cli binary for tauri `bundle.externalBin`,
// mirroring the kylins staging mechanism minimally. externalBin entries
// resolve per target as `bin/<name>-<target-triple>[.exe]`; this copies
// `target/release/astra-cli[.exe]` from the workspace build to that name for
// the host triple (cross-compile support: pass the target triple as an
// argument).
//
// `--placeholder-ok`: tauri-build validates the staged path's EXISTENCE on
// every cargo invocation of this package (even `cargo check`), so without a
// staged file the crate cannot build the CLI it needs to stage: a bootstrap
// loop. In placeholder mode a missing release binary stages a 0-byte marker
// so the crate keeps building; the bundling chain always re-stages the real
// binary after `cargo build --release --bin astra-cli`, so a placeholder can
// never reach an installer.
import { copyFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const placeholderOk = process.argv.includes("--placeholder-ok");
const tripleArg = process.argv
  .slice(2)
  .find((a) => a.includes("-") && !a.startsWith("--"));
// client/desktop/scripts -> client/desktop -> the workspace root.
const desktopDir = dirname(dirname(fileURLToPath(import.meta.url)));
const root = dirname(dirname(desktopDir));
const triple = tripleArg ??
  execSync("rustc -vV", { encoding: "utf8" }).match(/^host: (.+)$/m)[1].trim();
const isWindows = triple.includes("windows");
const binName = isWindows ? "astra-cli.exe" : "astra-cli";
const src = join(root, "target", "release", binName);
const outDir = join(desktopDir, "bin");
mkdirSync(outDir, { recursive: true });
const dst = join(outDir, `astra-cli-${triple}${isWindows ? ".exe" : ""}`);

if (existsSync(src)) {
  copyFileSync(src, dst);
  console.log(`[stage-cli] staged ${dst}`);
} else if (existsSync(dst)) {
  console.log(`[stage-cli] no release binary yet; keeping existing ${dst}`);
} else if (placeholderOk) {
  writeFileSync(dst, "");
  console.warn(
    `[stage-cli] PLACEHOLDER staged at ${dst}: build the real CLI before bundling`,
  );
} else {
  console.error(
    `[stage-cli] missing ${src}: run 'cargo build --release --bin astra-cli' first`,
  );
  process.exit(1);
}
