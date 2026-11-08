// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the showcase boot: the screen picker's spellings and its
//! refusal of a name no surface answers, the fixture app's recorded
//! answers for the named screen with every other pull at its honest
//! empty, and the shell's assembly over the fixture app.

use std::sync::Arc;

use astra_app::Intent;
use astra_viewmodel::{PeopleSnapshot, SettingsSnapshot};

use super::app::fixture_app;
use super::{assemble, requested, screen_from_reader, Screen};

/// A sink that drops every emission: the showcase's agent leg starts no
/// sidecar, so nothing stands there to emit.
struct QuietSink;

impl crate::agent::AgentEventSink for QuietSink {
    fn emit_agent_event(&self, _event: crate::agent::AgentStreamEvent) {}
    fn emit_sidecar_ready(&self, _models: Vec<String>, _harnesses: Vec<String>) {}
    fn emit_sidecar_lost(&self, _attempts: u32, _message: &str) {}
    fn emit_permission_request(&self, _approval: crate::agent::PendingApproval) {}
    fn emit_permission_resolved(&self, _resolved: crate::agent::PermissionResolved) {}
}

#[test]
fn the_screen_spellings_are_matched_literally() {
    assert_eq!(screen_from_reader(Some("list")), Ok(Some(Screen::List)));
    assert_eq!(
        screen_from_reader(Some(" Reading ")),
        Ok(Some(Screen::Reading))
    );
    assert_eq!(
        screen_from_reader(Some("settings")),
        Ok(Some(Screen::Settings))
    );
    assert_eq!(screen_from_reader(Some("people")), Ok(Some(Screen::People)));
}

#[test]
fn unset_and_off_values_read_as_no_showcase() {
    assert_eq!(screen_from_reader(None), Ok(None));
    for off in ["", "0", "false", "no", "off"] {
        assert_eq!(
            screen_from_reader(Some(off)),
            Ok(None),
            "the value {off} is off"
        );
    }
}

#[test]
fn an_unknown_screen_is_refused_with_the_name() {
    // A name no surface answers is refused at the launch rather than
    // silently photographed as the list: a well-lit capture of the wrong
    // screen is the failure nothing downstream can detect.
    assert_eq!(screen_from_reader(Some("agenda")), Err("agenda".to_owned()));
    assert_eq!(
        screen_from_reader(Some("compose")),
        Err("compose".to_owned())
    );
}

#[test]
fn requested_reads_the_launch_environment() {
    std::env::remove_var("ASTRA_SHOWCASE");
    assert_eq!(requested(), Ok(None));
    std::env::set_var("ASTRA_SHOWCASE", "reading");
    assert_eq!(requested(), Ok(Some(Screen::Reading)));
    std::env::remove_var("ASTRA_SHOWCASE");
}

#[test]
fn every_screen_answers_the_recorded_mailbox_list() {
    // The mailbox list is the shell's spine: without it the account-setup
    // flow opens over every screenshot, whatever screen was named.
    for screen in [
        Screen::List,
        Screen::Reading,
        Screen::Settings,
        Screen::People,
    ] {
        let app = fixture_app(screen);
        let snapshot = app.mailbox_list();
        assert_eq!(
            snapshot.account_folders.len(),
            2,
            "the recorded accounts stand"
        );
        assert_eq!(
            snapshot.rows.len(),
            2,
            "the recorded rows stand for {screen:?}"
        );
        assert_eq!(snapshot.rows[0].subject, "Quarterly report");
    }
}

#[test]
fn the_list_screen_leaves_every_other_surface_at_its_empty() {
    let app = fixture_app(Screen::List);
    assert_eq!(app.reading(), None, "no message stands open");
    assert_eq!(app.settings(), SettingsSnapshot::default());
    assert_eq!(app.people(), PeopleSnapshot::default());
}

#[test]
fn the_reading_screen_opens_the_recorded_message_at_rest() {
    // The hydration pull reads the reading surface at mount, so a recorded
    // snapshot at rest opens the pane with the list highlight already on
    // the message it names: no dispatch, no driver.
    let app = fixture_app(Screen::Reading);
    let reading = app.reading().expect("the recorded message stands open");
    assert_eq!(reading.subject, "Quarterly report");
    assert_eq!(reading.from_name, "Ada Lovelace");
    assert_eq!(reading.attachments.len(), 1);
    let row = &app.mailbox_list().rows[0];
    let astra_viewmodel::RowKey::Message { key, .. } = &row.key else {
        panic!("the recorded row is a single message");
    };
    assert_eq!(
        key.as_str(),
        reading.message.key.as_str(),
        "the open message is the list's first row",
    );
}

#[test]
fn the_settings_screen_answers_the_recorded_settings() {
    let app = fixture_app(Screen::Settings);
    let settings = app.settings();
    assert_eq!(settings.settings.zone, "Europe/Amsterdam");
    assert!(settings.settings.twenty_four_clock);
    assert_eq!(app.reading(), None, "the settings screen opens no message");
}

#[test]
fn the_people_screen_answers_the_recorded_rows() {
    let app = fixture_app(Screen::People);
    let people = app.people();
    assert_eq!(people.rows.len(), 1, "the recorded person stands");
    assert_eq!(people.rows[0].display_name, "Ada Lovelace");
}

#[test]
fn every_dispatch_settles_and_the_async_pulls_answer_their_empties() {
    let app = fixture_app(Screen::List);
    let account = engine_api::AccountId::try_from("work").expect("an account id");
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");

    let dispatched = runtime.block_on(|| async { app.dispatch(Intent::RefreshMail).await });
    assert!(
        dispatched.is_ok(),
        "a dispatch settles into the fixture app"
    );

    let chips = runtime.block_on(|| async {
        let key = engine_api::ProviderKey::new("imap:v1:u42").expect("a provider key");
        app.draft_chips(&account, &key).await
    });
    assert_eq!(chips, [] as [astra_app::Chip; 0]);
    assert_eq!(app.snippet_list(), [] as [astra_app::Snippet; 0]);
    assert_eq!(
        app.knowledge_list(&account),
        [] as [astra_app::KnowledgeView; 0]
    );
    let composition = astra_app::CompositionId::new("c-1".to_owned()).expect("a composition id");
    let resumed =
        runtime.block_on(|| async { app.resume_latest_draft(&account, composition).await });
    assert!(resumed.is_none());
    assert_eq!(app.sending().rows, [] as [astra_viewmodel::QueuedRow; 0]);
}

#[test]
fn the_shell_assembles_over_the_fixture_app() {
    let dir = crate::commands_fakes::scratch("showcase-assemble");
    let state =
        assemble(&dir, Screen::Reading, Arc::new(QuietSink)).expect("the showcase state assembles");

    // The showcase adopts no account: no dial ran, so no delivery loop or
    // calendar timer finds an id to adopt.
    assert!(
        state.connections.ids().is_empty(),
        "no account stands dialled"
    );
    assert_eq!(
        state
            .app
            .reading()
            .expect("the fixture app answers")
            .subject,
        "Quarterly report",
    );
    assert_eq!(state.app.mailbox_list().rows.len(), 2);
}
