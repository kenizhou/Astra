// SPDX-License-Identifier: GPL-3.0-only
//! The invitation machine: what a `text/calendar` message means to this
//! account, and how an answer travels. Ported from mail-calendar's
//! invitation contracts, over the engine's
//! [`message_scheduling`](engine_api::Engine::message_scheduling) read and
//! RSVP verb.
//!
//! The rules that decide everything here, all mail-calendar's:
//!
//! - **The two-condition gate.** An RSVP is offered for a scheduling
//!   `METHOD` an answer is owed on **and** an `ATTENDEE` that is one of the
//!   addresses the message was delivered to (an alias, not necessarily the
//!   account's primary). Either alone is wrong: a `PUBLISH` that happens to
//!   list the address is informational, and a `REQUEST` for somebody else is
//!   somebody else's meeting.
//! - **The answer's route is a capability fact, read before the write.** A
//!   server that schedules is trusted to deliver (a second reply of our own
//!   would reach the organiser twice); a server that does not is answered the
//!   Thunderbird way, the `PARTSTAT` stored **and** the iTIP `REPLY` mailed by
//!   the client (the document and the sending live in
//!   [`super::imip`]); an account with neither route offers no answer at all
//!   rather than three controls that quietly go nowhere.
//! - **A broken delivery promise becomes a prompt, never a silent repair.**
//!   On the server route the receipt's [`ReplyDelivery`] is read after the
//!   write: `NotReported` is no information (silence is not failure), while
//!   `Failed` records the reply-delivery-failure prompt, because sending mail
//!   as the user is a decision the user makes.
//! - **A stale invitation offers no answer.** The calendar holding a higher
//!   `SEQUENCE` for the same `UID` means the organiser has moved on; answering
//!   the old copy agrees to times that are no longer the meeting's, and the
//!   reply would carry a revision an organiser's scheduler may discard.

use astra_viewmodel::Participation;
use engine_api::{
    addresses_match, Event, EventRsvp, InboundScheduling, Provider, RsvpResponse, ScheduleMethod,
};

use crate::{
    calendar::prompt::ReplyFallback, calendar::writes::generated_idempotency, protocol::RsvpAnswer,
    reading::resolve, App, MessageRef,
};

/// What a reading view may offer for one scheduling message: the two-condition
/// gate's verdict, narrowed to the kinds this machine can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvitationKind {
    /// A reply is owed and this account is the one that owes it.
    Rsvp,
    /// The organiser cancelled the meeting. No reply is possible.
    Cancelled,
    /// The details, and no response: the message is informational for this
    /// account.
    Informational,
    /// The calendar holds a newer revision of the same meeting (RFC 5546
    /// §2.1.5). No response: the copy on screen is out of date.
    Superseded,
}

/// The two-condition gate's method half, for a message this account was an
/// attendee of: `None` for the methods that produce no card at all (a `REPLY`
/// arriving, a `COUNTER`, a bare `PUBLISH`), because each needs its own UI to
/// mean anything.
pub(crate) fn classify(method: &ScheduleMethod, i_am_an_attendee: bool) -> Option<InvitationKind> {
    match (method, i_am_an_attendee) {
        (ScheduleMethod::Request, true) => Some(InvitationKind::Rsvp),
        (ScheduleMethod::Cancel, true) => Some(InvitationKind::Cancelled),
        (ScheduleMethod::Request | ScheduleMethod::Cancel, false) => {
            Some(InvitationKind::Informational)
        }
        _ => None,
    }
}

/// Downgrades an answerable invitation when the calendar already holds a
/// newer revision of the same meeting.
///
/// `SEQUENCE` alone, deliberately: the RFC's `DTSTAMP` tie-break orders two
/// *messages*, and the comparison here is against a stored event whose
/// `updated` changes whenever anything touches it, our own RSVP included. A
/// tie-break against it would hide a card moments after the user answered it,
/// and the two ways this can be wrong are not symmetric: missing a
/// supersession leaves a stale card, while inventing one hides a reply the
/// organiser is waiting for.
pub(crate) fn supersede(
    kind: InvitationKind,
    mail_sequence: u32,
    stored: Option<&Event>,
) -> InvitationKind {
    let outranked = stored.is_some_and(|event| event.sequence > mail_sequence);
    match kind {
        InvitationKind::Rsvp if outranked => InvitationKind::Superseded,
        other => other,
    }
}

/// The `ATTENDEE` this account answers **as**: the participant whose address
/// matches one the message was delivered to, returned verbatim.
///
/// The delivery headers (`Delivered-To` and friends) are how an invitation
/// sent to an alias is recognised with no configuration at all, and returning
/// the matched address (rather than a bool) is what makes an alias RSVP work:
/// the write must name the `ATTENDEE` line the invitation actually used.
pub(crate) fn matched_attendee(scheduling: &InboundScheduling) -> Option<String> {
    scheduling
        .message
        .event
        .participants
        .iter()
        .find_map(|participant| {
            let attendee = participant.email.as_deref()?;
            scheduling
                .delivery_recipients
                .iter()
                .any(|mine| addresses_match(attendee, mine))
                .then(|| attendee.to_owned())
        })
}

