// SPDX-License-Identifier: GPL-3.0-only
/**
 * The ten mail tools the agent may call: the kylins sidecar's five
 * (list_folders, list_messages, read_message, save_draft, discard_draft)
 * beside the search and act family (search_messages, mark_read,
 * set_flagged, archive_message, move_to_trash) the assistant chat's SP2y
 * slice adds over the same CLI. Every tool funnels through safe_exec and
 * runGated; the model never builds a raw argv, so no send-class or unknown
 * verb can be spelled from the tool surface at all.
 *
 * The argv grammar is the one docs/cli.md pins: two positional words first,
 * then --json, then --correlation-id with the turn's correlation id, then
 * the verb's own flags. Sending stays a human action: the CLI refuses the
 * send class structurally, and the act verbs stop at the row's trash (no
 * delete tool, no send tool); every act gates through the bridge while the
 * search rides as a read.
 *
 * Tool outputs are strings on the wire, so the truncation policy adopts
 * the reference's caps (inbox-zero's readEmail bound of four thousand
 * characters on the read; the two-hundred-character preview cap the
 * desktop relay's tool_result audit row already enforces).
 *
 * pi 0.84.2 customTools shape (verified against the kylins adapter and
 * dist/core/extensions/types.d.ts): { name, label, description,
 * parameters: a TypeBox schema, execute(toolCallId, params, signal,
 * onUpdate, ctx) => Promise<AgentToolResult<TDetails>> }.
 *
 * The tools NEVER throw: the outcome rides in details (MailToolDetails) and
 * the harness turns it into the audited tool_result envelope. A non-zero
 * CLI exit is still 'ok' with the exit envelope in the output for the model
 * to read (the safe_exec contract); only spawn-level failures and gate
 * throws map to 'error'.
 */
import { Type, type TSchema } from 'typebox';
import type { AgentToolResult, ToolDefinition } from '@earendil-works/pi-coding-agent';
import type { PermissionBridge } from '../permissions/bridge';
import type { ToolResultStatus } from '../harness/types';
import type { BudgetView } from '../budget';
import { runSafeExec } from '../exec/safeExec';

/** The argv prefix token every tool's command starts with; the logical
 *  prefix safe_exec checks its allowlist against. */
export const CLI_PREFIX = 'astra-cli';

/** Structured tool outcome; the harness translates it into the wire
 *  tool_result envelope. */
export interface MailToolDetails {
  status: ToolResultStatus;
  output?: string;
}

/** Identity helper mirroring pi's defineTool: preserves TParams inference
 *  through the array literal WITHOUT a runtime import of the pi SDK in the
 *  tests (type-only imports keep the esbuild test bundles small); the
 *  intersection is what lets heterogeneous tools share one array. */
function defineMailTool<TParams extends TSchema>(
  tool: ToolDefinition<TParams, MailToolDetails>,
): ToolDefinition<TParams, MailToolDetails> & ToolDefinition<any, any, any> {
  return tool as ToolDefinition<TParams, MailToolDetails> & ToolDefinition<any, any, any>;
}

export interface MailToolDeps {
  bridge: PermissionBridge;
  /** Live view of the init-delivered astra-cli path. */
  cliPath: () => string | null;
  /** Live view of the init-delivered data dir. */
  dataDir?: () => string | null;
  /** Live view of the current turn's correlation id, set by the stdio loop
   *  before a prompt runs; it rides the argv flag and the spawn env. */
  correlationId: () => string | null;
  /** Live view of the current turn's tool budget. The deadline reaches the
   *  gate as this view handed over at construction, never a clock threaded
   *  through every call: the tools are built once and shared by every
   *  session, so the live view is the one seam that stays per-turn. A null
   *  view (between turns) or an absent dep (unit constructions) means no
   *  clock stands and the gate never skips for budget. */
  budget?: () => BudgetView | null;
}

function textResult(text: string, details: MailToolDetails): AgentToolResult<MailToolDetails> {
  return { content: [{ type: 'text', text }], details };
}

/** The read tool's model-facing output bound, the reference assistant's
 *  own readEmail cap (getEmailForLLM maxLength 4000): the read is the one
 *  tool whose CLI document carries whole message bodies, so it is the one
 *  that needs a cap. */
export const READ_OUTPUT_CAP = 4000;

/** Caps one tool output at `cap` characters, appending a marker that names
 *  the bound so the model knows the output was cut and by how much. Short
 *  outputs pass through whole. */
