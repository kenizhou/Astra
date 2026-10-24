// SPDX-License-Identifier: GPL-3.0-only
// The About detail: the version and the attributions, the core's answer
// straight from the snapshot, never the web's own guess. The one category
// that holds no setting: it is what a support answer needs quoted back.
// The copy renders through the strings module, the first localised set's
// own third member.

import { useSettingsStore } from './settingsStore';
import { currentStrings } from './strings';

/** The About page of the settings screen. */
export function AboutDetail() {
  const about = useSettingsStore((s) => s.about);
  const strings = currentStrings().about;

  return (
    <section aria-label="About" className="flex flex-col gap-4">
      <h2 className="text-sm font-semibold text-foreground">{strings.title}</h2>
      <p className="text-xs text-foreground">{strings.appName}</p>
      <p className="text-xs text-muted-foreground">
        {strings.versionLabel} {about.version}
      </p>
      {about.attributions !== undefined && about.attributions.length > 0 && (
        <div className="flex flex-col gap-1">
          <h3 className="text-xs font-medium text-foreground">{strings.attributionsLabel}</h3>
          <ul className="flex flex-col gap-0.5">
            {about.attributions.map((line) => (
              <li key={line} className="text-xs text-muted-foreground">
                {line}
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
