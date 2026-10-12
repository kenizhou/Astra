// SPDX-License-Identifier: GPL-3.0-only
//! The avatar read: one address resolved to one person's monogram and
//! cached photo, the one resolution every surface that shows the person
//! shares with the snapshot rows.
//!
//! The monogram is the row's own derivation ([`person_monogram`]), never a
//! second one here, so a person cannot paint one colour on the contacts row
//! and another beside the reading header. The photo only ever answers from
//! the engine's **cached** read, store work alone: filling the cache is a
//! background pass's business, so a screenful of strangers costs no provider
//! round trips.

use engine_api::{CanonicalEmail, Person, PersonId, Provider};
use serde::{Deserialize, Serialize};

use crate::people::snapshot::person_monogram;
use crate::App;

/// What a person's avatar draws: the monogram before any photo arrives, and
/// the cached photo file's path when one stands.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvatarView {
    /// The initials, from the row's own derivation.
    pub initials: String,
    /// The palette fill behind the initials.
    pub colour: String,
    /// The cached photo file's path; absent crosses no key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo: Option<String>,
}

/// The person id a stranger's monogram falls back to: the colour keys on the
/// address whenever one stands, which a read keyed by email always has, so
/// the id only satisfies the derivation's shape.
fn stranger_id() -> PersonId {
    PersonId::new(1).expect("the stranger fallback id mints")
}

impl<P: Provider> App<P> {
    /// The avatar one address resolves to: the person's monogram (the row's
    /// own derivation when the address resolves, the address's initial
    /// otherwise) and the cached photo file's path when one stands. Reads
    /// that fail log and answer the monogram alone, because an avatar that
    /// cannot resolve is one worth drawing, not an error worth raising.
    pub async fn avatar_by_email(&self, email: &str) -> AvatarView {
        let person = self.person_by_email(email).await;
        let monogram = match &person {
            Some(person) => person_monogram(person),
            None => crate::people::monogram_of("", Some(email), stranger_id()),
        };
        let photo = match &person {
            Some(person) => self.person_photo(person).await,
            None => None,
        };
        AvatarView {
            initials: monogram.initials,
            colour: monogram.colour,
            photo,
        }
    }

    /// The person carrying `email`, or nobody: the engine's own join, logged
    /// and empty on failure.
    async fn person_by_email(&self, email: &str) -> Option<Person> {
        let address = CanonicalEmail::parse(email).ok()?;
        match self.engine.people_by_email(&[address]).await {
            Ok(found) => found.values().next().cloned(),
            Err(err) => {
                log::warn!("people: the address lookup failed: {err}");
                None
            }
        }
    }

    /// The first cached photo file across the person's source cards, in the
    /// sources' own order: a card with no photo resource, and one whose
    /// cache holds nothing, both step aside for the next.
    async fn person_photo(&self, person: &Person) -> Option<String> {
        for source in &person.sources {
            let card = match self
                .engine
                .contact_card(&source.account, &source.contact)
                .await
            {
                Ok(Some(card)) => card,
                Ok(None) => continue,
                Err(err) => {
                    log::warn!("people: the card read failed: {err}");
                    continue;
                }
            };
            let media = card
                .media
                .values()
                .map(|property| &property.value)
                .find(|resource| resource.kind.as_deref() == Some("photo"));
            let Some(media) = media else {
                continue;
            };
            match self
                .engine
                .cached_contact_photo(&source.account, &card, media)
                .await
            {
                Ok(Some(file)) => return Some(file.path.display().to_string()),
                Ok(None) => {}
                Err(err) => log::warn!("people: the photo read failed: {err}"),
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "../people_avatar_tests.rs"]
mod tests;
