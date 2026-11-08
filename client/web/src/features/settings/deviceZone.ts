// SPDX-License-Identifier: GPL-3.0-only
// The device zone's web-side arm: the browser's own resolution, reported
// through the core's device-zone arm once at mount. The core owns the
// validation, the once-per-value latch, and the picked-zone refusal, so a
// report that changes nothing publishes nothing; this side only resolves
// and asks. The host's boot report answers the same arm first, so the two
// resolutions of one device never nag twice.

import { dispatch, intents } from '@/services/ipc';

/**
 * The device's own zone as the browser resolves it, or null when the
 * standard formatter answers nothing usable.
 */
export function resolvedDeviceZone(): string | null {
  let zone: string | undefined;
  try {
    zone = new Intl.DateTimeFormat().resolvedOptions().timeZone;
  } catch {
    return null;
  }
  return typeof zone === 'string' && zone !== '' ? zone : null;
}

/**
 * Reports the browser's resolved zone through the core arm, once at the
 * app's mount; a resolution that answers nothing reports nothing, and a
 * refusal lands in the console without breaking the mount.
 */
export function reportDeviceZone(): void {
  const zone = resolvedDeviceZone();
  if (zone === null) {
    return;
  }
  void dispatch(intents.reportDeviceZone(zone)).catch((err: unknown) => {
    console.error('device-zone: the report refused', err);
  });
}
