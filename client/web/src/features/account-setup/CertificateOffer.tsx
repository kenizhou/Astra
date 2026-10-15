// SPDX-License-Identifier: GPL-3.0-only
// The rejected certificate's offer: the server, its fingerprint, and the
// name it claims, with the sentence bounding what accepting trusts.

import type { RejectedCertificateView } from '@/services/ipc';

/** The one-server exception offer, shown after a rejected certificate. */
export function CertificateOffer({ certificate }: { certificate: RejectedCertificateView }) {
  return (
    <div className="rounded-[var(--radius-xs)] border border-warning bg-accent p-3 text-xs">
      <p className="font-medium text-foreground">
        The server&apos;s certificate could not be verified
      </p>
      <dl className="mt-2 grid gap-1">
        <div className="flex justify-between gap-2">
          <dt className="text-muted-foreground">Server</dt>
          <dd className="font-mono">{certificate.server_name}</dd>
        </div>
        <div className="flex justify-between gap-2">
          <dt className="text-muted-foreground">Fingerprint</dt>
          <dd className="break-all font-mono">{certificate.sha256}</dd>
        </div>
        {certificate.subject_common_name !== null && (
          <div className="flex justify-between gap-2">
            <dt className="text-muted-foreground">Claims to be</dt>
            <dd className="font-mono">{certificate.subject_common_name}</dd>
          </div>
        )}
      </dl>
      <p className="mt-2 text-muted-foreground">
        Accepting trusts this one certificate for this one server, and only here.
      </p>
    </div>
  );
}
