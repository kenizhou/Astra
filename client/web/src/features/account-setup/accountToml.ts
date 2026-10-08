// SPDX-License-Identifier: GPL-3.0-only
// Builds the account-config TOML the add_account command reads, from the same
// setup fields its test command takes: the frontend counterpart of
// astra-account's build_config_toml. Servers are a bare host plus a port, the
// TLS server name is the host alone, and an accepted certificate rides along
// as the one-server exception it is.

import type { AccountSetup, ConnectionSecurity } from '@/services/ipc';

/** The standard IMAP port for a connection security. */
export function imapDefaultPort(security: ConnectionSecurity): number {
  return security === 'start_tls' ? 143 : 993;
}

/**
 * Splits a server into its TLS server name and dial address, appending the
 * standard port when the input carries none. Mirrors the Rust side, so the
 * same input always builds the same account on either side of the IPC.
 */
function hostAndAddr(input: string, defaultPort: number): { host: string; addr: string } {
  const at = input.lastIndexOf(':');
  if (at > 0) {
    const host = input.slice(0, at);
    const port = input.slice(at + 1);
    if (host !== '' && port !== '' && /^\d+$/.test(port)) {
      return { host, addr: input };
    }
  }
  return { host: input, addr: `${input}:${defaultPort}` };
}

/** Escapes one string into a TOML basic string, quotes included. */
function tomlString(value: string): string {
  let out = '"';
  for (const ch of value) {
    const code = ch.codePointAt(0) ?? 0;
    if (ch === '"') {
      out += '\\"';
    } else if (ch === '\\') {
      out += '\\\\';
    } else if (ch === '\n') {
      out += '\\n';
    } else if (ch === '\r') {
      out += '\\r';
    } else if (ch === '\t') {
      out += '\\t';
    } else if (code < 0x20 || code === 0x7f) {
      out += `\\u${code.toString(16).padStart(4, '0')}`;
    } else {
      out += ch;
    }
  }
  return `${out}"`;
}

/** The canonical fingerprint form the config stores: lowercase hex, no colons. */
function canonicalFingerprint(sha256: string): string {
  return sha256.replaceAll(':', '').toLowerCase();
}

/** Serialises one setup into the account-config TOML to store and dial from. */
export function buildAccountToml(setup: AccountSetup): string {
  const { host, addr } = hostAndAddr(setup.imap_host.trim(), imapDefaultPort(setup.imap_security));
  const password = setup.credential === 'grant' ? undefined : setup.credential.password;

  const lines: string[] = [];
  lines.push('[imap]');
  lines.push(`addr = ${tomlString(addr)}`);
  lines.push(`server_name = ${tomlString(host)}`);
  lines.push(`username = ${tomlString(setup.username.trim())}`);
  if (password !== undefined) {
    lines.push(`password = ${tomlString(password)}`);
  }
  // Implicit TLS is the config's default; only the exception to it is written.
  if (setup.imap_security === 'start_tls') {
    lines.push('security = "start_tls"');
  }

  const accepted = setup.accepted_certificate;
  if (accepted) {
    lines.push('');
    lines.push('[[certificate_exception]]');
    lines.push(`server_name = ${tomlString(accepted.server_name)}`);
    lines.push(`sha256 = ${tomlString(canonicalFingerprint(accepted.sha256))}`);
  }
  return `${lines.join('\n')}\n`;
}
