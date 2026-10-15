// SPDX-License-Identifier: GPL-3.0-only
// The real-day export: reads one account's inbox through astra-cli and maps
// it into the fixture shapes the design harness serves, so a design review
// can triage a real day of mail instead of the recorded demo rows.
//
//   node scripts/dev/export-inbox.mjs --account work [--folder INBOX] [--limit 50]
//
// Writes client/web/src/services/__fixtures__/realday_inbox.json and prints
// the driver snippet that loads it into a running fixtures server. Needs a
// built CLI (cargo build -p astra-cli) and the account configured (or the
// docker harness: scripts/dev/harness.sh up && eval "$(scripts/dev/harness.sh env)").
import { execFile } from 'node:child_process';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const run = promisify(execFile);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const cli = path.join(root, 'target', 'debug', process.platform === 'win32' ? 'astra-cli.exe' : 'astra-cli');
const out = path.join(root, 'client', 'web', 'src', 'services', '__fixtures__', 'realday_inbox.json');

function arg(name, fallback) {
  const at = process.argv.indexOf(`--${name}`);
  return at === -1 ? fallback : process.argv[at + 1];
}
const account = arg('account', undefined);
if (!account) {
  console.error('export-inbox: --account <id> is required (see astra-cli accounts list --json)');
  process.exit(1);
}
const folder = arg('folder', 'INBOX');
const limit = Number(arg('limit', '50'));

async function cliJson(args) {
  const { stdout } = await run(cli, [...args, '--json'], { maxBuffer: 16 * 1024 * 1024 });
  return JSON.parse(stdout);
}

// The CLI envelope prints one line when piped: find the object itself.
function unwrap(envelope) {
  return envelope && typeof envelope === 'object' && 'data' in envelope ? envelope.data : envelope;
}

function folderRole(name) {
  const low = name.toLowerCase();
  if (low === 'inbox') return 'inbox';
  if (low === 'sent') return 'sent';
  if (low === 'drafts') return 'drafts';
  if (low === 'trash' || low === 'junk' || low === 'spam') return low === 'trash' ? 'trash' : 'junk';
  if (low === 'archive') return 'archive';
  return undefined;
}

const foldersAnswer = unwrap(await cliJson(['folders', 'list', '--account', account]));
const folders = (foldersAnswer.folders ?? []).map((f) => {
  const depth = f.name.split('/').length - 1;
  return {
    key: f.id,
    name: f.name.split('/').pop(),
    ...(folderRole(f.name) ? { role: folderRole(f.name) } : {}),
    ...(f.parent ? { parent: f.parent } : {}),
    unread: f.unread_count ?? 0,
    depth,
    has_children: false,
    expanded: false,
  };
});
const parentNames = new Set(folders.map((f) => f.parent).filter(Boolean));
for (const f of folders) {
  f.has_children = parentNames.has(f.key);
}

const listAnswer = unwrap(await cliJson(['messages', 'list', '--account', account, '--folder', folder, '--limit', String(limit)]));
const rows = (listAnswer.messages ?? []).map((m) => ({
  key: { message: { account, key: m.key } },
  account_id: account,
  sender_name: m.from_name ?? '',
  sender_address: m.from ?? '',
  subject: m.subject ?? '',
  sent_at: m.date ?? '',
  unread: Boolean(m.unread),
  flagged: false,
  has_attachments: Boolean(m.has_attachment),
}));

const snapshot = {
  account_folders: [
    {
      account_id: account,
      display_name: account,
      expanded: true,
      folders,
    },
  ],
  rows,
  visible_limit: rows.length,
};

await writeFile(out, JSON.stringify(snapshot, null, 2) + '\n');
console.log(`export-inbox: wrote ${rows.length} rows across ${folders.length} folders to ${path.relative(root, out)}`);
console.log('Load it into a running fixtures server with:');
console.log(`  fetch('/src/services/__fixtures__/realday_inbox.json').then(r => r.json()).then(j => {`);
console.log(`    window.__ASTRA_FIXTURES__.setFixtureResponse('mailbox_list', j);`);
console.log(`    window.__ASTRA_FIXTURES__.pushFixtureEvent('surface_changed', { surface: 'mailbox_list' });`);
console.log(`  })`);
