// SPDX-License-Identifier: GPL-3.0-only
/**
 * MockHarness: drives the whole stdio protocol from a fixture transcript
 * without contacting any model. Ported from the kylins sidecar, trimmed.
 * When constructed with the tool set and the fixture carries a tool_call,
 * the mock dispatches the named tool for real after the tool_call event
 * (the same execute body the pi harness runs) and emits the tool_result
 * event from the returned details: the e2e driver for the permission flow,
 * where the envelope SEQUENCE is what the tests assert, never model
 * internals.
 */
import { readFileSync } from 'node:fs';
import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import type { MailToolDetails } from '../tools/mailTools';
import type { AgentHarness, HarnessEvent, HarnessSession, PromptCallContext } from './types';

interface MockFixture {
  chunks: string[];
  chunk_delay_ms?: number;
  tool_call?: { name: string; args: unknown };
  /** Echo the received prompt text as the first chunk, so a host-side test
   *  can observe over the wire exactly what the session was asked. */
  echo_prompt?: boolean;
}

const DEFAULT_CHUNK_DELAY_MS = 5;

class MockSession implements HarnessSession {
  private listeners: Array<(e: HarnessEvent) => void> = [];
  private aborted = false;

  constructor(
    private readonly fixturePath?: string,
    private readonly tools?: ToolDefinition[],
  ) {}

  subscribe(cb: (e: HarnessEvent) => void): void {
    this.listeners.push(cb);
  }

  private emit(e: HarnessEvent): void {
    for (const cb of this.listeners) cb(e);
  }

  private loadFixture(text: string): MockFixture {
    if (this.fixturePath) {
      try {
        const parsed = JSON.parse(readFileSync(this.fixturePath, 'utf8')) as MockFixture;
        if (Array.isArray(parsed.chunks)) return parsed;
        process.stderr.write(
          `mock-harness: fixture at ${this.fixturePath} has no chunks array; using defaults\n`,
        );
      } catch (e) {
        process.stderr.write(
          `mock-harness: failed to read fixture ${this.fixturePath}: ${String(e)}; using defaults\n`,
        );
      }
    }
    return {
      chunks: ['Echo: ', text.slice(0, 40), ' (mock harness streaming reply)'],
    };
  }

  /** Sleep in small ticks so abort() interrupts the stream promptly. */
  private async sleep(ms: number): Promise<void> {
    const step = 5;
    let remaining = ms;
    while (remaining > 0 && !this.aborted) {
      await new Promise((r) => setTimeout(r, Math.min(step, remaining)));
      remaining -= step;
    }
  }

  async prompt(text: string, ctx: PromptCallContext): Promise<void> {
    this.aborted = false;
    this.emit({ kind: 'state', state: 'thinking' });
    const fixture = this.loadFixture(text);
    const delay = fixture.chunk_delay_ms ?? DEFAULT_CHUNK_DELAY_MS;
    if (fixture.echo_prompt && !this.aborted) {
      this.emit({ kind: 'text_delta', text });
    }
    for (const chunk of fixture.chunks) {
      if (this.aborted) break;
      await this.sleep(delay);
      if (this.aborted) break;
      this.emit({ kind: 'text_delta', text: chunk });
    }
    if (!this.aborted && fixture.tool_call) {
      // The tool_call event carries the same id the dispatch's tool_result
      // reports, so the two legs pair.
      const toolCallId = 'mock-tc-1';
      this.emit({
        kind: 'tool_call',
        name: fixture.tool_call.name,
        args: fixture.tool_call.args,
        tool_call_id: toolCallId,
      });
      if (this.tools) {
        await this.dispatchTool(fixture.tool_call.name, fixture.tool_call.args, toolCallId, ctx);
      }
    }
    if (!this.aborted) {
      // One synthetic llm_call per completed prompt. An aborted prompt
      // records nothing: a fabricated ok row would be a lie.
      this.emit({
        kind: 'llm_call',
        usage: {
          model: 'mock-1',
          prompt_tokens: text.length,
          completion_tokens: fixture.chunks.join('').length,
          cache_read_tokens: 0,
          cache_write_tokens: 0,
          duration_ms: 0,
          status: 'ok',
        },
      });
    }
    this.emit({ kind: 'state', state: 'done-streaming' });
  }

  /** Run the fixture's tool call through the real tool definition and emit
   *  the tool_result event from its details. A suspended approval inside
   *  execute resolves when the host's permission_response arrives; prompt()
   *  awaits the whole flow. */
  private async dispatchTool(
    name: string,
    args: unknown,
    toolCallId: string,
    ctx: PromptCallContext,
  ): Promise<void> {
    void ctx;
    const tool = this.tools?.find((t) => t.name === name);
    if (!tool) {
      process.stderr.write(`mock-harness: fixture tool_call names an unknown tool: ${name}\n`);
      this.emit({
        kind: 'tool_result',
        tool_call_id: toolCallId,
        name,
        status: 'error',
        output: `mock-harness: unknown tool: ${name}`,
      });
      return;
    }
    try {
      // signal/onUpdate/ctx are unused by the mail tools: the pi-side
      // equivalents are extension plumbing the mock does not have.
      const result = await tool.execute(
        toolCallId,
        args as never,
        undefined,
        undefined,
        undefined as never,
      );
      const details = result.details as MailToolDetails | undefined;
      const event: HarnessEvent = {
        kind: 'tool_result',
        tool_call_id: toolCallId,
        name,
        status: details?.status ?? 'ok',
      };
      if (details?.output !== undefined) event.output = details.output;
      this.emit(event);
    } catch (e) {
      // A foreign tool threw: mirror the pi adapter's degradation.
      this.emit({
        kind: 'tool_result',
        tool_call_id: toolCallId,
        name,
        status: 'error',
        output: e instanceof Error ? e.message : String(e),
      });
    }
  }

  abort(): void {
    // Setting the flag is enough: the stream loop checks it each tick,
    // stops, and resolves; the loop's finally sends the done envelope.
    this.aborted = true;
  }

  async dispose(): Promise<void> {
    this.listeners = [];
  }
}

export class MockHarness implements AgentHarness {
  readonly id = 'mock';

  constructor(
    private readonly opts: { fixturePath?: string; tools?: ToolDefinition[] } = {},
  ) {}

  async createSession(_opts: { harnessOptions?: unknown }): Promise<HarnessSession> {
    // Fault injection for the unhandled-rejection e2e test: sentinel path.
    if (this.opts.fixturePath === 'reject-create-session') {
      throw new Error('mock: createSession rejected (fault injection)');
    }
    return new MockSession(this.opts.fixturePath, this.opts.tools);
  }
}
