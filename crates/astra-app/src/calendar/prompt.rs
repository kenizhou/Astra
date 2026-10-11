// SPDX-License-Identifier: GPL-3.0-only
//! The reply-delivery-failure prompt's other half: acting on the answer. The
//! prompt itself is raised in [`super::invite`] the moment a scheduling
//! server reports it could not deliver a reply; this module is what the
//! user's answer to that prompt does, ported from mail-calendar's
//! `answer_reply_prompt` contracts.
//!
//! Two rules the port keeps:
//!
//! - **The question is taken before anything is awaited.** Two taps on a
//!   modal that has not closed yet, or a client that dispatches on both press
//!   and release, would otherwise email the organiser two replies, and the
//!   second would arrive with no way for them to tell it apart from a change
//!   of mind.
//! - **Everything about the reply is re-derived, never carried.** The only
//!   state worth trusting is what the store holds now: the invitation's
//!   bytes, the matched alias, the meeting's instant. A prompt that outlived a
//!   restart cannot act on stale state, because it carries none beyond the
//!   message to re-read.

use astra_viewmodel::Participation;
use engine_api::{AccountId, Provider};

use crate::{calendar::ReplyPrompt, protocol::RsvpAnswer, reading::resolve, App, MessageRef};

/// What this account does when its calendar server reports it could not
/// deliver an answer's reply: ask every time (the default; sending mail as
/// the user is the user's decision), email the organiser ourselves without
/// asking, or never email and never ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplyFallback {
    /// Ask each time a delivery fails.
    Ask,
    /// Mail the reply ourselves, no question asked.
    Always,
    /// Never mail a reply; the user is still told the organiser was not
    /// informed.
    Never,
}

impl<P: Provider> App<P> {
    /// The `AnswerReplyPrompt` arm: take the question, learn the standing
    /// choice when asked to, and mail the reply when told to. Dismissing
    /// never fails: nothing was asked of the network, and the answer itself
    /// was stored long before the question arose.
    pub(crate) async fn answer_reply_prompt(
        &self,
        send: bool,
        remember: bool,
        reply_subject: Option<String>,
    ) {
        let Some(prompt) = self.take_reply_prompt() else {
            log::info!("calendar: there is no pending question to answer");
            return;
        };
        if remember {
            let choice = if send {
                ReplyFallback::Always
            } else {
                ReplyFallback::Never
            };
            self.calendar_state
                .lock()
                .expect("calendar mutex poisoned")
                .reply_fallbacks
                .insert(prompt.account.clone(), choice);
            log::info!("calendar: the account learned its reply fallback");
        }
        // The question is gone whichever way it was answered, and the
        // rebuilt snapshot is what closes the surface's modal.
        self.rebuild_calendar().await;
        if !send {
            log::info!("calendar: the organiser will not be emailed");
            return;
        }
        self.send_reply_for(&prompt, reply_subject.as_deref()).await;
    }

    /// This account's standing answer to the prompt: [`ReplyFallback::Ask`]
    /// until the account itself says otherwise, because mail sent as the user
    /// starts as the user's decision every time.
    pub(crate) fn reply_fallback_for(&self, account: &AccountId) -> ReplyFallback {
        self.calendar_state
            .lock()
            .expect("calendar mutex poisoned")
            .reply_fallbacks
            .get(account)
            .copied()
            .unwrap_or(ReplyFallback::Ask)
    }

    /// Removes and returns the pending question, the most recently raised
    /// one: the question a surface is showing, and the one whose replacement
    /// of an older question about the same invitation kept it current.
    fn take_reply_prompt(&self) -> Option<ReplyPrompt> {
        self.calendar_state
            .lock()
            .expect("calendar mutex poisoned")
            .prompts
            .pop()
    }

    /// Rebuilds the answer from the message the prompt names and mails the
    /// iTIP `REPLY` through the account's submit path, with the subject the
    /// host composed when it composed one.
    async fn send_reply_for(&self, prompt: &ReplyPrompt, reply_subject: Option<&str>) {
        let Some(handle) = self.account_handle(&prompt.account).await else {
            log::warn!("calendar: the account the prompt belongs to is gone");
            return;
        };
        let Some(provider) = handle.providers.first() else {
            log::warn!("calendar: the account has no provider to reply through");
            return;
        };
        let Some(message) =
            MessageRef::from_parts(prompt.account.as_str(), prompt.message.as_str().to_owned())
        else {
            return;
        };
        let Some(original) = resolve(&self.engine, &message).await else {
            log::info!("calendar: the invitation message is no longer in the store");
            return;
        };
        let scheduling = self
            .engine
            .message_scheduling(provider, &prompt.account, &original)
            .await
            .ok()
            .flatten();
        let Some(scheduling) = scheduling else {
            log::info!("calendar: the message carries no invitation to answer");
            return;
        };
        let Some(attendee) = super::invite::matched_attendee(&scheduling) else {
            log::info!("calendar: the invitation is not addressed to this account");
            return;
        };
        let Some(response) = rsvp_answer_of(prompt.response) else {
            log::warn!("calendar: the prompt carries an answer a reply cannot state");
            return;
        };
        self.send_imip_reply(
            provider,
            &prompt.account,
            &original,
            &scheduling,
            &attendee,
            response,
            prompt.comment.as_deref(),
            reply_subject,
        )
        .await;
    }
}

/// The reply a prompt's participation word stands for: the three an answer
/// can be, or `None` for the holds and delegations no prompt is raised with.
fn rsvp_answer_of(participation: Participation) -> Option<RsvpAnswer> {
    match participation {
        Participation::Accepted => Some(RsvpAnswer::Accepted),
        Participation::Tentative => Some(RsvpAnswer::Tentative),
        Participation::Declined => Some(RsvpAnswer::Declined),
        Participation::NeedsAction | Participation::Delegated => None,
    }
}

#[cfg(test)]
#[path = "../calendar_prompt_tests.rs"]
mod tests;
