// SPDX-License-Identifier: GPL-3.0-only
// The diary's device-zone offer: when the core's report left an offer
// standing, a prompt over the calendar names the offered zone beside Use
// it and Keep {current}. The accept dispatches the acceptance arm (the
// core refuses it when a zone was picked, and the picked zone stands);
// the keep dismisses the standing offer, and the core's latches keep the
// dismissed value from offering again.

import { dispatch, intents } from '@/services/ipc';

import { useSettingsStore } from '@/features/settings/settingsStore';

/** The device-zone offer prompt over the diary. */
export function DeviceZoneOffer() {
  const offer = useSettingsStore((s) => s.deviceZoneOffer);
  const zone = useSettingsStore((s) => s.settings.zone);

  if (offer === null) {
    return null;
  }

  return (
    <div
      role="dialog"
      aria-label="Device time zone offer"
      className="flex shrink-0 items-center gap-2 border-b border-border-subtle bg-surface px-3 py-1.5 text-xs"
    >
      <span className="min-w-0 flex-1 text-foreground">
        Your device is set to {offer}. Use it for the calendar?
      </span>
      <button
        type="button"
        onClick={() => void dispatch(intents.acceptDeviceZone(offer))}
        className="whitespace-nowrap rounded-[var(--radius-xs)] bg-primary px-2 py-0.5 font-medium text-primary-foreground transition-colors hover:bg-primary-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        Use it
      </button>
      <button
        type="button"
        onClick={() => void dispatch(intents.dismissDeviceZone())}
        className="whitespace-nowrap rounded-[var(--radius-xs)] px-2 py-0.5 text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        Keep {zone}
      </button>
    </div>
  );
}
