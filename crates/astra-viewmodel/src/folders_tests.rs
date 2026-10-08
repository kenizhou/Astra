// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the folder projection: canonical ordering, the drawn tree shape, unread
//! counts, expansion defaults, and the wire form of the role enum.

use engine_api::{Mailbox, MailboxId, MailboxRole};

use super::*;

fn key(value: &str) -> MailboxId {
    MailboxId::try_from(value).unwrap()
}

/// A roleless top-level folder.
fn plain(id: &str, name: &str) -> Mailbox {
    Mailbox::new(key(id), name)
}

/// A folder filed inside another one, the way providers report it: its own name, and the
/// parent's id.
fn nested(id: &str, name: &str, parent: &str) -> Mailbox {
    let mut mailbox = plain(id, name);
    mailbox.parent = Some(key(parent));
    mailbox
}

/// A role-tagged mailbox.
fn roled(id: &str, name: &str, role: MailboxRole) -> Mailbox {
    let mut mailbox = plain(id, name);
    mailbox.role = Some(role);
    mailbox
}

/// A role-tagged mailbox the server has counted.
fn counted(id: &str, name: &str, role: MailboxRole, unread: u32) -> Mailbox {
    let mut mailbox = roled(id, name, role);
    mailbox.unread_count = Some(unread);
    mailbox
}

/// `(key, depth, parent)` per row, which is the shape every tree assertion here is about.
fn shape(rows: &[FolderRow]) -> Vec<(&str, u32, Option<&str>)> {
    rows.iter()
        .map(|row| {
            (
                row.key.as_str(),
                row.depth,
                row.parent.as_ref().map(MailboxId::as_str),
            )
        })
        .collect()
}

#[test]
fn folders_lead_with_special_roles_in_canonical_order_then_custom_by_name() {
    // The provider lists folders in an arbitrary order, mixing special (role-bearing) and
    // custom ones. The sidebar must always lead with the special folders in the fixed
    // canonical order (Inbox, Drafts, Sent, Archive, Junk, Trash), then every other
    // top-level folder by name, identical on every surface.
    let rows = sorted_folder_rows(&[
        plain("Zebra", "Zebra"),
        roled("Trash", "Trash", MailboxRole::Trash),
        plain("apricot", "apricot"),
        roled("Archive", "Archive", MailboxRole::Archive),
        roled("INBOX", "Inbox", MailboxRole::Inbox),
        // An unrecognised SPECIAL-USE role is a custom folder, not a special one.
        roled("Notes", "Notes", MailboxRole::Other("notes".into())),
        roled("Sent", "Sent", MailboxRole::Sent),
    ]);
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            // Special folders, canonical order (Drafts and Junk absent here, so skipped).
            "Inbox", "Sent", "Archive", "Trash",
            // Custom folders, case-insensitive by name (Notes carries an `Other` role).
            "apricot", "Notes", "Zebra",
        ]
    );
}

#[test]
fn a_folder_is_drawn_under_the_one_it_is_filed_in_depth_first() {
    // Listed in an order no pane would draw: a grandchild before its parent, a parent
    // after its own sibling. The projection is what puts each folder immediately beneath
    // the folder that holds it.
    let rows = sorted_folder_rows(&[
        nested("Clients/Acme/2024", "2024", "Clients/Acme"),
        plain("Zebra", "Zebra"),
        nested("Clients/Acme", "Acme", "Clients"),
        plain("Clients", "Clients"),
    ]);

    assert_eq!(
        shape(&rows),
        vec![
            ("Clients", 0, None),
            ("Clients/Acme", 1, Some("Clients")),
            ("Clients/Acme/2024", 2, Some("Clients/Acme")),
            ("Zebra", 0, None),
        ]
    );
    // A disclosure control belongs on the two that hold folders, and on neither of the
    // others.
    let holds: Vec<bool> = rows.iter().map(|row| row.has_children).collect();
    assert_eq!(holds, vec![true, true, false, false]);
}

#[test]
fn siblings_keep_the_canonical_order_within_their_own_branch() {
    // The ordering one level of the pane has, applied to each level rather than to the
    // list as a whole: two branches, each sorted among itself and nowhere else.
    let rows = sorted_folder_rows(&[
        plain("B", "B"),
        plain("A", "A"),
        nested("B/zebra", "zebra", "B"),
        nested("A/zebra", "zebra", "A"),
        nested("B/apricot", "apricot", "B"),
        nested("A/apricot", "apricot", "A"),
    ]);

    let names: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
    assert_eq!(
        names,
        vec!["A", "A/apricot", "A/zebra", "B", "B/apricot", "B/zebra"]
    );
}

