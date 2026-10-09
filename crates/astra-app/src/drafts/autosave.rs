// SPDX-License-Identifier: GPL-3.0-only
//! The idle autosave: one core-side task that settles an open composition's save once
//! its composer has been quiet for [`DRAFT_AUTOSAVE_IDLE`](super::DRAFT_AUTOSAVE_IDLE).
//!
//! A child of [`super`], whose state this drives. The core owns the interval and the
//! deadline, but owns no executor: the driver is a plain future the host runs on its own
//! runtime for the life of the app, exactly the seam [`boot`](crate::boot) hands every
//! long-lived task over on.

use super::{CompositionId, HeldSave};
use crate::App;
use engine_api::Provider;

/// One armed autosave: the content sits on the composition's record, so the arming
/// needs only to say when it fires and for which arming.
pub(crate) struct Armed {
    /// When the composer has been quiet long enough to save.
    pub(super) deadline: tokio::time::Instant,
    /// Which arming this is, so a fire whose deadline a newer save replaced does
    /// nothing.
    pub(super) seq: u64,
}

impl<P: Provider> App<P> {
    /// The idle-autosave driver: one future the host runs on its runtime for the life of
    /// the app, because the core owns no executor to spawn one itself.
    ///
    /// Sleeps until the earliest armed save's deadline, wakes early whenever an arm
    /// moves (a new save re-arms it; a close or discard cancels it), and fires only for
    /// the arming still in place at the deadline. A fire saves the composition's held
    /// content through the ordinary save path, so an unchanged draft writes nothing and
    /// an unsettled one is retried: the quiet-period retry is what carries a save that
    /// met no network the first time, without a composer that is done typing being made
    /// to wait on an upload while it typed.
    pub async fn drive_draft_autosave(&self) {
        loop {
            let target = {
                let drafts = self.drafts.lock().expect("drafts mutex poisoned");
                drafts
                    .armed
                    .iter()
                    .min_by_key(|(_, armed)| armed.deadline)
                    .map(|(composition, armed)| (composition.clone(), armed.deadline, armed.seq))
            };
            let notified = std::pin::pin!(self.autosave_armed.notified());
            match target {
                None => {
                    notified.await;
                }
                Some((composition, deadline, seq)) => {
                    let sleep = std::pin::pin!(tokio::time::sleep_until(deadline));
                    match futures::future::select(sleep, notified).await {
                        futures::future::Either::Left(_) => {
                            if let Some(held) = self.take_armed_fire(&composition, seq) {
                                self.save_draft(&composition, &held).await;
                            }
                        }
                        futures::future::Either::Right(_) => {}
                    }
                }
            }
        }
    }

    /// (Re)arms the idle autosave: the composition's quiet interval restarts here, with
    /// whatever content its record holds.
    pub(super) fn arm_autosave(&self, composition: &CompositionId) {
        {
            let mut drafts = self.drafts.lock().expect("drafts mutex poisoned");
            drafts.arm_seq += 1;
            let seq = drafts.arm_seq;
            let deadline = tokio::time::Instant::now() + self.autosave_idle;
            drafts
                .armed
                .insert(composition.clone(), Armed { deadline, seq });
            seq
        };
        self.autosave_armed.notify_one();
    }

    /// Cancels the composition's armed autosave, if one is armed.
    pub(super) fn cancel_armed(&self, composition: &CompositionId) {
        if self
            .drafts
            .lock()
            .expect("drafts mutex poisoned")
            .armed
            .remove(composition)
            .is_some()
        {
            self.autosave_armed.notify_one();
        }
    }

    /// Takes the armed save `seq` armed, answering its content only when that arming is
    /// still the one in place: a newer save's arming must not be consumed by an older
    /// deadline's fire.
    fn take_armed_fire(&self, composition: &CompositionId, seq: u64) -> Option<HeldSave> {
        let mut drafts = self.drafts.lock().expect("drafts mutex poisoned");
        if drafts
            .armed
            .get(composition)
            .is_some_and(|armed| armed.seq == seq)
        {
            drafts.armed.remove(composition);
            return drafts
                .open
                .get(composition)
                .and_then(|record| record.held.clone());
        }
        None
    }
}
