// SPDX-License-Identifier: GPL-3.0-only
//! The CRUD mapping: the editor's input subset rendered as the engine's own
//! [`ContactDraft`] and [`ContactPatch`], the findable-again rule the app
//! core adds, and the JSON seam the mapping rides.
//!
//! The engine's typed property maps (`ContactProperty` over `PropertyId`)
//! are not re-exported through its facade at this revision, so a card or a
//! field value is assembled in the engine's own JSON shape and decoded once,
//! and a changed field is rebuilt from the card's own serialised property
//! map: the contexts, preferences, and labels ride untouched, which is the
//! whole point of rebuilding on top rather than replacing.
//!
//! [`ContactProperty`]: engine_core::contact::ContactProperty

use engine_api::{ContactCard, ContactDraft, ContactField, ContactPatch, FieldPatch};
use serde_json::{json, Value};

use crate::protocol::{ContactDraftInput, ContactNameInput, ContactPatchInput};
use crate::reference::AddressBookRef;

/// Why a people write cannot be mapped onto an engine shape. Names a rule the
/// caller can act on, never a message to parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum PeopleEditError {
    /// A card value the mapping tried to build does not decode as the
    /// engine's own card shape.
    #[error("the contact could not be built as the engine's own shape")]
    Unrepresentable,
}

/// Whether a create names anything to file the card under: a name, an
/// organisation, or an address. The refusal is the app core's own rule; the
/// engine's create validation does not carry it.
pub(crate) fn is_findable(draft: &ContactDraftInput) -> bool {
    let has_name = draft.name.as_ref().is_some_and(|name| {
        !name.given.trim().is_empty()
            || name
                .surname
                .as_deref()
                .is_some_and(|surname| !surname.trim().is_empty())
    });
    has_name
        || draft
            .organisation
            .as_deref()
            .is_some_and(|organisation| !organisation.trim().is_empty())
        || draft
            .emails
            .iter()
            .any(|address| !address.trim().is_empty())
}

/// Builds the engine's [`ContactDraft`] for one create intent. The card is
/// assembled in the engine's own JSON shape and decoded once: the typed
/// property maps are the engine's, and this facade does not re-export them.
///
/// # Errors
///
/// Returns [`PeopleEditError::Unrepresentable`] when the assembled card does
/// not decode as the engine's own shape.
pub(crate) fn build_create_draft(
    book: &AddressBookRef,
    draft: &ContactDraftInput,
) -> Result<ContactDraft, PeopleEditError> {
    let mut card = serde_json::Map::new();
    card.insert("id".to_owned(), json!("draft"));
    card.insert("address_books".to_owned(), json!([book.book.as_str()]));
    if let Some(name) = &draft.name {
        if !name.given.trim().is_empty()
            || name
                .surname
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty())
        {
            card.insert("name".to_owned(), create_name_value(name));
        }
    }
    if let Some(organisation) = non_empty(draft.organisation.as_deref()) {
        card.insert(
            "organizations".to_owned(),
            single_property("o1", &json!({ "name": organisation })),
        );
    }
    if let Some(role) = non_empty(draft.role.as_deref()) {
        card.insert(
            "titles".to_owned(),
            single_property("t1", &json!({ "name": role })),
        );
    }
    // Blank entries a form leaves behind never become blank properties: the
    // card only carries the values somebody typed.
    let emails: Vec<&str> = draft
        .emails
        .iter()
        .map(|address| address.trim())
        .filter(|address| !address.is_empty())
        .collect();
    if !emails.is_empty() {
        card.insert(
            "emails".to_owned(),
            fresh_properties(&emails, |address| json!({ "address": address }), "e"),
        );
    }
    let phones: Vec<&str> = draft
        .phones
        .iter()
        .map(|number| number.trim())
        .filter(|number| !number.is_empty())
        .collect();
    if !phones.is_empty() {
        card.insert(
            "phones".to_owned(),
            fresh_properties(&phones, |number| json!({ "number": number }), "p"),
        );
    }
    let card: ContactCard = serde_json::from_value(Value::Object(card))
        .map_err(|_| PeopleEditError::Unrepresentable)?;
    Ok(ContactDraft {
        address_book: book.book.clone(),
        card,
    })
}

