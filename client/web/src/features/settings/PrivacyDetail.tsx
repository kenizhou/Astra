// SPDX-License-Identifier: GPL-3.0-only
// The Privacy detail: the plain statement of what stays on the device,
// what the AI features send, and what plugins may ask for, written in the
// house voice, no legalese; the analytics switch, whose off click is the
// one-click withdrawal and whose on click is the consent, one control;
// and the plugin registry link, which lands the settings screen on the
// Plugins category where each plugin's grants are listed.

import { dispatch, intents } from '@/services/ipc';

import { useSettingsStore } from './settingsStore';
import { currentStrings } from './strings';

/** The Privacy page of the settings screen. */
export function PrivacyDetail() {
  const settings = useSettingsStore((s) => s.settings);
  const requestCategory = useSettingsStore((s) => s.requestCategory);
  const strings = currentStrings().privacy;

  return (
    <section aria-label="Privacy settings" className="flex flex-col gap-4">
      <h2 className="text-sm font-semibold text-foreground">{strings.title}</h2>
      <div aria-label="Privacy statement" className="flex max-w-[520px] flex-col gap-2">
        <p className="text-xs text-muted-foreground">{strings.staysOnDevice}</p>
        <p className="text-xs text-muted-foreground">{strings.aiSends}</p>
        <p className="text-xs text-muted-foreground">{strings.pluginSentence}</p>
      </div>
      <div className="flex flex-col gap-1">
        <label className="flex items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            aria-label={strings.analyticsToggle}
            checked={settings.analytics_consented ?? false}
            onChange={(event) =>
              void dispatch(intents.setAnalyticsConsented(event.target.checked))
            }
          />
          {strings.analyticsToggle}
        </label>
        <p aria-label="Analytics consent note" className="max-w-[420px] text-xs text-muted-foreground">
          {strings.analyticsNote}
        </p>
      </div>
      <div>
        <button
          type="button"
          onClick={() => requestCategory('plugins')}
          className="rounded-lg border border-border px-3 py-1.5 text-xs text-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {strings.pluginLink}
        </button>
      </div>
    </section>
  );
}
