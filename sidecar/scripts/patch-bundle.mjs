// Post-build patches required to run the pi SDK from a single-file CJS
// bundle. Ported verbatim in substance from the kylins sidecar's
// scripts/patch-bundle.mjs (the recipe came from its pi SDK spike):
//
// 1. import_meta shim: esbuild emits `import_meta<N> = {};` for
//    import.meta in CJS output, and pi's config.ts does
//    fileURLToPath(import.meta.url) at module load; with the empty shim that
//    throws. Point it at the bundle file.
// 2. pi-coding-agent reads its own package.json at runtime (VERSION,
//    piConfig) via a __dirname-relative path. In a bundle that resolves
//    beside the bundle, so inline { name, version, piConfig } at the read
//    site.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

export const PI_BUNDLE_EXTERNALS = [];

/**
 * @param {string} code - bundle text
 * @param {string} piPkgDir - dir containing pi-coding-agent's package.json
 * @returns {string} patched bundle text
 */
export function patchPiBundle(code, piPkgDir) {
  code = code.replace(
    /(import_meta\d*) = \{\};/g,
    '$1 = { url: require("url").pathToFileURL(__filename).href };',
  );

  const piPkg = JSON.parse(readFileSync(join(piPkgDir, 'package.json'), 'utf8'));
  const inlined = JSON.stringify({
    name: piPkg.name,
    version: piPkg.version,
    piConfig: piPkg.piConfig,
  });
  const pkgRe = /pkg = JSON\.parse\(\(0, \w+\.readFileSync\)\(getPackageJsonPath\(\), "utf-8"\)\)/;
  if (pkgRe.test(code)) {
    code = code.replace(pkgRe, `pkg = ${inlined}`);
  } else {
    // Loud, not silent: if a future SDK version changes the emitted shape we
    // want the build log to say so.
    console.warn('[patch-bundle] pi package.json read site not found; VERSION will fall back');
  }
  return code;
}