export function capOutput(text: string, cap: number): string {
  if (text.length <= cap) return text;
  return text.slice(0, cap) + `[truncated at ${cap} characters]`;
}

/** Gate plus outcome mapping shared by all five tools. The tool budget is
 *  the first gate: past the bound the model is told no (the skipped outcome
 *  the harness already reports, its audit row landing exactly as a refused
 *  tool's does through the same tool_result envelope), never a silent
 *  drop, and no permission card opens for a call that cannot run. */
async function runGated(
  deps: MailToolDeps,
  toolCallId: string,
  tool: string,
  args: Record<string, unknown>,
  exec: () => Promise<string>,
): Promise<AgentToolResult<MailToolDetails>> {
  const budget = deps.budget?.() ?? null;
  if (budget !== null && budget.spent()) {
    const msg = "the turn's tool budget is spent";
    return textResult(`${tool}: skipped (${msg}; finish in text)`, {
      status: 'skipped',
      output: msg,
    });
  }
  if (deps.bridge.check(tool) === 'needs-approval') {
    let outcome: { approved: boolean };
    try {
      outcome = await deps.bridge.requestApproval({
        toolCallId,
        tool,
        level: deps.bridge.levelOf(tool),
        args,
      });
    } catch (e) {
      // rejectAll fired while suspended (prompt abort): no decision exists
      // and nothing executed; skipped, not error.
      const msg = e instanceof Error ? e.message : String(e);
      return textResult(`${tool}: skipped (${msg})`, { status: 'skipped', output: msg });
    }
    if (!outcome.approved) {
      return textResult(`${tool}: rejected by the user; nothing was executed`, {
        status: 'rejected',
      });
    }
  }
  // auto / notify / approved: run. notify's tell-the-panel leg is the
  // tool_result envelope itself.
  try {
    const output = await exec();
    return textResult(output, { status: 'ok', output });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    return textResult(`${tool} failed: ${msg}`, { status: 'error', output: msg });
  }
}

/** Assemble the CLI argv: prefix token, the two positionals, then --json,
 *  then the correlation flag pair when a turn id is live, then the verb's
 *  flags. docs/cli.md pins the grammar and the free flag order within. */
export function buildCliArgv(
  correlationId: string | null,
  positional: [string, string],
  flags: string[],
): string[] {
  const argv = [CLI_PREFIX, positional[0], positional[1], '--json'];
  if (correlationId) argv.push('--correlation-id', correlationId);
  argv.push(...flags);
  return argv;
}

/** Run one CLI invocation through safe_exec and shape the tool output: the
 *  CLI's JSON envelope document verbatim on success, a readable one-liner
 *  when the CLI failed without printing one. */
async function execCli(deps: MailToolDeps, argv: string[]): Promise<string> {
  const r = await runSafeExec(argv, {
    cliPath: deps.cliPath(),
    allowlist: [CLI_PREFIX],
    dataDir: deps.dataDir?.() ?? null,
    correlationId: deps.correlationId(),
  });
  if (r.exitCode !== 0 && r.stdout.trim().length === 0) {
    return `astra-cli exited ${r.exitCode}: ${r.stderr.trim()}`;
  }
  return r.stdout.trim();
}

/** Build the ten customTool definitions. Created once at startup and shared
 *  by every session: the deps are live views, so an init refresh needs no
 *  rebuild. */
