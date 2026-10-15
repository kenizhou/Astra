// SPDX-License-Identifier: GPL-3.0-only
//! The host half of throttling: one policy every provider gets, one
//! **ceiling per account**, and the log line that explains a pause a person
//! would otherwise just experience.
//!
//! The waiting itself is the engine's, in one place for every HTTP
//! provider. What cannot be the engine's is the log: it writes none, by
//! design, so a throttle reaches a diagnostic log only through an observer
//! a host implements. Without one, a mail server slowing the app down
//! looks exactly like the app being slow.
//!
//! What also cannot be the engine's is **where an account ends**. A server
//! that limits concurrency counts requests against the account, not
//! against whichever provider object happened to send them, so the engine's
//! `RequestGate` has to be built by whoever knows which providers belong
//! together: this file. The *number* stays the engine's, since each adapter
//! narrows the gate to what its own server allows as it connects, so
//! nothing here names a provider's limit.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use engine_api::{AccountId, RequestGate, RetryConfig, ThrottleEvent, ThrottleObserver};

/// The throttling policy every provider connected for this account is
/// given.
///
/// The policy and the observer are the same for every account and built
/// once. The **gate** is not: it is this account's, shared with every other
/// provider connected for it (its folder-bound mail providers, its
/// calendar, its contacts) and shared across runtimes, because a second
/// runtime is a thing a host produces (see
/// `graph::token_source::shared`). A bound per source would mean one
/// ceiling per source against a server that counts requests per account.
pub(crate) fn account_retry(account: &AccountId) -> RetryConfig {
    shared_policy().clone().gated(gate_for(account))
}

fn shared_policy() -> &'static RetryConfig {
    static RETRY: OnceLock<RetryConfig> = OnceLock::new();
    RETRY.get_or_init(|| RetryConfig::default().with_observer(Arc::new(LogThrottles)))
}

/// Every account's gate, so two runtimes share one ceiling rather than one
/// each.
///
/// Held strongly rather than weakly, unlike the credential registry beside
/// it: a gate holds no secret, so an account the person removes leaves
/// behind two words rather than a token, and the alternative is handing out
/// a ceiling that a race could drop and rebuild mid-sync.
fn gate_for(account: &AccountId) -> RequestGate {
    static GATES: OnceLock<Mutex<HashMap<String, RequestGate>>> = OnceLock::new();
    GATES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("request gate registry poisoned")
        .entry(account.as_str().to_owned())
        .or_default()
        .clone()
}

/// Writes every throttle to the app log.
struct LogThrottles;

impl ThrottleObserver for LogThrottles {
    fn throttled(&self, event: &ThrottleEvent<'_>) {
        let (level, line) = describe(event);
        log::log!(level, "{line}");
    }
}

