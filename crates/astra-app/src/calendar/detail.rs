// SPDX-License-Identifier: GPL-3.0-only
//! The event detail read: the roll-call of who is on one event and the full
//! repeating rule, joined from the master the event reference names through
//! the same targeted read the snapshot rebuild rides
//! ([`Engine::events_by_keys`](engine_api::Engine::events_by_keys), never a
//! decode of the whole diary). The occurrence row stays lean; this read is
//! what a surface pulls when the person picks an event and the detail asks
//! who is coming and how it repeats.

use astra_viewmodel::{EventDetail, ParticipantRow};
use engine_api::{Participant, ParticipantRole, Provider};

use crate::{reference::EventRef, App};

/// How many lines the roll-call shows before folding the tail into a count.
/// Eight is about the size a docked pane reads at a glance: beyond it the
/// list is a crowd, and the folded count tells the size better than the
/// names do.
pub(crate) const ROLL_CALL_LIMIT: usize = 8;

impl<P: Provider> App<P> {
    /// Reads one event's detail: the participants roll-call (the organiser
    /// first, capped at [`ROLL_CALL_LIMIT`] lines, the tail folded into a
    /// count) and the full repeating rule with its read-back sentence, both
    /// from the master the store holds. `None` when the account's store
    /// holds no event at the reference's key.
    pub async fn calendar_event_detail(&self, event: &EventRef) -> Option<EventDetail> {
        let masters = self
            .engine
            .events_by_keys(&event.account, std::slice::from_ref(&event.key))
            .await
            .ok()?;
        let master = masters.into_iter().next()?;
        let (participants, folded_participants) = roll_call(&master);
        Some(EventDetail {
            account: event.account.clone(),
            event: event.key.clone(),
            participants,
            folded_participants,
            rule: master
                .recurrence
                .as_ref()
                .and_then(super::rule_words::rule_read),
        })
    }
}

/// The roll-call: the owner-role participants first (the organiser the
/// briefs and the invitation card read), every further participant behind
/// them in stored order, each as one line, capped with the tail folded into
/// a count. A participant with neither a name nor an address renders nothing
/// and counts nowhere: there is no person to show for such a line.
fn roll_call(event: &engine_api::Event) -> (Vec<ParticipantRow>, u32) {
    let mut rows = Vec::new();
    let mut folded = 0_u32;
    let owners = event
        .participants
        .iter()
        .filter(|participant| participant.has_role(&ParticipantRole::Owner));
    let others = event
        .participants
        .iter()
        .filter(|participant| !participant.has_role(&ParticipantRole::Owner));
    for participant in owners.chain(others) {
        let Some(line) = line_of(participant) else {
            continue;
        };
        if rows.len() < ROLL_CALL_LIMIT {
            rows.push(ParticipantRow {
                name_or_address: line,
                role: role_word(participant),
                response: super::snapshot::participation_word(&participant.participation_status),
            });
        } else {
            folded += 1;
        }
    }
    (rows, folded)
}

/// One participant as a line: the name where the line carries one, the
/// address otherwise, nothing when it carries neither.
fn line_of(participant: &Participant) -> Option<String> {
    fn non_empty(value: Option<&str>) -> Option<&str> {
        value.map(str::trim).filter(|value| !value.is_empty())
    }
    non_empty(participant.name.as_deref())
        .or_else(|| non_empty(participant.email.as_deref()))
        .map(str::to_owned)
}

/// The role word where the participant holds a seat beside the plain
/// attendee's, by precedence: the organiser's word outranks the chair's, and
/// a role this vocabulary does not know crosses verbatim rather than being
/// translated into a word that lies.
fn role_word(participant: &Participant) -> Option<String> {
    for (role, word) in [
        (ParticipantRole::Owner, "organiser"),
        (ParticipantRole::Chair, "chair"),
        (ParticipantRole::Optional, "optional"),
        (ParticipantRole::Informational, "informational"),
        (ParticipantRole::Contact, "contact"),
    ] {
        if participant.has_role(&role) {
            return Some(word.to_owned());
        }
    }
    participant.roles.iter().find_map(|role| match role {
        ParticipantRole::Other(word) => Some(word.clone()),
        _ => None,
    })
}

#[cfg(test)]
#[path = "../calendar_detail_tests.rs"]
mod tests;
