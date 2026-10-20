// SPDX-License-Identifier: GPL-3.0-only
// The folders wire, pinned against the app protocol's serde: every carrying
// verb crosses under the `folders` tag with its snake_case key in the
// struct's own field order, the optional `parent` absent rather than null,
// and the move batch under its own `move_messages` tag.
import { describe, expect, it } from 'vitest';

import { intents } from '@/services/ipc';

describe('the folder intent family', () => {
  it('carries the rename under the folders tag', () => {
    expect(JSON.stringify(intents.renameFolder('work', 'Projects', 'Clients'))).toBe(
      '{"folders":{"rename":{"account":"work","folder":"Projects","name":"Clients"}}}',
    );
  });

  it('carries the move with its parent, and without one for the root', () => {
    expect(JSON.stringify(intents.moveFolder('work', 'Acme', 'Clients'))).toBe(
      '{"folders":{"move":{"account":"work","folder":"Acme","parent":"Clients"}}}',
    );
    expect(JSON.stringify(intents.moveFolder('work', 'Acme'))).toBe(
      '{"folders":{"move":{"account":"work","folder":"Acme"}}}',
    );
  });

  it('carries the trash and the permanent delete', () => {
    expect(JSON.stringify(intents.trashFolder('work', 'Projects'))).toBe(
      '{"folders":{"trash":{"account":"work","folder":"Projects"}}}',
    );
    expect(JSON.stringify(intents.deleteFolder('work', 'Projects'))).toBe(
      '{"folders":{"delete":{"account":"work","folder":"Projects"}}}',
    );
  });
});

describe('the move-to-folder batch', () => {
  it('carries the rows and the destination under the move_messages tag', () => {
    expect(
      JSON.stringify(
        intents.moveMessages(
          [
            { account: 'work', key: 'm1' },
            { account: 'work', key: 'm2' },
          ],
          { account: 'work', key: 'Archive' },
        ),
      ),
    ).toBe(
      '{"move_messages":{"rows":[{"account":"work","key":"m1"},{"account":"work","key":"m2"}],"folder":{"account":"work","key":"Archive"}}}',
    );
  });
});
