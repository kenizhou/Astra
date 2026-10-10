// SPDX-License-Identifier: GPL-3.0-only
/**
 * PiAdapter: the real harness, driving @earendil-works/pi-coding-agent
 * (pinned 0.84.2, the kylins pin). Ported from the kylins adapter, trimmed
 * to chat-only: one model registration from the init envelope's
 * model_configs, no compaction takeover, no channels. Wiring notes carried
 * from the kylins spike:
 *
 * - 0.84.x has no authStorage/modelRegistry options: model plus auth go
 *   through ModelRuntime.create({ authPath, modelsPath }) then
 *   registerProvider(providerId, { baseUrl, apiKey, api, models }) and
 *   modelRuntime.getModel(providerId, modelId). The API key lives in this
 *   process's memory only and is never logged.
 * - Persistence is the coding-agent SessionManager JSONL under the app data
 *   dir's agent directory, never ~/.pi. Session ids are sanitised because
 *   pi writes them verbatim into the JSONL filename and ':' is illegal on
 *   Windows.
 * - The five mail tools mount via customTools; the built-in coding tools
 *   and any auto-discovered extension tools stay disabled by the explicit
 *   tools: allowlist (CHAT_TOOL_ALLOWLIST), so only the five names enable.
 * - The astra-cli skill index rides the system prompt through a
 *   DefaultResourceLoader appendSystemPrompt entry. When a resourceLoader
 *   is supplied, createAgentSession does NOT reload it, so the adapter owns
 *   the reload() call.
 * - usage comes from message_end events' message.usage {input, output,
 *   cacheRead, cacheWrite}; duration is measured from message_start of the
 *   same assistant message.
 */
import {
  createAgentSession,
  DefaultResourceLoader,
  ModelRuntime,
  SessionManager,
  SettingsManager,
  type AgentSession,
  type ToolDefinition,
} from '@earendil-works/pi-coding-agent';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import type { ModelConfig } from '../protocol';
import type { MailToolDetails } from '../tools/mailTools';
import type {
  AgentHarness,
  HarnessEvent,
  HarnessSession,
  PromptCallContext,
} from './types';

/** Provider id used when registering the configured endpoint. */
const PROVIDER_ID = 'astra-agent';

/** G10: the explicit session allowlist, the five mail tools and nothing
 *  else. With tools: provided, pi enables ONLY these names, so the built-in
 *  fs/bash tools and any future extension directory stay off. */
export const CHAT_TOOL_ALLOWLIST: readonly string[] = [
  'list_folders',
  'list_messages',
  'read_message',
  'save_draft',
  'discard_draft',
];

/** Resident skill index appended to every pi system prompt. Short by
 *  design; the full guide sits in the bundle's skills directory. */
const ASTRA_CLI_SKILL_INDEX =
  'astra-cli: the mail operations CLI behind these tools. The mounted tools cover ' +
  'folders, message lists and reads, and draft save and discard; each call is ' +
  'audited against its turn. Sending is a human action in the app and is never ' +
  'available here. Ask before writing anything; the approval cards are the user.';

export interface PiHarnessOptions {
  sessionId?: string;
}

/** Minimal structural views of pi's event payloads (the SDK's own types are
 *  deep generics; only these fields are touched). */
interface PiUsage {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
}
interface PiAssistantMessage {
  role: 'assistant';
  model?: string;
  responseModel?: string;
  usage?: PiUsage;
  stopReason?: string;
  errorMessage?: string;
}
interface PiEvent {
  type: string;
  message?: { role?: string } & Partial<PiAssistantMessage>;
  assistantMessageEvent?: { type: string; delta?: string };
  toolName?: string;
  toolCallId?: string;
  args?: unknown;
  result?: { details?: Partial<MailToolDetails> };
  isError?: boolean;
  success?: boolean;
  finalError?: string;
}

function logLine(msg: string): void {
  process.stderr.write(`pi-adapter: ${msg}\n`);
}

/** Map an Astra session id to a filename-safe pi session id. */
export function toFileId(sessionId: string): string {
  return sessionId.replace(/[^A-Za-z0-9._-]/g, '_');
}

class PiSession implements HarnessSession {
  private listeners: Array<(e: HarnessEvent) => void> = [];
  private unsubscribe: () => void;
  /** Timestamp of the current assistant message's start, for llm_call
   *  duration_ms (message_start to message_end of the same message). */
  private messageStartedAt = 0;