/// Builds the engine's [`ContactPatch`] for one edit intent: only the fields
/// that changed, each rebuilt on top of the property the card carries.
pub(crate) fn build_patch(base: &ContactCard, patch: &ContactPatchInput) -> ContactPatch {
    let mut engine_patch = ContactPatch::default();
    if let Some(name) = &patch.name {
        let (base_given, base_surname) = base_name_parts(base);
        let surname = non_empty(name.surname.as_deref());
        if name.given != base_given || surname != base_surname.as_deref() {
            let value = name_value(base, name);
            engine_patch
                .fields
                .insert(ContactField::Name, FieldPatch::Set(value));
        }
    }
    if let Some(organisation) = patch.organisation.as_deref() {
        let current = base_first_flat_value(base, "organizations", "name");
        patch_flat_field(
            &mut engine_patch,
            &ORGANISATION,
            organisation,
            current.as_deref(),
            base,
        );
    }
    if let Some(role) = patch.role.as_deref() {
        let current = base_first_flat_value(base, "titles", "name");
        patch_flat_field(&mut engine_patch, &ROLE, role, current.as_deref(), base);
    }
    if let Some(emails) = &patch.emails {
        let current: Vec<String> = base
            .emails
            .values()
            .map(|property| property.value.address.clone())
            .collect();
        if emails != &current {
            let values: Vec<Value> = emails
                .iter()
                .map(|address| json!({ "address": address }))
                .collect();
            engine_patch.fields.insert(
                ContactField::Emails,
                FieldPatch::Set(rebuilt_properties(base, "emails", &values, "e")),
            );
        }
    }
    if let Some(phones) = &patch.phones {
        let current: Vec<String> = base
            .phones
            .values()
            .map(|property| property.value.number.clone())
            .collect();
        if phones != &current {
            let values: Vec<Value> = phones
                .iter()
                .map(|number| json!({ "number": number }))
                .collect();
            engine_patch.fields.insert(
                ContactField::Phones,
                FieldPatch::Set(rebuilt_properties(base, "phones", &values, "p")),
            );
        }
    }
    engine_patch
}

/// `Some` for a non-blank string, `None` for one that only holds whitespace.
fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

/// One new property under `id`, carrying `value` and nothing else.
fn single_property(id: &str, value: &Value) -> Value {
    json!({ id: { "value": value } })
}