/// How an answer reaches the organiser on one account, from the two
/// capabilities that decide it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplyRoute {
    /// The calendar server schedules: storing the answer *is* sending it, and
    /// a reply of our own would reach the organiser twice.
    Server,
    /// Nobody schedules, but the mail transport can carry an iTIP object: the
    /// client builds and mails the `REPLY` itself.
    ClientImip,
    /// Neither route exists. No answer is offered.
    None,
}

/// The route from the capabilities: `can_store` is the RSVP verb being
/// expressible at all, `server_schedules` and `can_send_imip` the engine's
/// `calendar_scheduling` and `scheduling_submission`.
pub(crate) const fn reply_route(
    can_store: bool,
    server_schedules: bool,
    can_send_imip: bool,
) -> ReplyRoute {
    match (can_store, server_schedules, can_send_imip) {
        (true, true, _) => ReplyRoute::Server,
        (_, false, true) => ReplyRoute::ClientImip,
        _ => ReplyRoute::None,
    }
}

impl<P: Provider> App<P> {
    /// The `RespondToInvitation` arm: the whole machine above, end to end.
    pub(crate) async fn respond_to_invitation(
        &self,
        message: MessageRef,
        response: RsvpAnswer,
        comment: Option<String>,
        notify_organizer: bool,
    ) {
        let Some(handle) = self.account_handle(&message.account).await else {
            log::warn!("calendar: the account the invitation belongs to is gone");
            return;
        };
        let Some(provider) = super::calendar_provider_of(&handle) else {
            log::warn!("calendar: the account has no provider to answer through");
            return;
        };
        let Some(original) = resolve(&self.engine, &message).await else {
            log::info!("calendar: the invitation message is no longer in the store");
            return;
        };
        let scheduling = self
            .engine
            .message_scheduling(provider, &message.account, &original)
            .await
            .ok()
            .flatten();
        let Some(scheduling) = scheduling else {
            log::info!("calendar: the message carries no invitation to answer");
            return;
        };
        let Some(attendee) = matched_attendee(&scheduling) else {
            log::info!("calendar: the invitation is not addressed to this account");
            return;
        };
        let event = &scheduling.message.event;
        let stored = self.stored_event_by_uid(&message.account, event).await;
        let kind = supersede(
            classify(&scheduling.message.method, true).unwrap_or(InvitationKind::Informational),
            event.sequence,
            stored.as_ref(),
        );
        if kind != InvitationKind::Rsvp {
            log::info!("calendar: the invitation offers no answer on this copy");
            return;
        }
        let caps = provider.connection_info().capabilities;
        let route = reply_route(
            caps.calendar_rsvp().is_some(),
            caps.calendar_scheduling(),
            caps.scheduling_submission(),
        );
        if route == ReplyRoute::None {
            log::info!("calendar: this account has no route that reaches the organiser");
            return;
        }
        // On the server route the note and the quiet flag belong to the verb,
        // and a transport that cannot honour one refuses the write rather than
        // dropping it. On the client route neither belongs there: the note
        // travels as the reply's `COMMENT`, and whether the organiser hears
        // is whether we mail the reply at all.
        let (verb_comment, verb_notify) = match route {
            ReplyRoute::Server => {
                if let Some(controls) = caps.calendar_rsvp() {
                    if comment.is_some() && !controls.comment {
                        log::info!("calendar: this transport cannot carry a note; refused");
                        return;
                    }
                    if !notify_organizer && !controls.suppress_notification {
                        log::info!(
                            "calendar: this transport's server always sends the reply; refused"
                        );
                        return;
                    }
                }
                (comment.clone(), notify_organizer)
            }
            ReplyRoute::ClientImip | ReplyRoute::None => (None, true),
        };
        let base = if let Some(stored) = stored {
            stored
        } else {
            // The meeting may never have left the mailbox: no RFC moves an
            // invitation into the calendar, so the client stores it (RFC 6638
            // §3.2.2), from the invitation's own bytes under a guarded create
            // that finds a copy the server deposited instead of duplicating it.
            let Some(stored) = self
                .store_invitation(provider, &message.account, &scheduling)
                .await
            else {
                log::info!("calendar: the meeting could not be put on the calendar");
                return;
            };
            stored
        };
        let mut rsvp = EventRsvp::to(
            &base,
            &attendee,
            match response {
                RsvpAnswer::Accepted => RsvpResponse::Accepted,
                RsvpAnswer::Tentative => RsvpResponse::Tentative,
                RsvpAnswer::Declined => RsvpResponse::Declined,
            },
        );
        if let Some(note) = &verb_comment {
            rsvp = rsvp.comment(note);
        }
        if !verb_notify {
            rsvp = rsvp.quietly();
        }
        self.mark_calendar_write(
            &message.account,
            astra_viewmodel::CalendarWriteState::Writing,
        );
        self.rebuild_calendar().await;
        let written = self
            .engine
            .rsvp_calendar_event(
                provider,
                &message.account,
                &generated_idempotency(),
                &base,
                &rsvp,
            )
            .await;
        match written {
            Ok(write) => {
                let at = self.account_ordinal(&message.account).await;
                log::info!("calendar[a{at}]: the invitation was answered");
                self.settle_calendar_write(&message.account, &write.reconciled)
                    .await;
                match route {
                    ReplyRoute::Server => {
                        // A broken delivery promise becomes the user's
                        // decision: the account's standing answer decides
                        // between mailing the reply ourselves (no question
                        // asked), staying silent (no question offered), and
                        // asking, which is where every account starts.
                        if let Some(prompt) = read_reply_delivery(
                            &message,
                            event,
                            response,
                            comment.as_deref(),
                            &write.write.reply_delivery,
                        ) {
                            match self.reply_fallback_for(&message.account) {
                                ReplyFallback::Always => {
                                    log::info!(
                                        "calendar: emailing the reply ourselves; the account \
                                         says always"
                                    );
                                    self.send_imip_reply(
                                        provider,
                                        &message.account,
                                        &original,
                                        &scheduling,
                                        &attendee,
                                        response,
                                        comment.as_deref(),
                                        None,
                                    )
                                    .await;
                                }
                                ReplyFallback::Never => {
                                    log::info!(
                                        "calendar: not asking; the account says never email a \
                                         reply it cannot deliver"
                                    );
                                }
                                ReplyFallback::Ask => {
                                    self.raise_reply_prompt(prompt).await;
                                }
                            }
                        }
                    }
                    ReplyRoute::ClientImip => {
                        if notify_organizer {
                            self.send_imip_reply(
                                provider,
                                &message.account,
                                &original,
                                &scheduling,
                                &attendee,
                                response,
                                comment.as_deref(),
                                None,
                            )
                            .await;
                        } else {
                            log::info!("calendar: the organiser was deliberately not told");
                        }
                    }
                    ReplyRoute::None => {}
                }
            }
            Err(err) => {
                log::warn!("calendar: the answer was refused: {err}");
                self.mark_calendar_write(
                    &message.account,
                    astra_viewmodel::CalendarWriteState::FailedWithRetry,
                );
                self.rebuild_calendar().await;
            }
        }
    }