/// The log record one event becomes.
///
/// Separated from writing it so the wording and the level are asserted
/// directly; installing a logger to read them back would be a
/// process-global the rest of the suite shares.
fn describe(event: &ThrottleEvent<'_>) -> (log::Level, String) {
    let provider = event.provider;
    let millis = event.delay.as_millis();
    let tries = event.attempt.saturating_add(1);
    if event.gave_up {
        // The one a slow sync is explained by, so it is a warning: the pass stopped early
        // and the rest waits for the next one.
        //
        // Where the server named an instant, say it. That is the whole difference between a
        // line a reader can act on and one that only reports a stall: the engine declines a
        // long wait rather than sleeping a task through it, so on this path `delay` is
        // usually nothing and the instant is the only number worth printing.
        let line = match event.stated {
            Some(stated) => format!(
                "{provider}: still limiting us after {tries} tries; it says {}s before it \
                 will take more, so the rest waits for the next sync",
                stated.as_secs(),
            ),
            None => format!(
                "{provider}: still limiting us after {tries} tries and {millis}ms of \
                 waiting; the rest waits for the next sync"
            ),
        };
        return (log::Level::Warn, line);
    }
    let next = event.attempt.saturating_add(2);
    let line = match event.stated {
        // `delay` is the server's figure plus jitter, so the two differ by a little and
        // printing the one it asked for is the honest half.
        Some(stated) => format!(
            "{provider}: limiting how fast we can fetch; it asked for {}ms before try {next}",
            stated.as_millis(),
        ),
        None => {
            format!(
                "{provider}: limiting how fast we can fetch; waiting {millis}ms before try {next}"
            )
        }
    };
    (log::Level::Info, line)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use engine_api::ThrottleEvent;

    use super::describe;

    fn event() -> ThrottleEvent<'static> {
        ThrottleEvent {
            provider: "gmail",
            status: 429,
            attempt: 0,
            delay: Duration::from_millis(750),
            stated: None,
            gave_up: false,
        }
    }

    #[test]
    fn an_absorbed_wait_is_reported_without_alarming_anyone() {
        let (level, line) = describe(&event());
        assert_eq!(level, log::Level::Info);
        assert!(line.contains("gmail"), "{line}");
        assert!(line.contains("750ms"), "{line}");
        assert!(line.contains("try 2"), "{line}");
    }

    #[test]
    fn a_server_that_named_its_own_delay_says_so() {
        // The figure printed is the server's, not the one the engine will sleep. Those
        // differ by the jitter the engine adds above it, and quoting the server back to
        // itself is what makes the line checkable against the provider's own docs.
        let (_, line) = describe(&ThrottleEvent {
            stated: Some(Duration::from_secs(30)),
            delay: Duration::from_millis(30_400),
            ..event()
        });
        assert!(line.contains("it asked for 30000ms"), "{line}");
    }

    #[test]
    fn giving_up_on_a_named_instant_says_when_rather_than_how_little_it_waited() {
        // The engine declines a long wait instead of sleeping a task through it, so
        // `delay` here is nothing at all; a line reporting that would say "0ms of
        // waiting" and leave a reader none the wiser.
        let (level, line) = describe(&ThrottleEvent {
            gave_up: true,
            attempt: 0,
            delay: Duration::ZERO,
            stated: Some(Duration::from_secs(47)),
            ..event()
        });
        assert_eq!(level, log::Level::Warn);
        assert!(line.contains("47s"), "{line}");
        assert!(!line.contains("0ms"), "{line}");
    }

    #[test]
    fn giving_up_is_a_warning_and_says_the_work_is_not_lost() {
        let (level, line) = describe(&ThrottleEvent {
            gave_up: true,
            attempt: 4,
            ..event()
        });
        assert_eq!(level, log::Level::Warn);
        assert!(line.contains("5 tries"), "{line}");
        assert!(line.contains("next sync"), "{line}");
    }

    #[test]
    fn one_account_has_one_ceiling_however_many_runtimes_ask_for_it() {
        // A bound per source means a host that builds the runtime twice gets two
        // ceilings against a mailbox that allows one limit in total, and a measurable
        // share of its requests refused. Two independent asks must land on one gate.
        let account = engine_api::AccountId::try_from("throttle-scope").unwrap();
        let first = super::gate_for(&account);
        let second = super::gate_for(&account);

        first.narrow_to(4);

        assert_eq!(
            second.limit(),
            Some(4),
            "the second runtime got a gate of its own, so the account has two ceilings",
        );
    }

    #[test]
    fn two_accounts_do_not_share_a_ceiling() {
        // The opposite error, and just as wrong: one server's limit applied to another's
        // account is a throughput cut nobody asked for. Without this, a single
        // process-wide gate would pass the test above.
        let one = super::gate_for(&engine_api::AccountId::try_from("acct-one").unwrap());
        let two = super::gate_for(&engine_api::AccountId::try_from("acct-two").unwrap());

        one.narrow_to(4);

        assert_eq!(one.limit(), Some(4));
        assert_eq!(two.limit(), None, "one account's ceiling bound another's");
    }

    #[test]
    fn no_line_can_name_the_users_mail() {
        // The event carries no URL by construction; this locks the log line to the same
        // rule, since a request path names a mailbox or a message.
        for gave_up in [false, true] {
            for stated in [None, Some(Duration::from_secs(30))] {
                let (_, line) = describe(&ThrottleEvent {
                    gave_up,
                    stated,
                    ..event()
                });
                assert!(!line.contains("http"), "{line}");
                assert!(!line.contains('@'), "{line}");
            }
        }
    }
}
