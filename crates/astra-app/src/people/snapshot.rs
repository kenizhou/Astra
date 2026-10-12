// SPDX-License-Identifier: GPL-3.0-only
//! The people snapshot rebuild: the engine's unified page re-ordered into the
//! fold, the open person's detail over the engine's own sources, the write
//! states, and the create targets the installed legs advertise.
//!
//! Three rules the rebuild owns, each recorded once:
//!
//! - **The list lays out by the fold, not the page order.** The engine orders
//!   its page by raw lowercased code points and does not fold, so the rebuild
//!   re-orders the page's rows by its own [`FoldKey`] before publishing: one
//!   alphabet with accented letters filed under their base, one `#` bucket
//!   after it. The rows are never re-filtered: the engine matched the query
//!   over every person, and a host-side filter over the page would silently
//!   unfind anyone the page cap had already cut off.
//! - **The display name is a name or it is empty.** The engine's display pick
//!   falls back to the first address when no source carries a name; the row
//!   blanks that to empty and carries the address in its own field, so the
//!   host supplies the placeholder words in the user's language and the
//!   monogram and the fold still key on the identity the engine chose.
//! - **The disclosure facts are counted, not echoed.** The account count is
//!   distinct accounts across the person's sources, and it crosses no key
//!   until there is a merge to disclose; the monogram and section are derived
//!   once here so the same person cannot differ between surfaces.

use std::collections::BTreeSet;

use astra_viewmodel::{
    CreateTargetRow, EditableCardRow, PeopleSnapshot, PersonDetail, PersonRow, PersonSourceRow,
    PersonValueKind, PersonValueRow,
};
use engine_api::{
    AccountId, ContactId, ContactsProvider, Person, PersonId, PersonSourceId, Provider,
};

use crate::people::{fold_identity, monogram_of, section_of, PeopleInputs, PEOPLE_PAGE_LIMIT};
use crate::App;

impl<P: Provider> App<P> {
    /// Rebuilds the people snapshot from the **already-synced** store and
    /// publishes it on [`Surface::People`](crate::Surface::People): the
    /// query's page re-ordered into the fold, the open person's detail, the
    /// write states, and the create targets. A store failure is logged and
    /// the snapshot holds what it can: an empty rows list from a failed read
    /// is a list that honestly says nothing, never a half-built page.
    pub(crate) async fn rebuild_people(&self) {
        let inputs = self.people_inputs();
        let rows = match self
            .engine
            .people_page(&engine_api::PeopleQuery {
                query: inputs.query.clone(),
                limit: PEOPLE_PAGE_LIMIT,
                ..engine_api::PeopleQuery::default()
            })
            .await
        {
            Ok(page) => fold_page_rows(page.people),
            Err(err) => {
                log::warn!("people: the page read failed: {err}");
                Vec::new()
            }
        };
        let detail = match inputs.open {
            Some(open) => self.person_detail(open).await,
            None => None,
        };
        let create_targets = self.people_create_targets(&inputs).await;
        let write_states = inputs
            .writes
            .into_iter()
            .map(|(account, state)| astra_viewmodel::PeopleWriteRow { account, state })
            .collect();
        self.people.publish(PeopleSnapshot {
            rows,
            detail,
            write_states,
            create_targets,
        });
    }

    /// The open person's detail: the engine's own resolution of `open`
    /// (through a merge's retired id), its sources in the engine's order, and
    /// each value tagged with the source cards that supplied it. `None` when
    /// the id resolves to nobody, which is also what a person whose every
    /// card has since been tombstoned resolves to: nothing left to show.
    async fn person_detail(&self, open: PersonId) -> Option<astra_viewmodel::PersonDetail> {
        let person = match self.engine.person(open).await {
            Ok(person) => person?,
            Err(err) => {
                log::warn!("people: the person read failed: {err}");
                return None;
            }
        };
        let sources = match self.engine.person_sources(person.id).await {
            Ok(sources) => sources,
            Err(err) => {
                log::warn!("people: the source read failed: {err}");
                Vec::new()
            }
        };
        let source_rows: Vec<PersonSourceRow> = sources
            .iter()
            .map(|source| PersonSourceRow {
                account: source.id.account.clone(),
                card: source.id.contact.clone(),
                writable: source.writable,
            })
            .collect();
        let lookup = |account: &AccountId, card: &ContactId| {
            sources
                .iter()
                .find(|source| source.id.account == *account && source.id.contact == *card)
                .is_some_and(|source| source.writable)
        };
        let values = detail_values(&person, &lookup);
        let display_name = row_display_name(&person);
        let editable_cards = sources
            .iter()
            .filter(|source| source.writable)
            .map(|source| EditableCardRow {
                person: person.id,
                account: source.id.account.clone(),
                card: source.id.contact.clone(),
            })
            .collect();
        Some(PersonDetail {
            person: person.id,
            display_name,
            sources: source_rows,
            values,
            editable_cards,
        })
    }