    /// Records the reply-delivery-failure prompt and publishes it: the
    /// question a surface asks about mail sent as the user.
    async fn raise_reply_prompt(&self, prompt: crate::calendar::ReplyPrompt) {
        {
            let mut state = self.calendar_state.lock().expect("calendar mutex poisoned");
            // One prompt per invitation: a re-answer replaces the question it
            // supersedes rather than stacking beside it.
            state.prompts.retain(|standing| {
                standing.account != prompt.account || standing.message != prompt.message
            });
            state.prompts.push(prompt);
        }
        self.rebuild_calendar().await;
    }
}

/// Reads what the server reported about delivering the answer, on the
/// route that trusted it to: `Failed` builds the reply-delivery-failure
/// prompt for the caller to raise or resolve (mail as the user is the
/// user's decision), anything else, including silence, is logged and
/// left alone.
fn read_reply_delivery(
    message: &MessageRef,
    event: &Event,
    response: RsvpAnswer,
    comment: Option<&str>,
    delivery: &engine_api::ReplyDelivery,
) -> Option<crate::calendar::ReplyPrompt> {
    let status = match delivery {
        engine_api::ReplyDelivery::Failed { status } => status.clone(),
        engine_api::ReplyDelivery::Delivered { status } => {
            log::info!("calendar: the server delivered the reply (status {status})");
            return None;
        }
        engine_api::ReplyDelivery::Unrecognized { status } => {
            log::info!("calendar: the server reported an unrecognized delivery status ({status})");
            return None;
        }
        engine_api::ReplyDelivery::NotReported => {
            log::info!("calendar: the server reported nothing about the reply's delivery");
            return None;
        }
    };
    let organizer = event
        .participants
        .iter()
        .find(|participant| participant.has_role(&engine_api::ParticipantRole::Owner))
        .and_then(|participant| participant.email.clone())
        .unwrap_or_default();
    log::warn!(
        "calendar: the server could not deliver the reply (status {status}); the answer \
         stands and the telling does not"
    );
    Some(crate::calendar::ReplyPrompt {
        account: message.account.clone(),
        message: message.key.clone(),
        summary: event.title.clone(),
        organizer,
        response: match response {
            RsvpAnswer::Accepted => Participation::Accepted,
            RsvpAnswer::Tentative => Participation::Tentative,
            RsvpAnswer::Declined => Participation::Declined,
        },
        comment: comment.map(str::to_owned),
        status_code: status,
    })
}

#[cfg(test)]
#[path = "../calendar_invite_tests.rs"]
mod tests;