#[test]
fn a_role_bearing_folder_is_drawn_at_the_top_however_the_server_files_it() {
    // Gmail files Sent, Drafts and Trash inside a `[Gmail]` container over IMAP. Burying
    // the folder a person reaches for most inside something their provider invented is
    // what no mail client does, so a role-bearing folder is drawn where the role says
    // rather than where the path says.
    let mut sent = roled("[Gmail]/Sent Mail", "Sent Mail", MailboxRole::Sent);
    sent.parent = Some(key("[Gmail]"));
    let mut all = roled("[Gmail]/All Mail", "All Mail", MailboxRole::All);
    all.parent = Some(key("[Gmail]"));
    let rows = sorted_folder_rows(&[
        plain("[Gmail]", "[Gmail]"),
        sent,
        all,
        roled("INBOX", "Inbox", MailboxRole::Inbox),
        nested("[Gmail]/Spam-ish", "Spam-ish", "[Gmail]"),
    ]);

    assert_eq!(
        shape(&rows),
        vec![
            ("INBOX", 0, None),
            ("[Gmail]/Sent Mail", 0, None),
            // `All` ranks after the six named roles but is still a role, so it is hoisted
            // too.
            ("[Gmail]/All Mail", 0, None),
            // The container stays, holding whatever has no role of its own.
            ("[Gmail]", 0, None),
            ("[Gmail]/Spam-ish", 1, Some("[Gmail]")),
        ]
    );
}

#[test]
fn a_folder_whose_parent_the_account_does_not_list_is_drawn_at_the_top() {
    // An unsubscribed or hidden intermediate: the row has to go somewhere, and it cannot
    // go under a row that is not there. Its own name is all it has, which is the trade
    // for not dropping it.
    let rows = sorted_folder_rows(&[nested("Work/Clients", "Clients", "Work")]);

    assert_eq!(shape(&rows), vec![("Work/Clients", 0, None)]);
}

#[test]
fn every_folder_gets_a_row_even_when_the_parents_form_a_cycle() {
    // No server should be able to report this. If one does, a folder missing from the
    // pane is a folder whose mail the user has no way to open, so each still gets a row.
    let rows = sorted_folder_rows(&[nested("A", "A", "B"), nested("B", "B", "A")]);

    let names: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"A") && names.contains(&"B"));
    assert!(rows.iter().all(|row| row.depth == 0));
}

#[test]
fn a_folder_carries_the_servers_count_and_an_uncounted_one_shows_nothing() {
    let rows = sorted_folder_rows(&[
        counted("INBOX", "Inbox", MailboxRole::Inbox, 545),
        counted("Trash", "Trash", MailboxRole::Trash, 4),
        // Counted and empty: a real zero, which renders the same as no badge.
        counted("Sent", "Sent", MailboxRole::Sent, 0),
        // Never counted (Gmail's label list, or an IMAP folder the server refused a
        // STATUS for): collapses to 0, because the surface hides the badge either way.
        roled("Archive", "Archive", MailboxRole::Archive),
    ]);
    let counts: Vec<(&str, u32)> = rows
        .iter()
        .map(|folder| (folder.name.as_str(), folder.unread))
        .collect();
    assert_eq!(
        counts,
        vec![("Inbox", 545), ("Sent", 0), ("Archive", 0), ("Trash", 4)]
    );
}

#[test]
fn the_projection_leaves_expansion_open_for_the_publisher_to_stamp() {
    // A folder that holds folders reads as open, which an untouched one is; the publisher
    // stamps the persisted expansion state over the row before it crosses the IPC, so a
    // rebuild that began before the user's chevron click cannot publish the state as it
    // was then.
    let rows = sorted_folder_rows(&[
        plain("Clients", "Clients"),
        nested("Clients/Acme", "Acme", "Clients"),
    ]);

    assert!(rows[0].expanded);
    // Never "expanded" with nothing inside: there would be no tree to open.
    assert!(!rows[1].expanded);
}

#[test]
fn the_engine_role_maps_onto_the_client_role() {
    assert_eq!(
        folder_role(&roled("INBOX", "Inbox", MailboxRole::Inbox)),
        Some(FolderRole::Inbox)
    );
    assert_eq!(folder_role(&plain("stuff", "Stuff")), None);
    // Every unranked engine role is one client role, so no provider invention can escape
    // the enum.
    assert_eq!(
        folder_role(&roled("flag", "Flagged", MailboxRole::Flagged)),
        Some(FolderRole::Other)
    );
}

#[test]
fn folder_roles_cross_the_wire_as_snake_case_words() {
    for (role, word) in [
        (FolderRole::Inbox, "inbox"),
        (FolderRole::Drafts, "drafts"),
        (FolderRole::Sent, "sent"),
        (FolderRole::Archive, "archive"),
        (FolderRole::Junk, "junk"),
        (FolderRole::Trash, "trash"),
        (FolderRole::Other, "other"),
    ] {
        assert_eq!(
            serde_json::to_string(&role).unwrap(),
            format!("\"{word}\""),
            "{role:?}"
        );
        assert_eq!(
            serde_json::from_str::<FolderRole>(&format!("\"{word}\"")).unwrap(),
            role
        );
    }
}
