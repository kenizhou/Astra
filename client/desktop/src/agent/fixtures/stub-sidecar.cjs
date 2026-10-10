// SPDX-License-Identifier: GPL-3.0-only
// The stub sidecar the relay, supervisor, and command tests spawn in place of
// the real bundle: one Node process speaking the exact JSONL protocol over
// stdio, driven by a mode argument. echo (the default) answers the init
// handshake and echoes a prompt back with a state event, a text delta, an
// llm_call envelope, and done; ask suspends the prompt on a
// permission_request until a permission_response arrives; hang streams one
// delta and settles only on abort; die exits at once (the supervisor's death
// path); die-after-ready answers the handshake first. The ping command
// reports the environment variable names the host left in the spawn, so a
// test can prove the scrub.
'use strict';

const readline = require('node:readline');

const mode = process.argv[2] || 'echo';

if (mode === 'die') {
  process.exit(0);
}

let ready = false;
const pendingAsks = new Map();

function writeLine(value) {
  try {
    process.stdout.write(JSON.stringify(value) + '\n');
  } catch (e) {
    if (e && e.code === 'EPIPE') {
      process.exit(0);
    }
    throw e;
  }
}

function answerReady() {
  ready = true;
  writeLine({ type: 'ready', models: ['stub-1'], harnesses: ['mock', 'pi'] });
}

function llmCall(cmd) {
  return {
    type: 'llm_call',
    call: {
      session_id: cmd.session_id,
      prompt_id: cmd.id,
      correlation_id: cmd.correlation_id,
      model: 'stub-1',
      tier: 'chat',
      caller: 'agent-chat',
      prompt_tokens: 7,
      completion_tokens: 3,
      cache_read_tokens: 0,
      cache_write_tokens: 0,
      duration_ms: 4,
      status: 'ok',
    },
  };
}

function runEchoPrompt(cmd) {
  writeLine({
    type: 'event',
    session_id: cmd.session_id,
    event: { kind: 'state', state: 'thinking' },
  });
  writeLine({
    type: 'event',
    session_id: cmd.session_id,
    event: { kind: 'text_delta', text: `echo: ${cmd.text}` },
  });
  writeLine(llmCall(cmd));
  writeLine({ type: 'done', id: cmd.id });
}

function runAskPrompt(cmd) {
  const askId = `ask-${process.pid}-1`;
  const settled = { value: null };
  pendingAsks.set(askId, settled);
  writeLine({
    type: 'permission_request',
    id: askId,
    tool_call_id: 'stub-call-1',
    tool: 'save_draft',
    level: 'confirm',
    args: { account: 'acct-1', subject: `Re: ${cmd.text}` },
  });
  const finish = () => {
    const approved = settled.value === true;
    writeLine({
      type: 'tool_result',
      tool_call_id: 'stub-call-1',
      tool: 'save_draft',
      status: approved ? 'ok' : 'rejected',
    });
    writeLine({
      type: 'event',
      session_id: cmd.session_id,
      event: { kind: 'text_delta', text: approved ? 'draft saved' : 'refused' },
    });
    writeLine(llmCall(cmd));
    writeLine({ type: 'done', id: cmd.id });
  };
  settled.finish = finish;
}

function handleCommand(cmd) {
  switch (cmd.type) {
    case 'init':
      if (mode === 'die-after-ready') {
        answerReady();
        process.exit(0);
      }
      answerReady();
      break;
    case 'prompt':
      if (!ready) {
        writeLine({ type: 'error', id: cmd.id, message: 'prompt before init' });
        break;
      }
      if (mode === 'ask') {
        runAskPrompt(cmd);
      } else if (mode === 'hang') {
        writeLine({
          type: 'event',
          session_id: cmd.session_id,
          event: { kind: 'text_delta', text: 'thinking aloud' },
        });
      } else if (mode === 'die-mid') {
        writeLine({
          type: 'event',
          session_id: cmd.session_id,
          event: { kind: 'text_delta', text: 'cut short' },
        });
        process.exit(0);
      } else {
        runEchoPrompt(cmd);
      }
      break;
    case 'abort':
      if (mode === 'hang') {
        writeLine({ type: 'done', id: cmd.id });
      }
      break;
    case 'command':
      if (cmd.name === 'ping') {
        writeLine({
          type: 'result',
          id: cmd.id,
          value: { ok: true, env: Object.keys(process.env).sort() },
        });
      } else {
        writeLine({
          type: 'error',
          id: cmd.id,
          message: `unknown command: ${cmd.name}`,
        });
      }
      break;
    case 'permission_response':
      if (mode === 'ask') {
        const settled = pendingAsks.get(cmd.id);
        if (settled && settled.finish) {
          settled.value = cmd.approved;
          settled.finish();
        }
      }
      break;
    default:
      break;
  }
}

const rl = readline.createInterface({ input: process.stdin });
rl.on('line', (line) => {
  let cmd;
  try {
    cmd = JSON.parse(line);
  } catch {
    process.stderr.write(`stub-sidecar: skipping unparsable line: ${line}\n`);
    return;
  }
  handleCommand(cmd);
});
rl.on('close', () => process.exit(0));
process.stdout.on('error', (e) => {
  if (e.code === 'EPIPE') {
    process.exit(0);
  }
});