  constructor(private readonly session: AgentSession) {
    this.unsubscribe = session.subscribe((e) => this.onPiEvent(e as PiEvent));
  }

  subscribe(cb: (e: HarnessEvent) => void): void {
    this.listeners.push(cb);
  }

  private emit(e: HarnessEvent): void {
    for (const cb of this.listeners) cb(e);
  }

  private onPiEvent(e: PiEvent): void {
    switch (e.type) {
      case 'message_update': {
        const ame = e.assistantMessageEvent;
        if (ame?.type === 'text_delta' && typeof ame.delta === 'string') {
          this.emit({ kind: 'text_delta', text: ame.delta });
        }
        break;
      }
      case 'message_start': {
        if (e.message?.role === 'assistant') this.messageStartedAt = Date.now();
        break;
      }
      case 'message_end': {
        const m = e.message;
        if (m?.role !== 'assistant') break;
        const usage = m.usage;
        if (!usage) break; // defensive: assistant messages always carry usage
        const failed = m.stopReason === 'error' || m.stopReason === 'aborted';
        const event: HarnessEvent = {
          kind: 'llm_call',
          usage: {
            model: m.responseModel ?? m.model ?? 'unknown',
            prompt_tokens: usage.input,
            completion_tokens: usage.output,
            cache_read_tokens: usage.cacheRead,
            cache_write_tokens: usage.cacheWrite,
            duration_ms: this.messageStartedAt > 0 ? Date.now() - this.messageStartedAt : 0,
            status: failed ? 'error' : 'ok',
          },
        };
        if (failed) {
          event.usage.error =
            m.errorMessage ?? (m.stopReason === 'aborted' ? 'aborted' : 'unknown error');
        }
        this.emit(event);
        break;
      }
      case 'tool_execution_start': {
        if (typeof e.toolName === 'string') {
          this.emit({
            kind: 'tool_call',
            name: e.toolName,
            args: e.args,
            tool_call_id: e.toolCallId ?? '',
          });
        }
        break;
      }
      case 'tool_execution_end': {
        // The audited outcome of a tool call: our tools never throw, the
        // status rides in result.details; a foreign tool without details
        // degrades to pi's isError flag.
        if (typeof e.toolName !== 'string') break;
        const details = e.result?.details;
        const status = details?.status ?? (e.isError ? 'error' : 'ok');
        const event: HarnessEvent = {
          kind: 'tool_result',
          tool_call_id: e.toolCallId ?? '',
          name: e.toolName,
          status,
        };
        if (details?.output !== undefined) event.output = details.output;
        this.emit(event);
        break;
      }
      case 'auto_retry_end': {
        // pi retries failed API calls internally; when the LAST retry fails
        // the run ends without an assistant message, so the panel would
        // otherwise see thinking to done with nothing between. The llm_call
        // rows carry the per-attempt detail; this state event is the
        // user-visible failure signal.
        if (e.success === false) {
          logLine(`prompt failed after retries: ${e.finalError ?? 'unknown error'}`);
          this.emit({ kind: 'state', state: 'error' });
        }
        break;
      }
      default:
        break; // thinking deltas, turn lifecycle, retries: not surfaced
    }
  }

  async prompt(text: string, _ctx: PromptCallContext): Promise<void> {
    this.emit({ kind: 'state', state: 'thinking' });
    await this.session.prompt(text);
    this.emit({ kind: 'state', state: 'done-streaming' });
  }

  abort(): void {
    // pi's abort is async; the session contract is fire-and-forget (the
    // stdio loop's finally sends the done envelope either way).
    this.session.abort().catch((e: unknown) => {
      logLine(`abort failed: ${String(e)}`);
    });
  }

  async dispose(): Promise<void> {
    this.unsubscribe();
    this.listeners = [];
    this.session.dispose();
  }
}

export class PiAdapter implements AgentHarness {
  readonly id = 'pi';
  private readonly agentDir: string;
  private readonly sessionDir: string;
  private runtimePromise: Promise<ModelRuntime> | null = null;

