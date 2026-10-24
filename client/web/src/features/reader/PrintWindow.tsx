// SPDX-License-Identifier: GPL-3.0-only
// The print window's root: the print-optimised document the web renders and
// prints itself (window.print(), the platform dialog the webview opens; a
// host print verb exists in Tauri but on Windows it is window.print()
// evaluated by the host, so keeping print in the web adds no permission).
// The document is the sanitised reading view reused verbatim, never a
// second sanitiser: the subject, the header lines, the body at the standing
// zoom with its remote placeholders still inert (a printed page loads no
// remote images), and the attachment names. The window closes once the
// dialog settles, and Escape closes it if the dialog never came.

import { useEffect, useRef } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { createListenerRegistry, isTauri, pullSurface, type Surface } from '@/events/surfaceEvents';
import { formatReadingDate } from '@/components/reading-pane/ReadingHeader';
import { READING_CSP_GATED, READING_ZOOM_BASE_PX, wrapBodyDocument } from '@/components/reading-pane/MessageHtml';
import { dispatch, intents, reading, settings, type MessageRef } from '@/services/ipc';
import { useAppearanceStore } from '@/services/appearance';
import { useReadingStore } from '@/stores/readingStore';
import { useSettingsStore } from '@/features/settings/settingsStore';

interface PrintWindowProps {
  /** The message this window prints, from the URL. */
  message: MessageRef;
}

/** Whether the reading snapshot holds this window's message. */
function holdsMessage(
  snapshot: ReturnType<typeof useReadingStore.getState>['snapshot'],
  message: MessageRef,
): boolean {
  return (
    snapshot !== null &&
    snapshot.message.account === message.account &&
    snapshot.message.key === message.key
  );
}

export function PrintWindow({ message }: PrintWindowProps) {
  const snapshot = useReadingStore((s) => s.snapshot);
  const zoom = useAppearanceStore((s) => s.readingZoom);
  const hydrated = useRef(false);
  const printed = useRef(false);

  // One hydration per window, StrictMode safe: open the message through the
  // shared path (a no-op second mark-read for an already-read message),
  // then pull the snapshot and the clock the header formats by.
  useEffect(() => {
    if (hydrated.current) {
      return;
    }
    hydrated.current = true;
    if (!isTauri) {
      return;
    }
    void (async () => {
      try {
        await dispatch(intents.openMessage(message));
        const [preferences, openReading] = await Promise.all([settings(), reading()]);
        useSettingsStore.getState().applySnapshot(preferences);
        useReadingStore.getState().applySnapshot(openReading);
      } catch (err) {
        console.error('print-window: the window hydration failed', err);
      }
    })();
  }, [message]);

  useEffect(() => {
    if (!isTauri) {
      return;
    }
    const registry = createListenerRegistry();
    void registry.safeListen<{ surface: Surface }>(
      'surface_changed',
      (event) => {
        const { surface } = event.payload;
        if (surface === 'reading' || surface === 'settings') {
          void pullSurface(surface);
        }
      },
      'surface_changed',
    );
    return () => registry.disposeAll();
  }, []);

  // Print once the document stands, then close after the platform dialog
  // settles. The guard holds the print to the first landing: a republished
  // snapshot must not open a second dialog over the first.
  useEffect(() => {
    if (printed.current || !holdsMessage(snapshot, message)) {
      return;
    }
    printed.current = true;
    window.print();
  }, [snapshot, message]);

  // The dialog settling closes the window; Escape is the way out when no
  // dialog came at all.
  useEffect(() => {
    const close = (): void => {
      void getCurrentWindow()
        .close()
        .catch(() => undefined);
    };
    window.addEventListener('afterprint', close);
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        close();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('afterprint', close);
      window.removeEventListener('keydown', onKey);
    };
  }, []);

  // The header lines format by the clock setting the hydration pulled.
  const twentyFour = useSettingsStore((s) => s.settings.twenty_four_clock);
  const date = formatReadingDate(snapshot?.sent_at ?? '', twentyFour);

  return (
    <div className="flex h-full flex-col bg-white text-black">
      {snapshot !== null ? (
        <div className="mx-auto flex w-full max-w-[680px] flex-1 flex-col overflow-y-auto px-8 py-6">
          <h1 className="text-[22px] font-semibold leading-tight">{snapshot.subject === '' ? '(no subject)' : snapshot.subject}</h1>
          {snapshot.from_name !== '' && <p className="mt-2 text-sm">{snapshot.from_name}</p>}
          {snapshot.from_address !== '' && <p className="text-sm">{snapshot.from_address}</p>}
          {date !== '' && <p className="text-sm text-neutral-600">{date}</p>}
          {snapshot.html !== undefined ? (
            <iframe
              data-testid="print-body"
              title="Printed message body"
              sandbox="allow-same-origin"
              csp={READING_CSP_GATED}
              srcDoc={wrapBodyDocument(snapshot.html, zoom)}
              // The printed body rides the reading pane's own instrument: a
              // scriptless sandboxed frame over the sanitised fragment, its
              // CSP the gated constants, so the sender's styling renders and
              // no script and no remote load ever crosses print.
              className="min-h-0 w-full flex-1 border-0 bg-white"
            />
          ) : snapshot.plain !== undefined ? (
            <pre
              data-testid="print-body"
              style={{ fontSize: `${READING_ZOOM_BASE_PX[zoom]}px` }}
              className="min-h-0 flex-1 whitespace-pre-wrap py-4 font-sans"
            >
              {snapshot.plain}
            </pre>
          ) : (
            <p className="py-4 text-sm text-neutral-500">This message has no readable body.</p>
          )}
          {(snapshot.attachments ?? []).length > 0 && (
            <div className="mt-4 border-t border-neutral-300 pt-3">
              <p className="text-xs font-medium uppercase tracking-wide text-neutral-500">
                Attachments
              </p>
              <ul className="mt-1 text-sm">
                {(snapshot.attachments ?? []).map((attachment) => (
                  <li key={attachment.id}>{attachment.file_name}</li>
                ))}
              </ul>
            </div>
          )}
        </div>
      ) : (
        <div
          role="status"
          className="flex flex-1 items-center justify-center text-sm text-neutral-500"
        >
          Preparing the document
        </div>
      )}
    </div>
  );
}
