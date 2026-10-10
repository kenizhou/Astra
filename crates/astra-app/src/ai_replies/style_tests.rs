// SPDX-License-Identifier: GPL-3.0-only
//! Style profile tests, over the fixture completion seam and an in-memory
//! profile store: the infer path (the StyleInfer use case under the turn's
//! correlation, the structured shape, the repair path, the save), the
//! built_at daily cap as a pure decision with its boundaries, the force
//! flag the CLI rebuild rides, the no-Sent no-profile rule, the fallback
//! to the stored note on a failed or unparseable infer, and the sample
//! pins (the newest twenty, oldest first, each in its own untrusted wrap,
//! each body capped).

use astra_ai::{AiStore, CorrelationId, LlmUseCase};
use engine_api::AccountId;

use super::style::{
    should_rebuild, style_profile, style_prompt, STYLE_PROFILE_MAX_AGE_SECS, STYLE_PROMPT_VERSION,
    STYLE_SAMPLE_BODY_CAP, STYLE_SAMPLE_LIMIT,
};
use crate::dispatch_fakes::ai::FixtureComplete;

/// The account the profile describes.
fn account() -> AccountId {
    AccountId::try_from("work").unwrap()
}

/// The good fixture: one structured style note.
const GOOD: &str = r#"{"style_note":"Concise and warm; opens with Hi; signs off with Cheers."}"#;

/// `count` sent samples, newest first, each labelled by its newness:
/// `Sample 00` is the newest, `Sample 24` the oldest of twenty-five.
fn samples(count: usize) -> Vec<String> {
    (0..count)
        .map(|at| format!("Sample {at:02}: plain words with a {at}-ish tone."))
        .collect()
}

#[tokio::test]
async fn a_fresh_infer_builds_and_saves_the_profile() {
    let complete = FixtureComplete::scripted(&[GOOD]);
    let store = AiStore::open_in_memory().unwrap();
    let correlation = CorrelationId::generate();
    let note = style_profile(
        &account(),
        &samples(3),
        false,
        &complete,
        &correlation,
        &store,
    )
    .await
    .expect("a first build answers its note");
    assert_eq!(
        note,
        "Concise and warm; opens with Hi; signs off with Cheers."
    );
    assert_eq!(complete.call_count(), 1);
    let call = &complete.calls()[0];
    assert_eq!(call.use_case, LlmUseCase::StyleInfer);
    assert_eq!(call.correlation, correlation);
    let saved = store
        .style_profile("work")
        .expect("the read runs")
        .expect("the built profile saves");
    assert_eq!(saved.note, note);
    assert!(saved.built_at > 0, "the store stamps the build itself");
}

#[tokio::test]
async fn a_fresh_profile_answers_without_a_call() {
    let complete = FixtureComplete::scripted(&[]);
    let store = AiStore::open_in_memory().unwrap();
    store
        .save_style_profile("work", "The stored note.")
        .expect("the profile saves");
    let note = style_profile(
        &account(),
        &samples(3),
        false,
        &complete,
        &CorrelationId::generate(),
        &store,
    )
    .await
    .expect("the stored note answers");
    assert_eq!(note, "The stored note.");
    assert_eq!(
        complete.call_count(),
        0,
        "a profile built within the day spends no infer call",
    );
}

#[tokio::test]
async fn force_rebuilds_even_when_fresh() {
    let complete = FixtureComplete::scripted(&[GOOD]);
    let store = AiStore::open_in_memory().unwrap();
    store
        .save_style_profile("work", "The stored note.")
        .expect("the profile saves");
    let note = style_profile(
        &account(),
        &samples(3),
        true,
        &complete,
        &CorrelationId::generate(),
        &store,
    )
    .await
    .expect("the forced rebuild answers its note");
    assert_eq!(
        note,
        "Concise and warm; opens with Hi; signs off with Cheers."
    );
    assert_eq!(complete.call_count(), 1, "force bypasses the daily cap");
    let saved = store
        .style_profile("work")
        .expect("the read runs")
        .expect("the rebuild replaces the row");
    assert_eq!(saved.note, note, "the newest build wins");
}

