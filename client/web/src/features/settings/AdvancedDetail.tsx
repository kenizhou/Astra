// SPDX-License-Identifier: GPL-3.0-only
// The Advanced detail: the AI assistant access panel. The toggle drives
// the host's run/stop seam, the allowlist picker names the accounts a
// tool may reach (the rows the settings snapshot carries), and every
// control reflects the host's pulled truth, never local optimism. The
// client-config snippet carries the relay command a user pastes into
// an MCP client's config. Nothing persists: the panel states the
// restart rule itself.

import { useCallback, useEffect, useState } from 'react';

import { mcpSetAllowed, mcpSetEnabled, mcpStatus } from '@/services/ipc';
import type { McpStatusView } from '@/services/ipc';

import { useSettingsStore } from './settingsStore';

/** The view the panel renders before the first pull answers: the host's
 * own default state, off and exposing nothing. */
const OFF_VIEW: McpStatusView = {
  status: 'off',
  endpoint: '',
  allowed: [],
  relay_command: [],
};

/** The reach sentence an empty gate draws: the exposure contract in one line. */
export const REACH_EMPTY_SENTENCE =
  'No tool reaches any mail: no account is named.';

/** The reach sentence a named gate draws. */
export function reachSentence(count: number): string {
  return count === 1
    ? 'Tools may read and act on 1 account.'
    : `Tools may read and act on ${count} accounts.`;
}

/** The restart rule, said in the panel itself. */
export const RESTART_SENTENCE =
  'The server turns off and the account list starts empty at every restart.';

/** The status line per state word. */
function statusSentence(status: McpStatusView['status']): string {
  if (status === 'listening') {
    return 'The server is running.';
  }
  if (status === 'failed') {
    return 'The server could not start.';
  }
  return 'The server is off.';
}

/** The Advanced page of the settings screen. */
export function AdvancedDetail() {
  const accounts = useSettingsStore((s) => s.accounts);
  const [view, setView] = useState<McpStatusView>(OFF_VIEW);
  // A second click inside one round trip would compute from the stale
  // pulled set and drop the first change; the picker stands still until
  // the write lands and the pull answers.
  const [writing, setWriting] = useState(false);

  const pull = useCallback((): void => {
    mcpStatus()
      .then((next) => setView(next))
      .catch((err: unknown) => console.error('mcp: the status pull failed', err));
  }, []);

  useEffect(() => {
    pull();
  }, [pull]);

  const setEnabled = (on: boolean): void => {
    void mcpSetEnabled(on).then(pull);
  };

  const setAllowed = (account: string, expose: boolean): void => {
    const next = expose
      ? [...view.allowed, account]
      : view.allowed.filter((id) => id !== account);
    setWriting(true);
    void mcpSetAllowed(next)
      .then(pull)
      .finally(() => {
        setWriting(false);
      });
  };

  return (
    <section aria-label="Advanced settings" className="flex flex-col gap-4">
      <h2 className="text-sm font-semibold text-foreground">Advanced</h2>
      <div className="flex flex-col gap-1">
        <label className="flex items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            aria-label="AI assistant access"
            checked={view.status !== 'off'}
            onChange={(event) => setEnabled(event.target.checked)}
          />
          AI assistant access
        </label>
        <p className="text-xs text-muted-foreground">{statusSentence(view.status)}</p>
        {view.status === 'failed' && view.reason !== undefined && (
          <p role="alert" className="max-w-[420px] text-xs text-[var(--warning)]">
            {view.reason}
          </p>
        )}
      </div>
      <div className="flex flex-col gap-1">
        <p className="text-xs text-muted-foreground">
          {view.allowed.length === 0 ? REACH_EMPTY_SENTENCE : reachSentence(view.allowed.length)}
        </p>
        <div className="flex flex-col gap-1">
          {accounts.map((account) => (
            <label
              key={account.account}
              className="flex items-center gap-2 text-xs text-foreground"
            >
              <input
                type="checkbox"
                aria-label={`Expose ${account.address}`}
                checked={view.allowed.includes(account.account)}
                disabled={writing}
                onChange={(event) => setAllowed(account.account, event.target.checked)}
              />
              {account.address}
            </label>
          ))}
          {accounts.length === 0 && (
            <p className="text-xs text-muted-foreground">No account is configured.</p>
          )}
        </div>
        <p className="text-xs text-muted-foreground">{RESTART_SENTENCE}</p>
      </div>
      <div className="flex flex-col gap-1">
        <p id="mcp-client-config-caption" className="text-xs text-muted-foreground">
          Paste this command into your MCP client&apos;s config.
        </p>
        {view.relay_command.length > 0 && (
          <code
            aria-labelledby="mcp-client-config-caption"
            className="max-w-full overflow-x-auto rounded-[var(--radius-xs)] border border-border bg-background px-2 py-1.5 text-xs text-foreground"
          >
            {view.relay_command.join(' ')}
          </code>
        )}
      </div>
    </section>
  );
}
