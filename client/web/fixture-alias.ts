// SPDX-License-Identifier: GPL-3.0-only
// The fixtures-mode alias decision, shared by vite.config.ts and pinned by
// harness.test.ts: only `--mode fixtures` may swap the Tauri bridge for the
// recorded-snapshot harness. Every other mode (test, production) leaves the
// bridge alone, which is what keeps the harness out of the shipping bundle.

/**
 * The alias pair for one mode, empty unless the mode is fixtures. The
 * caller passes the directory the harness resolves from (vite.config.ts's
 * own `__dirname`), so this module carries no node-only APIs: the web
 * tsconfig has no node types, and the harness test imports this module.
 */
export function tauriBridgeAliases(mode: string, configDir: string): Record<string, string> {
  if (mode !== 'fixtures') {
    return {};
  }
  const harness = `${configDir}/src/services/__fixtures__/harness.ts`;
  return {
    '@tauri-apps/api/core': harness,
    '@tauri-apps/api/event': harness,
  };
}
