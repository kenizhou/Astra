// SPDX-License-Identifier: GPL-3.0-only
// The plugin frame: a plugin's declared rich page in a sandboxed iframe over
// the astra-plugin:// scheme, bridged to its worker by pluginBridge. The
// frame is the whole of a plugin page's reach: the CSP (served by the scheme)
// allows the bundle's own assets and nothing remote, and the bridge answers
// only the plugin's declared custom methods.

import { useEffect, useRef } from 'react';

import type { PageDescriptor, PluginRow } from '@/services/plugins';

import { bridgeFrame } from './pluginBridge';

export function PluginFrame({ plugin, page }: { plugin: PluginRow; page: PageDescriptor }) {
  const frameRef = useRef<HTMLIFrameElement>(null);
  useEffect(() => {
    const frame = frameRef.current;
    if (frame === null) {
      return;
    }
    return bridgeFrame(frame, plugin.id);
  }, [plugin.id]);
  return (
    <iframe
      ref={frameRef}
      title={page.title}
      sandbox="allow-scripts allow-same-origin"
      src={`astra-plugin://${plugin.id}/${page.entry}`}
      className="h-full min-h-[320px] w-full rounded-[var(--radius-xs)] border border-border-subtle bg-card"
    />
  );
}