#[tokio::test]
async fn no_sent_mail_leaves_no_profile() {
    let complete = FixtureComplete::scripted(&[]);
    let store = AiStore::open_in_memory().unwrap();
    let note = style_profile(
        &account(),
        &[],
        false,
        &complete,
        &CorrelationId::generate(),
        &store,
    )
    .await;
    assert_eq!(note, None, "no Sent mail, no profile");
    assert_eq!(complete.call_count(), 0);
    assert!(
        store.style_profile("work").expect("runs").is_none(),
        "nothing was saved",
    );
}

#[tokio::test]
async fn no_sent_mail_keeps_the_stored_profile() {
    let complete = FixtureComplete::scripted(&[]);
    let store = AiStore::open_in_memory().unwrap();
    store
        .save_style_profile("work", "The stored note.")
        .expect("the profile saves");
    let note = style_profile(
        &account(),
        &[],
        true,
        &complete,
        &CorrelationId::generate(),
        &store,
    )
    .await;
    assert_eq!(
        note.as_deref(),
        Some("The stored note."),
        "an empty Sent folder never erases a built profile",
    );
    assert_eq!(complete.call_count(), 0);
}

#[tokio::test]
async fn a_failed_infer_falls_back_to_the_stored_profile() {
    let complete = FixtureComplete::failing("llm unreachable: all entries failed");
    let store = AiStore::open_in_memory().unwrap();
    store
        .save_style_profile("work", "The stored note.")
        .expect("the profile saves");
    let note = style_profile(
        &account(),
        &samples(3),
        true,
        &complete,
        &CorrelationId::generate(),
        &store,
    )
    .await;
    assert_eq!(
        note.as_deref(),
        Some("The stored note."),
        "a stale note beats no note",
    );
    let saved = store
        .style_profile("work")
        .expect("runs")
        .expect("the stored row stands");
    assert_eq!(saved.note, "The stored note.");
}

#[tokio::test]
async fn a_failed_infer_with_no_stored_profile_answers_none() {
    let complete = FixtureComplete::failing("llm unreachable: all entries failed");
    let store = AiStore::open_in_memory().unwrap();
    let note = style_profile(
        &account(),
        &samples(3),
        false,
        &complete,
        &CorrelationId::generate(),
        &store,
    )
    .await;
    assert_eq!(note, None);
}

#[tokio::test]
async fn an_unparseable_infer_falls_back_to_the_stored_profile() {
    let complete = FixtureComplete::scripted(&["no json here at all"]);
    let store = AiStore::open_in_memory().unwrap();
    store
        .save_style_profile("work", "The stored note.")
        .expect("the profile saves");
    let note = style_profile(
        &account(),
        &samples(3),
        true,
        &complete,
        &CorrelationId::generate(),
        &store,
    )
    .await;
    assert_eq!(note.as_deref(), Some("The stored note."));
    let saved = store
        .style_profile("work")
        .expect("runs")
        .expect("the stored row stands");
    assert_eq!(saved.note, "The stored note.");
}

#[tokio::test]
async fn prose_around_the_json_takes_the_repair_path() {
    let answer = format!("Sure, the style note:\n{GOOD}\nHope that helps!");
    let complete = FixtureComplete::scripted(&[&answer]);
    let store = AiStore::open_in_memory().unwrap();
    let note = style_profile(
        &account(),
        &samples(3),
        false,
        &complete,
        &CorrelationId::generate(),
        &store,
    )
    .await
    .expect("the repaired completion infers");
    assert_eq!(
        note,
        "Concise and warm; opens with Hi; signs off with Cheers."
    );
}