    /// The writable books a create may land in: the destinations the
    /// installed legs advertise, named and preselected from the store's own
    /// book rows. A destination the store cannot yet name stays off the list:
    /// a create target with no name to choose it by is a refresh away.
    async fn people_create_targets(&self, inputs: &PeopleInputs) -> Vec<CreateTargetRow> {
        let mut targets = Vec::new();
        for (account, legs) in &inputs.legs {
            let destinations = self
                .engine
                .contact_destinations(legs.iter().map(|leg| leg.as_ref() as &dyn ContactsProvider));
            if destinations.is_empty() {
                continue;
            }
            let books = match self.engine.address_books(account).await {
                Ok(books) => books,
                Err(err) => {
                    log::warn!("people: the address-book read failed: {err}");
                    continue;
                }
            };
            for destination in destinations {
                if let Some(book) = books
                    .iter()
                    .find(|book| book.id == destination.address_book)
                {
                    targets.push(CreateTargetRow {
                        account: account.clone(),
                        book: book.id.clone(),
                        name: book.name.clone(),
                        is_default: book.is_default,
                    });
                }
            }
        }
        targets.sort_by(|left, right| {
            (left.account.as_str(), left.book.as_str())
                .cmp(&(right.account.as_str(), right.book.as_str()))
        });
        targets
    }
}

/// The page's rows, re-ordered into the fold: the alphabet first (each row
/// keyed by its folded identity), then the one `#` bucket, each bucket
/// ordered by its key with the person id breaking ties so the order is
/// deterministic whatever the page handed over.
fn fold_page_rows(people: Vec<Person>) -> Vec<PersonRow> {
    let mut keyed: Vec<(u8, String, PersonId, PersonRow)> = people
        .into_iter()
        .map(|person| {
            let fold = fold_identity(person.display_name.as_deref().unwrap_or_default());
            let section = section_of(&fold);
            let person_id = person.id;
            let row = person_row(&person, &section);
            (fold.bucket, fold.key, person_id, row)
        })
        .collect();
    keyed.sort_by(|left, right| (left.0, &left.1, left.2).cmp(&(right.0, &right.1, right.2)));
    keyed.into_iter().map(|entry| entry.3).collect()
}

/// One unified person as the list renders them, with the presentation facts
/// derived once: the blanked display name, the counted disclosure, and the
/// monogram keyed on the primary address.
fn person_row(person: &Person, section: &str) -> PersonRow {
    let display_name = row_display_name(person);
    let primary_address = person
        .emails
        .first()
        .map(|email| email.value.as_str().to_owned());
    let accounts: BTreeSet<&AccountId> = person.sources.iter().map(|id| &id.account).collect();
    PersonRow {
        person: person.id,
        monogram: monogram_of(&display_name, primary_address.as_deref(), person.id),
        account_count: (accounts.len() > 1)
            .then(|| u32::try_from(accounts.len()).unwrap_or(u32::MAX)),
        display_name,
        section: section.to_owned(),
        primary_address,
    }
}

/// The monogram one person's avatar draws: the row's own derivation, shared
/// by every surface that shows the person so the two cannot disagree.
pub(crate) fn person_monogram(person: &Person) -> astra_viewmodel::Monogram {
    let display_name = row_display_name(person);
    let primary_address = person
        .emails
        .first()
        .map(|email| email.value.as_str().to_owned());
    monogram_of(&display_name, primary_address.as_deref(), person.id)
}

/// The row's display name: the engine's pick when the sources carry a name,
/// empty when they do not (the host supplies the words; the engine's
/// address fallback never leaks into the name field).
fn row_display_name(person: &Person) -> String {
    if person.names.is_empty() {
        String::new()
    } else {
        person.display_name.clone().unwrap_or_default()
    }
}

/// The person's values with their provenance: names, addresses, phones,
/// organisations, and titles, each tagged with the source cards that
/// supplied it, in the engine's own order within each kind.
fn detail_values(
    person: &Person,
    writable: &impl Fn(&AccountId, &ContactId) -> bool,
) -> Vec<PersonValueRow> {
    let mut values = Vec::new();
    let mut push = |kind: PersonValueKind, value: String, ids: &BTreeSet<PersonSourceId>| {
        let sources = ids
            .iter()
            .map(|id| PersonSourceRow {
                account: id.account.clone(),
                card: id.contact.clone(),
                writable: writable(&id.account, &id.contact),
            })
            .collect();
        values.push(PersonValueRow {
            kind,
            value,
            sources,
        });
    };
    for name in &person.names {
        push(PersonValueKind::Name, name.value.clone(), &name.sources);
    }
    for email in &person.emails {
        push(
            PersonValueKind::Email,
            email.value.as_str().to_owned(),
            &email.sources,
        );
    }
    for phone in &person.phones {
        push(PersonValueKind::Phone, phone.value.clone(), &phone.sources);
    }
    for organisation in &person.organizations {
        push(
            PersonValueKind::Organisation,
            organisation.value.clone(),
            &organisation.sources,
        );
    }
    for title in &person.titles {
        push(PersonValueKind::Title, title.value.clone(), &title.sources);
    }
    values
}
#[cfg(test)]
#[path = "../people_snapshot_tests.rs"]
mod tests;
