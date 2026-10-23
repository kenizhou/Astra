// SPDX-License-Identifier: GPL-3.0-only
//! The crypto outbox half: the drain, the sending snapshot's crypto rows,
//! and the outbox verbs' routing for them (SP4b spec D3). The submit-side
//! branch lives in [`crate::crypto`]; this is the same split `submit` and
//! `outbox` take over the engine's queue.
//!
//! **Privacy.** A message is the user's. Nothing here logs its content,
//! its recipients, or its subject; counts and ids only.

use astra_crypto_store::{
    list_crypto_outbox, list_due_crypto_sends, mark_crypto_failed, mark_crypto_sending,
    mark_crypto_unconfirmed, requeue_crypto_send, settle_crypto_send, SubmitFailureKind,
};
use astra_viewmodel::{QueuedKey, QueuedRow, QueuedState};
use engine_api::{AccountId, PendingOpId, Provider};

use crate::App;

/// The op-id namespace the crypto queue's rows take in the sending
/// snapshot: the high bit set, so a verb routed by op id can tell a crypto
/// row from an engine op and the two id spaces never collide.
pub(crate) const CRYPTO_OP_NAMESPACE: u64 = 1 << 63;

impl<P: Provider> App<P> {
    /// Drains the crypto outbox through the bypass transport: every due row
    /// submits, a delivered row files its Sent copy and settles, a
    /// transient failure stays queued, a permanent refusal fails, and a
    /// mid-DATA loss lands unconfirmed for the user's answer.
    pub(crate) async fn drain_crypto_outbox(&self) {
        let Some(support) = self.crypto_support() else {
            return;
        };
        let due = list_due_crypto_sends(&support.store).unwrap_or_default();
        for row in due {
            let Ok(account) = AccountId::try_from(row.account_id.as_str()) else {
                continue;
            };
            let Some(transport) = support.resolver.crypto_transport(&account).await else {
                log::warn!(
                    "outbox: a crypto send's account no longer has submission settings; it waits"
                );
                continue;
            };
            if let Err(err) = mark_crypto_sending(&support.store, row.id) {
                log::warn!("outbox: marking a crypto send in-flight failed: {err}");
                continue;
            }
            let result = astra_crypto_store::submit_smtp(
                &transport.connector,
                &transport.smtp,
                &row.env_from,
                &row.env_to,
                &row.wire_bytes,
            )
            .await;
            match result {
                Ok(()) => {
                    // The Sent copy is best-effort beside the delivery,
                    // exactly the engine's Unfiled posture: said aloud,
                    // never retried automatically here.
                    if let Err(err) = astra_crypto_store::file_sent_copy(
                        &transport.connector,
                        &transport.imap,
                        &row.filed_bytes,
                    )
                    .await
                    {
                        log::warn!("outbox: delivered, but the Sent copy was not filed: {err}");
                    }
                    if let Err(err) = settle_crypto_send(&support.store, row.id) {
                        log::warn!("outbox: settling a delivered crypto send failed: {err}");
                    }
                    log::info!("outbox: a crypto send was delivered");
                }
                Err(failure) => match failure.kind() {
                    SubmitFailureKind::Transient => {
                        if let Err(err) =
                            requeue_crypto_send(&support.store, row.id, Some(&failure.to_string()))
                        {
                            log::warn!("outbox: requeueing a crypto send failed: {err}");
                        }
                    }
                    SubmitFailureKind::Permanent => {
                        if let Err(err) =
                            mark_crypto_failed(&support.store, row.id, &failure.to_string())
                        {
                            log::warn!("outbox: failing a crypto send failed: {err}");
                        }
                    }
                    SubmitFailureKind::Unconfirmed => {
                        log::warn!(
                            "outbox: a crypto send may or may not have been delivered: {failure}"
                        );
                        if let Err(err) =
                            mark_crypto_unconfirmed(&support.store, row.id, &failure.to_string())
                        {
                            log::warn!("outbox: marking a crypto send unconfirmed failed: {err}");
                        }
                    }
                },
            }
        }
    }

    /// The crypto queue's rows for the sending snapshot, projected into the
    /// engine rows' shape under the crypto op namespace.
    pub(crate) fn crypto_outbox_rows(&self) -> Vec<QueuedRow> {
        let Some(support) = self.crypto_support() else {
            return Vec::new();
        };
        list_crypto_outbox(&support.store)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|row| {
                let account = AccountId::try_from(row.account_id.as_str()).ok()?;
                let state = match row.state.as_str() {
                    "queued" => QueuedState::Queued,
                    "sending" => QueuedState::Sending,
                    "unconfirmed" => QueuedState::Unconfirmed,
                    _ => QueuedState::Failed,
                };
                let arrived_at = row
                    .created_at
                    .parse::<i64>()
                    .ok()
                    .and_then(|secs| time::OffsetDateTime::from_unix_timestamp(secs).ok())
                    .map_or_else(String::new, |instant| {
                        crate::list_rows::format_instant(
                            engine_api::UtcDateTime::new(
                                instant.year(),
                                u8::from(instant.month()),
                                instant.day(),
                                instant.hour(),
                                instant.minute(),
                                instant.second(),
                            )
                            .expect("a unix timestamp is always a valid date"),
                        )
                    });
                Some(QueuedRow {
                    queued: QueuedKey {
                        account,
                        op: CRYPTO_OP_NAMESPACE | u64::try_from(row.id).unwrap_or(0),
                    },
                    state,
                    subject: row.subject,
                    arrived_at,
                })
            })
            .collect()
    }

    /// Whether `op` names a crypto-queue row (the namespace bit) and, when
    /// it does, its row id.
    pub(crate) fn crypto_row_of(op: PendingOpId) -> Option<i64> {
        let raw = op.get();
        if raw & CRYPTO_OP_NAMESPACE == 0 {
            return None;
        }
        i64::try_from(raw & !CRYPTO_OP_NAMESPACE).ok()
    }

    /// Routes an outbox verb that named a crypto row: cancel withdraws,
    /// send-now requeues and drains, the confirmation pair answers an
    /// unconfirmed row (delivered settles, not-delivered requeues and
    /// drains, mirroring the engine outbox's contract).
    pub(crate) async fn crypto_verb(&self, op: PendingOpId, verb: CryptoVerb) {
        let Some(support) = self.crypto_support() else {
            return;
        };
        let Some(row_id) = Self::crypto_row_of(op) else {
            return;
        };
        let result = match verb {
            CryptoVerb::Withdraw | CryptoVerb::ConfirmDelivered => {
                settle_crypto_send(&support.store, row_id)
            }
            CryptoVerb::SendNow | CryptoVerb::ConfirmNotDelivered => {
                requeue_crypto_send(&support.store, row_id, None)
            }
        };
        if let Err(err) = result {
            log::warn!("outbox: a crypto row verb failed: {err}");
        }
        if matches!(verb, CryptoVerb::SendNow | CryptoVerb::ConfirmNotDelivered) {
            self.drain_crypto_outbox().await;
        }
        self.rebuild_sending().await;
    }
}

/// The outbox verbs a crypto row accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CryptoVerb {
    /// Withdraw the row: the send never goes out.
    Withdraw,
    /// Attempt the row now.
    SendNow,
    /// The user says the unconfirmed send was delivered.
    ConfirmDelivered,
    /// The user says it was not: re-send.
    ConfirmNotDelivered,
}
