// SPDX-License-Identifier: GPL-3.0-only
/**
 * The five key tools the agent may call over the CLI's keys verbs:
 * list_keys and list_collected_keys (the public reads), import_key (the
 * public-only cert import), promote_key and reject_key (the collected-key
 * review actions). PUBLIC material only: private-key import stays desktop
 * (the vault master key is OS-keyring-resident and the CLI contract
 * forbids opening the keyring), so no tool carries a passphrase, and the
 * CLI refuses secret-bearing import material with a pointer to the
 * desktop path.
 *
 * Every tool funnels through the shared gate (gated.ts) onto the same
 * argv grammar docs/cli.md pins. The two reads run auto; the import and
 * the two review actions ask at confirm: promote and reject change the
 * send resolver's future decisions, which is exactly why they gate.
 * None is send-class; sending stays a human action.
 */
import { Type } from 'typebox';
import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import {
  buildCliArgv,
  defineMailTool,
  execCli,
  runGated,
  type MailToolDeps,
} from './gated';

/** The key standards the tools answer, the closed set the CLI validates. */
const STANDARD = Type.Union([Type.Literal('openpgp'), Type.Literal('smime')]);

/** Build the five key customTool definitions. Created once at startup and
 *  shared by every session, the mail tools' discipline: the deps are live
 *  views, so an init refresh needs no rebuild. */
export function createKeyTools(deps: MailToolDeps): ToolDefinition[] {
  const listKeys = defineMailTool({
    name: 'list_keys',
    label: 'List keys',
    description:
      'List an account\'s encryption keys and certificates across both standards ' +
      '(OpenPGP and S/MIME): fingerprint, email binding, standard, key type, ' +
      'origin, whether a private half exists, and timestamps. Public view only: ' +
      'no key material crosses the answer.',
    parameters: Type.Object({
      account: Type.String(),
    }),
    execute: (toolCallId, params) =>
      runGated(deps, toolCallId, 'list_keys', { account: params.account }, () =>
        execCli(
          deps,
          buildCliArgv(deps.correlationId(), ['keys', 'list'], ['--account', params.account]),
        ),
      ),
  });

  const listCollectedKeys = defineMailTool({
    name: 'list_collected_keys',
    label: 'List collected keys',
    description:
      'List the peer keys the account collected from incoming mail (Autocrypt and ' +
      'similar discovery) and has not reviewed yet, newest first: the staging row ' +
      'id, the peer address, the standard, the fingerprint, and the discovery ' +
      'source. Review each with promote_key or reject_key; the fingerprint is ' +
      'shown so the user can verify it out of band.',
    parameters: Type.Object({
      account: Type.String(),
    }),
    execute: (toolCallId, params) =>
      runGated(deps, toolCallId, 'list_collected_keys', { account: params.account }, () =>
        execCli(
          deps,
          buildCliArgv(deps.correlationId(), ['keys', 'collected'], ['--account', params.account]),
        ),
      ),
  });

  const importKey = defineMailTool({
    name: 'import_key',
    label: 'Import a public key',
    description:
      'Import a peer\'s PUBLIC key or certificate into the account\'s key store: ' +
      'an armored OpenPGP public cert or an S/MIME PEM certificate, pasted as ' +
      'text. The user approves the card before anything is stored. Secret-bearing ' +
      'material is refused: private-key import stays desktop, where the vault ' +
      'master key lives. The import records an unverified trust decision, so the ' +
      'key is encryption-qualified straight away.',
    parameters: Type.Object({
      account: Type.String(),
      material: Type.String(),
      standard: Type.Optional(STANDARD),
    }),
    execute: (toolCallId, params) => {
      const args: Record<string, unknown> = { account: params.account, material: params.material };
      const flags: string[] = ['--account', params.account, '--plain', params.material];
      if (params.standard !== undefined) {
        args.standard = params.standard;
        flags.push('--standard', params.standard);
      }
      return runGated(deps, toolCallId, 'import_key', args, () =>
        execCli(deps, buildCliArgv(deps.correlationId(), ['keys', 'import'], flags)),
      );
    },
  });

  const promoteKey = defineMailTool({
    name: 'promote_key',
    label: 'Accept a collected key',
    description:
      'Accept a collected (staged) peer key into the account\'s key store and ' +
      'record the trust decision that makes it send-usable. The user approves ' +
      'the card first; offer the fingerprint so they can verify it out of band ' +
      'and pass verified: true only when they confirmed it. Accepting without ' +
      'verification never lowers a decision already made. The acceptance binds ' +
      'email when given, else the staged row\'s peer address.',
    parameters: Type.Object({
      account: Type.String(),
      standard: STANDARD,
      fingerprint: Type.String(),
      email: Type.Optional(Type.String()),
      verified: Type.Optional(Type.Boolean()),
    }),
    execute: (toolCallId, params) => {
      const verified = params.verified ?? false;
      const args: Record<string, unknown> = {
        account: params.account,
        standard: params.standard,
        fingerprint: params.fingerprint,
        verified,
      };
      const flags: string[] = [
        '--account',
        params.account,
        '--standard',
        params.standard,
        '--fingerprint',
        params.fingerprint,
      ];
      if (params.email !== undefined) {
        args.email = params.email;
        flags.push('--email', params.email);
      }
      if (verified) flags.push('--verified');
      return runGated(deps, toolCallId, 'promote_key', args, () =>
        execCli(deps, buildCliArgv(deps.correlationId(), ['keys', 'promote'], flags)),
      );
    },
  });

  const rejectKey = defineMailTool({
    name: 'reject_key',
    label: 'Reject a collected key',
    description:
      'Reject a collected (staged) peer key: remove the staging row and record a ' +
      'rejected decision, so the key stays refused in future. The user approves ' +
      'the card first. The rejection binds email when given, else the staged ' +
      'row\'s peer address; a later acceptance without verification never ' +
      'overturns it.',
    parameters: Type.Object({
      account: Type.String(),
      id: Type.Number(),
      standard: STANDARD,
      fingerprint: Type.String(),
      email: Type.Optional(Type.String()),
    }),
    execute: (toolCallId, params) => {
      const args: Record<string, unknown> = {
        account: params.account,
        id: params.id,
        standard: params.standard,
        fingerprint: params.fingerprint,
      };
      const flags: string[] = [
        '--account',
        params.account,
        '--id',
        String(params.id),
        '--standard',
        params.standard,
        '--fingerprint',
        params.fingerprint,
      ];
      if (params.email !== undefined) {
        args.email = params.email;
        flags.push('--email', params.email);
      }
      return runGated(deps, toolCallId, 'reject_key', args, () =>
        execCli(deps, buildCliArgv(deps.correlationId(), ['keys', 'reject'], flags)),
      );
    },
  });

  return [listKeys, listCollectedKeys, importKey, promoteKey, rejectKey];
}