  constructor(
    private readonly opts: {
      /** App data dir from init; pi state lives under <data>/agent. */
      dataDir: string;
      /** tier name to endpoint config, from init's model_configs. */
      modelConfigs: Map<string, ModelConfig>;
      /** The five mail customTools; optional so unit constructions can omit. */
      mailTools?: ToolDefinition[];
    },
  ) {
    this.agentDir = join(opts.dataDir, 'agent', 'pi');
    this.sessionDir = join(opts.dataDir, 'agent', 'sessions');
    mkdirSync(this.agentDir, { recursive: true });
    mkdirSync(this.sessionDir, { recursive: true });
  }

  private runtime(): Promise<ModelRuntime> {
    if (!this.runtimePromise) {
      this.runtimePromise = ModelRuntime.create({
        authPath: join(this.agentDir, 'auth.json'),
        modelsPath: join(this.agentDir, 'models.json'),
      });
    }
    return this.runtimePromise;
  }

  /** The one configured tier from init's model_configs (insertion order).
   *  Null when the host delivered none: the caller fails loudly. */
  private firstTier(): { tier: string; cfg: ModelConfig } | null {
    for (const [tier, cfg] of this.opts.modelConfigs) return { tier, cfg };
    return null;
  }

  /** Register the configured endpoint with the ModelRuntime and resolve the
   *  pi Model. Throws when a configured model cannot be resolved; returns
   *  null when no config was delivered. */
  private async resolveModel(): Promise<{
    tier: string;
    model: NonNullable<ReturnType<ModelRuntime['getModel']>>;
  } | null> {
    const first = this.firstTier();
    if (!first) return null;
    const { tier, cfg } = first;
    const runtime = await this.runtime();
    runtime.registerProvider(PROVIDER_ID, {
      name: 'Astra agent endpoint',
      baseUrl: cfg.base_url,
      // The validator requires a non-empty apiKey even for key-less local
      // servers, so a sentinel stands in when none was delivered.
      apiKey: cfg.api_key && cfg.api_key.length > 0 ? cfg.api_key : 'no-auth',
      api: cfg.api,
      models: [
        {
          id: cfg.model,
          name: cfg.model,
          reasoning: false,
          input: ['text'],
          cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
          contextWindow: 128_000,
          maxTokens: 8_192,
        },
      ],
    });
    const model = runtime.getModel(PROVIDER_ID, cfg.model);
    if (!model) {
      throw new Error(`pi harness: registered model ${cfg.model} not resolvable`);
    }
    return { tier, model };
  }

  /** Restore the persisted SessionManager for an id, or create one. */
  private async sessionManagerFor(sessionId: string): Promise<SessionManager> {
    const id = toFileId(sessionId);
    const infos = await SessionManager.listAll(this.sessionDir);
    const hit = infos.find((i) => i.id === id);
    if (hit) return SessionManager.open(hit.path, this.sessionDir, this.opts.dataDir);
    return SessionManager.create(this.opts.dataDir, this.sessionDir, { id });
  }

  async createSession(opts: { harnessOptions?: unknown }): Promise<HarnessSession> {
    const hOpts = opts.harnessOptions as PiHarnessOptions | undefined;
    const sessionId = hOpts?.sessionId ?? `s-${Date.now()}`;
    const resolved = await this.resolveModel();
    if (!resolved) {
      throw new Error(
        'pi harness: no model config was delivered on init; the host must send model_configs',
      );
    }
    const runtime = await this.runtime();
    const sessionManager = await this.sessionManagerFor(sessionId);
    const settingsManager = SettingsManager.inMemory();
    // Same discovery surface createAgentSession would build by default,
    // plus the resident skill index. A supplied loader is NOT reloaded by
    // createAgentSession; reload() is this adapter's job.
    const resourceLoader = new DefaultResourceLoader({
      cwd: this.opts.dataDir,
      agentDir: this.agentDir,
      settingsManager,
      appendSystemPrompt: [ASTRA_CLI_SKILL_INDEX],
    });
    await resourceLoader.reload();

    const { session, modelFallbackMessage } = await createAgentSession({
      cwd: this.opts.dataDir,
      agentDir: this.agentDir,
      modelRuntime: runtime,
      model: resolved.model,
      sessionManager,
      settingsManager,
      resourceLoader,
      tools: [...CHAT_TOOL_ALLOWLIST],
      customTools: this.opts.mailTools ?? [],
    });
    if (modelFallbackMessage) logLine(modelFallbackMessage);
    return new PiSession(session);
  }
}
