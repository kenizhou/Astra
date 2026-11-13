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
 * The gate, the argv grammar, and the spawn live in gated.ts, shared with
 * the key tools; the outcome contract (never throw, details carry the
 * status) is documented there.
 */
import { Type } from 'typebox';
import type { AgentToolResult, ToolDefinition } from '@earendil-works/pi-coding-agent';
import {
  buildCliArgv,
  defineMailTool,
  execCli,
  runGated,
  type MailToolDeps,
  type MailToolDetails,
} from './gated';

// The shared plumbing's public face stays importable from here: the
// harnesses and the sibling suites name these through mailTools.
export { CLI_PREFIX, buildCliArgv } from './gated';
export type { MailToolDeps, MailToolDetails } from './gated';

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
      'in the index except each one\'s Trash; an account covers its whole ' +
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