#[test]
fn the_daily_cap_decision_is_pinned() {
    let now = 1_700_000_000;
    let row = |built_at: i64| astra_ai::StyleProfileRow {
        account: "work".to_owned(),
        note: "The stored note.".to_owned(),
        built_at,
    };
    assert!(
        should_rebuild(None, false, now),
        "no stored profile always rebuilds",
    );
    let fresh = now - 3_600;
    assert!(
        !should_rebuild(Some(&row(fresh)), false, now),
        "a profile built an hour ago is fresh",
    );
    let exactly_a_day = now - STYLE_PROFILE_MAX_AGE_SECS;
    assert!(
        !should_rebuild(Some(&row(exactly_a_day)), false, now),
        "the cap is inclusive: a profile exactly 24h old is still fresh",
    );
    let past_the_day = now - STYLE_PROFILE_MAX_AGE_SECS - 1;
    assert!(
        should_rebuild(Some(&row(past_the_day)), false, now),
        "a profile older than 24h rebuilds",
    );
    let skewed = now + 600;
    assert!(
        !should_rebuild(Some(&row(skewed)), false, now),
        "a future stamp (clock skew) reads fresh, spending nothing",
    );
    assert!(
        should_rebuild(Some(&row(fresh)), true, now),
        "force rebuilds even a fresh profile",
    );
}

#[test]
fn the_sample_cap_keeps_the_newest_twenty_oldest_first() {
    let prompt = style_prompt(&samples(STYLE_SAMPLE_LIMIT + 5));
    for dropped in 20..25 {
        assert!(
            !prompt.contains(&format!("Sample {dropped:02}")),
            "only the newest {STYLE_SAMPLE_LIMIT} samples ride:\n{prompt}",
        );
    }
    // The kept samples render oldest first: one strictly descending run of
    // newness, so the model reads the writing in the order it happened.
    let mut last = None;
    for newness in (0..STYLE_SAMPLE_LIMIT).rev() {
        let label = format!("Sample {newness:02}");
        let at = prompt
            .find(&label)
            .unwrap_or_else(|| panic!("{label} renders:\n{prompt}"));
        if let Some(before) = last {
            assert!(
                at > before,
                "{label} must follow the older sample before it:\n{prompt}",
            );
        }
        last = Some(at);
    }
}

#[test]
fn each_sample_body_is_capped_with_the_marker() {
    let long = format!("{}TAILMARK", "x".repeat(STYLE_SAMPLE_BODY_CAP + 50));
    let prompt = style_prompt(&[long]);
    assert!(
        prompt.contains("[truncated]"),
        "an over-cap sample carries the truncation marker:\n{prompt}",
    );
    assert!(
        !prompt.contains("TAILMARK"),
        "the text past the cap never rides:\n{prompt}",
    );
}

#[test]
fn every_sample_rides_its_own_untrusted_wrap() {
    let prompt = style_prompt(&samples(3));
    assert_eq!(
        prompt.matches("[[[untrusted:begin]]]").count(),
        3,
        "every sent sample rides inside its own wrap",
    );
    let first_begin = prompt
        .find("[[[untrusted:begin]]]")
        .expect("the wrap opens");
    let schema_at = prompt
        .find(r#"{"style_note":"..."}"#)
        .expect("the policy rides");
    assert!(
        schema_at < first_begin,
        "the trusted instruction and policy ride ahead of the mail:\n{prompt}",
    );
}

#[test]
fn the_style_prompt_pins_its_shape() {
    let prompt = style_prompt(&samples(1));
    for word in ["tone", "formality", "sign-off"] {
        assert!(
            prompt.to_lowercase().contains(word),
            "the note's dimensions teach {word}:\n{prompt}",
        );
    }
    assert!(
        prompt.contains(r#"{"style_note":"..."}"#),
        "the schema example rides:\n{prompt}",
    );
    assert_eq!(STYLE_PROMPT_VERSION, "1", "the pipeline constant is pinned");
}