export function createMailTools(deps: MailToolDeps): ToolDefinition[] {
  const listFolders = defineMailTool({
    name: 'list_folders',
    label: 'List folders',
    description:
      'List an account\'s folders (mailboxes) with id, name, role, parent, and ' +
      'unread count, in the sidebar\'s canonical order. Discover account ids ' +
      'first with the accounts index the host delivers, or ask the user.',
    parameters: Type.Object({
      account: Type.String(),
    }),
    execute: (toolCallId, params) =>
      runGated(deps, toolCallId, 'list_folders', { account: params.account }, () =>
        execCli(
          deps,
          buildCliArgv(deps.correlationId(), ['folders', 'list'], ['--account', params.account]),
        ),
      ),
  });

  const listMessages = defineMailTool({
    name: 'list_messages',
    label: 'List messages',
    description:
      'Page through an account\'s message list: key, from, subject, date, unread, ' +
      'draft flag, attachment flag, and a preview. Optional folder key filters ' +
      'to that folder; limit caps the page (default 50).',
    parameters: Type.Object({
      account: Type.String(),
      folder: Type.Optional(Type.String()),
      limit: Type.Optional(Type.Number()),
    }),
    execute: (toolCallId, params) => {
      const args: Record<string, unknown> = { account: params.account };
      const flags: string[] = ['--account', params.account];
      if (params.folder !== undefined) {
        args.folder = params.folder;
        flags.push('--folder', params.folder);
      }
      if (params.limit !== undefined) {
        args.limit = params.limit;
        flags.push('--limit', String(params.limit));
      }
      return runGated(deps, toolCallId, 'list_messages', args, () =>
        execCli(deps, buildCliArgv(deps.correlationId(), ['messages', 'list'], flags)),
      );
    },
  });

  const readMessage = defineMailTool({
    name: 'read_message',
    label: 'Read message',
    description:
      'Read one or more whole messages by key: the envelope plus the plain text ' +
      'body. The HTML alternative is reported present but never emitted. Pass ' +
      'every key you want in one call. Output past four thousand characters is ' +
      'truncated and says so.',
    parameters: Type.Object({
      account: Type.String(),
      keys: Type.Array(Type.String(), { minItems: 1 }),
    }),
    execute: (toolCallId, params) => {
      const flags: string[] = ['--account', params.account];
      for (const key of params.keys) flags.push('--key', key);
      return runGated(
        deps,
        toolCallId,
        'read_message',
        { account: params.account, keys: params.keys },
        async () =>
          capOutput(
            await execCli(deps, buildCliArgv(deps.correlationId(), ['messages', 'read'], flags)),
            READ_OUTPUT_CAP,
          ),
      );
    },
  });

  const saveDraft = defineMailTool({
    name: 'save_draft',
    label: 'Save draft',
    description:
      'Compose and store a draft through the engine\'s own draft store. The user ' +
      'approves the card before the draft is written. Never sends: sending is a ' +
      'human action in the app. A second save of one composition stores a second ' +
      'draft beside the first.',
    parameters: Type.Object({
      account: Type.String(),
      from: Type.String(),
      to: Type.Array(Type.String(), { minItems: 1 }),
      cc: Type.Optional(Type.Array(Type.String())),
      subject: Type.String(),
      body: Type.String(),
    }),
    execute: (toolCallId, params) => {
      const args: Record<string, unknown> = {
        account: params.account,
        from: params.from,
        to: params.to,
        subject: params.subject,
      };
      const flags: string[] = [
        '--account',
        params.account,
        '--from',
        params.from,
        '--to',
        params.to.join(','),
      ];
      if (params.cc !== undefined && params.cc.length > 0) {
        args.cc = params.cc;
        flags.push('--cc', params.cc.join(','));
      }
      flags.push('--subject', params.subject, '--plain', params.body);
      return runGated(deps, toolCallId, 'save_draft', args, () =>
        execCli(deps, buildCliArgv(deps.correlationId(), ['drafts', 'save'], flags)),
      );
    },
  });

  const discardDraft = defineMailTool({
    name: 'discard_draft',
    label: 'Discard draft',
    description:
      'Delete a stored draft through the engine\'s own draft store. The user ' +
      'approves the card before anything is deleted.',
    parameters: Type.Object({
      account: Type.String(),
      key: Type.String(),
    }),
    execute: (toolCallId, params) =>
      runGated(
        deps,
        toolCallId,
        'discard_draft',
        { account: params.account, key: params.key },
        () =>
          execCli(
            deps,
            buildCliArgv(
              deps.correlationId(),
              ['drafts', 'discard'],
              ['--account', params.account, '--key', params.key],
            ),
          ),
      ),
  });

  const searchMessages = defineMailTool({
    name: 'search_messages',
    label: 'Search messages',
    description:
      'Search mail and answer one page of hits, newest first, with no bodies: ' +
      'query the same DSL the app search field takes (from:a, to:b, quoted ' +
      'phrases, free text). Without an account the search covers every account ' +
      'the host exposed except each one\'s Trash; an account covers its whole ' +
      'mailbox; a folder narrows within the named account. Pages hold twenty ' +
      'by default: pass offset to continue, and more exist while offset plus ' +
      'rows is under total.',
    parameters: Type.Object({
      query: Type.String(),
      account: Type.Optional(Type.String()),
      folder: Type.Optional(Type.String()),
      limit: Type.Optional(Type.Number()),
      offset: Type.Optional(Type.Number()),
    }),
    execute: (toolCallId, params) => {
      const args: Record<string, unknown> = { query: params.query };
      const flags: string[] = ['--query', params.query];
      if (params.account !== undefined) {
        args.account = params.account;
        flags.push('--account', params.account);
      }
      if (params.folder !== undefined) {
        args.folder = params.folder;
        flags.push('--folder', params.folder);
      }
      flags.push('--limit', String(params.limit ?? 20));
      if (params.offset !== undefined) {
        args.offset = params.offset;
        flags.push('--offset', String(params.offset));
      }
      return runGated(deps, toolCallId, 'search_messages', args, () =>
        execCli(deps, buildCliArgv(deps.correlationId(), ['messages', 'search'], flags)),
      );
    },
  });

  const markRead = defineMailTool({
    name: 'mark_read',
    label: 'Mark read or unread',
    description:
      'Mark one message read (the default) or unread (read: false) through ' +
      'the same door the user\'s own click uses. The user approves the card ' +
      'before anything changes.',
    parameters: Type.Object({
      account: Type.String(),
      key: Type.String(),
      read: Type.Optional(Type.Boolean()),
    }),
    execute: (toolCallId, params) => {
      const read = params.read ?? true;
      const args: Record<string, unknown> = {
        account: params.account,
        key: params.key,
        read,
      };
      const flags: string[] = ['--account', params.account, '--key', params.key];
      if (!read) flags.push('--unread');
      return runGated(deps, toolCallId, 'mark_read', args, () =>
        execCli(deps, buildCliArgv(deps.correlationId(), ['messages', 'mark-read'], flags)),
      );
    },
  });

  const setFlagged = defineMailTool({
    name: 'set_flagged',
    label: 'Set or clear the flag',
    description:
      'Flag one message (the default) or clear its flag (flagged: false). ' +
      'The user approves the card before anything changes.',
    parameters: Type.Object({
      account: Type.String(),
      key: Type.String(),
      flagged: Type.Optional(Type.Boolean()),
    }),
    execute: (toolCallId, params) => {
      const flagged = params.flagged ?? true;
      const args: Record<string, unknown> = {
        account: params.account,
        key: params.key,
        flagged,
      };
      const flags: string[] = ['--account', params.account, '--key', params.key];
      if (!flagged) flags.push('--clear');
      return runGated(deps, toolCallId, 'set_flagged', args, () =>
        execCli(deps, buildCliArgv(deps.correlationId(), ['messages', 'set-flagged'], flags)),
      );
    },
  });

  const archiveMessage = defineMailTool({
    name: 'archive_message',
    label: 'Archive a message',
    description:
      'Move one message out of the inbox and into the account\'s archive ' +
      'folder. The user approves the card before anything moves; the move ' +
      'is visible in their own list and can be undone there.',
    parameters: Type.Object({
      account: Type.String(),
      key: Type.String(),
    }),
    execute: (toolCallId, params) =>
      actOnMessage(deps, toolCallId, 'archive_message', 'archive', params),
  });

  const moveToTrash = defineMailTool({
    name: 'move_to_trash',
    label: 'Move a message to trash',
    description:
      'Move one message to the account\'s trash folder. The user approves ' +
      'the card before anything moves; nothing is permanently deleted and ' +
      'the message is recoverable from trash.',
    parameters: Type.Object({
      account: Type.String(),
      key: Type.String(),
    }),
    execute: (toolCallId, params) =>
      actOnMessage(deps, toolCallId, 'move_to_trash', 'trash', params),
  });

  return [
    listFolders,
    listMessages,
    readMessage,
    saveDraft,
    discardDraft,
    searchMessages,
    markRead,
    setFlagged,
    archiveMessage,
    moveToTrash,
  ];
}

/** The shared body of the two move tools: one account/key pair through the
 *  gate onto the CLI's own move subverb. */
function actOnMessage(
  deps: MailToolDeps,
  toolCallId: string,
  tool: string,
  subverb: string,
  params: { account: string; key: string },
): Promise<AgentToolResult<MailToolDetails>> {
  return runGated(deps, toolCallId, tool, { account: params.account, key: params.key }, () =>
    execCli(
      deps,
      buildCliArgv(
        deps.correlationId(),
        ['messages', subverb],
        ['--account', params.account, '--key', params.key],
      ),
    ),
  );
}
