// SPDX-License-Identifier: GPL-3.0-only
/**
 * The permission bridge, ported from the kylins sidecar. The sidecar
 * consults the host-delivered policy table before running a tool; tools
 * that need approval suspend here while a permission_request envelope
 * travels to the host, and the host's permission_response resolves the
 * suspended promise via respond.
 *
 * Security default: the lookup FAILS CLOSED. A tool absent from the table
 * maps to needs-approval, never auto; a malformed level likewise. Only an
 * explicit table grant (level auto, or the allowlist flag) lets a call run
 * without asking. No promise can leak: every suspended request has exactly
 * three exits, respond resolves it, rejectAll rejects it, or a failed
 * envelope write rejects it immediately (the host never saw the request,
 * so waiting would hang forever).
 */
import { writeEnvelope, type OutEnvelope, type PermissionPolicy } from '../protocol';

export type { PermissionPolicy };
export type RiskLevel = PermissionPolicy['level'];

/** Gate decision consumed by the tool layer before executing a call. */
export type GateDecision = 'auto' | 'notify' | 'needs-approval';

export interface ApprovalRequest {
  toolCallId: string;
  tool: string;
  level: 'confirm' | 'confirm_preview';
  args: Record<string, unknown>;
  preview?: unknown;
}

export interface ApprovalOutcome {
  approved: boolean;
  alwaysAllow: boolean;
}

export interface PermissionBridgeOptions {
  /** Live view over the policy table (init fills it in place). */
  policy: () => Record<string, PermissionPolicy>;
  /** Id generator. Defaults to a pid-prefixed counter so a restarted
   *  sidecar never re-issues an id the host may still hold. */
  nextId?: () => string;
  /** Envelope writer seam; production leaves it unset and gets the stdout
   *  writeEnvelope, tests capture envelopes in an array. */
  write?: (env: OutEnvelope) => void;
}

interface Pending {
  resolve: (outcome: ApprovalOutcome) => void;
  reject: (err: Error) => void;
  toolCallId: string;
}

function defaultNextId(): () => string {
  let n = 0;
  return () => `perm-${process.pid}-${++n}`;
}

export class PermissionBridge {
  private readonly policyFn: () => Record<string, PermissionPolicy>;
  private readonly nextIdFn: () => string;
  private readonly writeFn: (env: OutEnvelope) => void;
  private readonly pending = new Map<string, Pending>();

  constructor(opts: PermissionBridgeOptions) {
    this.policyFn = opts.policy;
    this.nextIdFn = opts.nextId ?? defaultNextId();
    this.writeFn = opts.write ?? writeEnvelope;
  }

  /** Map a tool's policy entry to a gate decision. allowlist=true
   *  short-circuits to 'auto' regardless of level. Unknown tools and
   *  malformed levels fail closed to 'needs-approval', never auto. */
  check(tool: string): GateDecision {
    const entry: PermissionPolicy | undefined = this.policyFn()[tool];
    if (!entry) return 'needs-approval';
    if (entry.allowlist) return 'auto';
    switch (entry.level) {
      case 'auto':
        return 'auto';
      case 'notify':
        return 'notify';
      case 'confirm':
      case 'confirm_preview':
        return 'needs-approval';
      default:
        // Malformed host policy (a hand-edited row, a future version's
        // level): fail closed, never auto.
        return 'needs-approval';
    }
  }

  /** The approval level to request for a tool. Only meaningful when check()
   *  returned needs-approval: a confirm_preview entry asks the host to
   *  render a preview payload; anything else degrades to plain confirm. */
  levelOf(tool: string): 'confirm' | 'confirm_preview' {
    const entry: PermissionPolicy | undefined = this.policyFn()[tool];
    return entry?.level === 'confirm_preview' ? 'confirm_preview' : 'confirm';
  }

  /** Emit a permission_request envelope and suspend until the host answers
   *  (or rejectAll / a write failure rejects). */
  requestApproval(req: ApprovalRequest): Promise<ApprovalOutcome> {
    const id = this.nextIdFn();
    const promise = new Promise<ApprovalOutcome>((resolve, reject) => {
      this.pending.set(id, { resolve, reject, toolCallId: req.toolCallId });
    });
    const env: OutEnvelope = {
      type: 'permission_request',
      id,
      tool_call_id: req.toolCallId,
      tool: req.tool,
      level: req.level,
      args: req.args,
    };
    if (req.preview !== undefined) env.preview = req.preview;
    try {
      this.writeFn(env);
    } catch (e) {
      // The host never saw the request: remove the pending entry and reject
      // loudly instead of leaving the promise suspended forever.
      const p = this.pending.get(id);
      this.pending.delete(id);
      p?.reject(e instanceof Error ? e : new Error(String(e)));
    }
    return promise;
  }

  /** Resolve a suspended request. Unknown ids (already answered, aborted, or
   *  from a previous process) are warned on stderr, never thrown: the
   *  host's one-shot semantics make duplicates expected. */
  respond(id: string, approved: boolean, alwaysAllow: boolean): void {
    const p = this.pending.get(id);
    if (!p) {
      process.stderr.write(
        `sidecar: permission response for unknown id ${id} (approved=${approved}); ignoring\n`,
      );
      return;
    }
    this.pending.delete(id);
    p.resolve({ approved, alwaysAllow });
  }

  /** Reject every suspended approval (prompt abort, harness death). Clears
   *  the map so no promise leaks and late responses become unknown ids. */
  rejectAll(reason: string): void {
    const all = [...this.pending.values()];
    this.pending.clear();
    for (const p of all) p.reject(new Error(reason));
  }
}
