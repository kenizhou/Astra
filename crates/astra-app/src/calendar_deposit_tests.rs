// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the stored-copy leg's pure halves: the `METHOD` strip that makes
//! an invitation storable and the resource address a stored invitation lands
//! at.

use super::{event_href, for_storage};

#[test]
fn the_method_strip_removes_only_the_method_line() {
    let raw = concat!(
        "BEGIN:VCALENDAR\r\n",
        "VERSION:2.0\r\n",
        "METHOD:REQUEST\r\n",
        "BEGIN:VEVENT\r\n",
        "UID:meeting-1@test.local\r\n",
        "END:VEVENT\r\n",
        "END:VCALENDAR\r\n",
    );
    assert_eq!(
        for_storage(raw).as_deref(),
        Some(concat!(
            "BEGIN:VCALENDAR\r\n",
            "VERSION:2.0\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:meeting-1@test.local\r\n",
            "END:VEVENT\r\n",
            "END:VCALENDAR\r\n",
        )),
        "everything but the METHOD line survives byte for byte: the VTIMEZONE, \
         the X- properties, the ATTENDEE lines that are the whole point"
    );
    // Case and surrounding blanks do not hide a METHOD line from the strip.
    assert_eq!(
        for_storage("BEGIN:VCALENDAR\r\nmethod:publish\r\nEND:VCALENDAR\r\n").as_deref(),
        Some("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n")
    );
}

#[test]
fn the_method_strip_refuses_what_is_not_a_calendar() {
    assert_eq!(
        for_storage("Subject: not a calendar\r\nMETHOD:REQUEST\r\n"),
        None,
        "bytes with no VCALENDAR are not a calendar object resource, and \
         storing them as one would corrupt the collection"
    );
}

#[test]
fn a_stored_invitation_lands_at_the_collection_uid_address() {
    let href = event_href("cal-1", "meeting-1@test.local").unwrap();
    assert_eq!(
        href.key().as_str(),
        // Only the unreserved set crosses bare (RFC 3986 §2.3); the `@` of an
        // address encodes, which stays a valid and stable spelling of the
        // same segment.
        "cal-1/meeting-1%40test.local.ics",
        "the universal convention, which is what makes the guarded create's \
         precondition worth having: a server that deposited its own copy put \
         it at the same address"
    );
}

#[test]
fn the_address_percent_encodes_the_opaque_uid() {
    // A UID is opaque: nothing forbids a slash, a space, or a non-ASCII
    // letter, and every one of them would change the address the resource
    // lands at if it crossed unencoded (RFC 3986 §2.3).
    let href = event_href("dav/cal/", "a b/c@d").unwrap();
    assert_eq!(href.key().as_str(), "dav/cal/a%20b%2Fc%40d.ics");
    let href = event_href("dav/cal", "tëst").unwrap();
    assert_eq!(href.key().as_str(), "dav/cal/t%C3%ABst.ics");
}