/// A whole new property map: one property per value, ids minted `prefix1`
/// onward, no metadata to carry.
fn fresh_properties(values: &[&str], render: impl Fn(&str) -> Value, prefix: &str) -> Value {
    let properties = values
        .iter()
        .enumerate()
        .map(|(at, value)| {
            (
                format!("{prefix}{}", at + 1),
                json!({ "value": render(value) }),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    Value::Object(properties)
}

/// The create path's name value: structured when the form holds a surname,
/// formatted alone when it does not, per the surname rule.
fn create_name_value(name: &ContactNameInput) -> Value {
    match non_empty(name.surname.as_deref()) {
        Some(surname) => {
            let mut components = vec![
                json!({ "kind": "given", "value": name.given }),
                json!({ "kind": "surname", "value": surname }),
            ];
            components.retain(|component| {
                component["value"]
                    .as_str()
                    .is_some_and(|value| !value.trim().is_empty())
            });
            let full = components
                .iter()
                .filter_map(|component| component["value"].as_str())
                .collect::<Vec<_>>()
                .join(" ");
            json!({ "full": full, "components": components })
        }
        None => json!({ "full": name.given, "components": [] }),
    }
}

/// The card's given and surname as the form holds them. A card with
/// components compares by its given and surname components; a card with a
/// formatted name only compares that whole name against the given field,
/// because that is the field the editor seeded it into. Read through the
/// card's own JSON, because the component kinds are engine types this
/// facade does not re-export.
pub(crate) fn base_name_parts(base: &ContactCard) -> (String, Option<String>) {
    let name = serde_json::to_value(&base.name).unwrap_or(Value::Null);
    let components = name.get("components").and_then(Value::as_array);
    let join = |kind: &str| -> Option<String> {
        let joined: Vec<&str> = components
            .into_iter()
            .flatten()
            .filter(|component| component.get("kind").and_then(Value::as_str) == Some(kind))
            .filter_map(|component| component.get("value").and_then(Value::as_str))
            .collect();
        (!joined.is_empty()).then(|| joined.join(" "))
    };
    if components.is_some_and(|list| !list.is_empty()) {
        (join("given").unwrap_or_default(), join("surname"))
    } else {
        (
            name.get("full")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            None,
        )
    }
}

/// The name value one edit writes, rebuilt on top of the card's own name:
/// with a surname, the structured form preserving every component the form
/// cannot state (prefixes, middles, suffixes, and the rest of the name's
/// own metadata); without one, the formatted form alone.
fn name_value(base: &ContactCard, name: &ContactNameInput) -> Value {
    let mut object = serde_json::to_value(&base.name)
        .unwrap_or(Value::Null)
        .as_object()
        .cloned()
        .unwrap_or_default();
    if let Some(surname) = non_empty(name.surname.as_deref()) {
        let empty = json!([]);
        let (prefixes, middles, suffixes) =
            split_components(object.get("components").unwrap_or(&empty));
        let mut components = prefixes;
        if !name.given.trim().is_empty() {
            components.push(json!({ "kind": "given", "value": name.given }));
        }
        components.extend(middles);
        components.push(json!({ "kind": "surname", "value": surname }));
        components.extend(suffixes);
        let full = components
            .iter()
            .filter_map(|component| component["value"].as_str())
            .collect::<Vec<_>>()
            .join(" ");
        object.insert("components".to_owned(), Value::Array(components));
        object.insert("full".to_owned(), json!(full));
    } else {
        object.insert("components".to_owned(), json!([]));
        object.insert("full".to_owned(), json!(name.given));
    }
    Value::Object(object)
}

/// The card name's components split into what survives a structured rebuild:
/// the prefixes, the components the form cannot state (middles and the rest,
/// kept in their order), and the suffixes. The given and surname components
/// are the two the rebuild replaces.
fn split_components(components: &Value) -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    let mut prefixes = Vec::new();
    let mut middles = Vec::new();
    let mut suffixes = Vec::new();
    for component in components.as_array().into_iter().flatten() {
        match component.get("kind").and_then(Value::as_str) {
            Some("prefix") => prefixes.push(component.clone()),
            Some("suffix") => suffixes.push(component.clone()),
            Some("given" | "surname") => {}
            _ => middles.push(component.clone()),
        }
    }
    (prefixes, middles, suffixes)
}

/// The first property's flat value of one card field (`"name"` inside an
/// organisation or title, say), or `None` when the field carries nothing.
pub(crate) fn base_first_flat_value(base: &ContactCard, field: &str, key: &str) -> Option<String> {
    serde_json::to_value(field_of(base, field))
        .unwrap_or(Value::Null)
        .as_object()
        .and_then(|properties| properties.values().next())
        .and_then(|property| property.get("value"))
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// One flat editor field: the patch key it writes, where it lives on the
/// card, the value's inner key, and the property id a card with none of
/// these gets.
struct FlatField {
    /// The engine's patch key.
    field: ContactField,
    /// The card's own property-map field name.
    base: &'static str,
    /// The value's inner key (`"name"` inside an organisation or title).
    key: &'static str,
    /// The minted property id.
    mint: &'static str,
}

const ORGANISATION: FlatField = FlatField {
    field: ContactField::Organizations,
    base: "organizations",
    key: "name",
    mint: "o1",
};

const ROLE: FlatField = FlatField {
    field: ContactField::Titles,
    base: "titles",
    key: "name",
    mint: "t1",
};

/// One flat field's patch entry: cleared when the form cleared it (and the
/// card carried one to clear), replaced when it changed, left out when it
/// matches what the card already carries.
fn patch_flat_field(
    engine_patch: &mut ContactPatch,
    flat: &FlatField,
    stated: &str,
    current: Option<&str>,
    base: &ContactCard,
) {
    match non_empty(Some(stated)) {
        _ if current == Some(stated) => {}
        None => {
            if current.is_some() {
                engine_patch.fields.insert(flat.field, FieldPatch::Clear);
            }
        }
        Some(value) => {
            let rebuilt =
                rebuilt_properties(base, flat.base, &[json!({ flat.key: value })], flat.mint);
            engine_patch
                .fields
                .insert(flat.field, FieldPatch::Set(rebuilt));
        }
    }
}

/// One list field's rebuilt property map: the card's own properties in
/// order, their metadata (contexts, preferences, labels) riding untouched,
/// with the new values in their new order and ids minted `prefix1` onward
/// for positions the card does not carry.
fn rebuilt_properties(base: &ContactCard, field: &str, values: &[Value], prefix: &str) -> Value {
    let existing: Vec<(String, Value)> = serde_json::to_value(field_of(base, field))
        .unwrap_or(Value::Null)
        .as_object()
        .map(|properties| {
            properties
                .iter()
                .map(|(id, property)| (id.clone(), property.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut rebuilt = serde_json::Map::new();
    for (at, value) in values.iter().enumerate() {
        let (id, mut property) = existing
            .get(at)
            .cloned()
            .unwrap_or_else(|| (format!("{prefix}{}", at + 1), json!({})));
        if let Some(property_object) = property.as_object_mut() {
            property_object.insert("value".to_owned(), value.clone());
        }
        rebuilt.insert(id, property);
    }
    Value::Object(rebuilt)
}

/// One card field serialised to its JSON shape: the property maps and the
/// name, the two forms the rebuild reads.
fn field_of(base: &ContactCard, field: &str) -> Value {
    match field {
        "emails" => serde_json::to_value(&base.emails).unwrap_or(Value::Null),
        "phones" => serde_json::to_value(&base.phones).unwrap_or(Value::Null),
        "organizations" => serde_json::to_value(&base.organizations).unwrap_or(Value::Null),
        "titles" => serde_json::to_value(&base.titles).unwrap_or(Value::Null),
        "name" => serde_json::to_value(&base.name).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}
