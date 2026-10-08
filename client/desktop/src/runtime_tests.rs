// SPDX-License-Identifier: GPL-3.0-only
//! A blocking host call is polled on a thread of our own, whatever thread the
//! host calls it from, and the runtime takes one fewer core than the machine
//! has.

use std::hint::black_box;

use super::{build_runtime, drive, AppRuntime, DRIVER_NAME};

/// The stack a host thread might have: well under what [`deep`] needs.
const HOST_STACK: usize = 512 * 1024;

/// The app runtime under test, panicking if it cannot start.
fn runtime() -> AppRuntime {
    build_runtime().expect("the runtime starts")
}

/// Uses about `frames` x 64 KiB of stack, the way a deep call path does in a
/// debug build. The frame is a plain stack array on purpose: heap-allocating
/// it would leave the test with nothing to observe.
#[allow(clippy::large_stack_arrays)]
fn deep(frames: usize) -> usize {
    let frame = [1u8; 64 * 1024];
    let here = black_box(&frame)
        .iter()
        .map(|&b| usize::from(b))
        .sum::<usize>();
    if frames == 0 {
        here
    } else {
        here + deep(frames - 1)
    }
}

/// Runs `call` on a thread with [`HOST_STACK`], as a small host thread would.
fn from_a_host_thread<T: Send + 'static>(call: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(HOST_STACK)
        .spawn(call)
        .expect("a host thread starts")
        .join()
        .expect("the host thread returns")
}

#[test]
fn worker_threads_match_the_formula() {
    let runtime = runtime();
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    assert_eq!(runtime.workers(), cores.saturating_sub(1).max(1));
}

#[test]
fn a_call_is_polled_on_the_driver_thread_not_the_callers() {
    let runtime = runtime();
    let name = from_a_host_thread(move || {
        runtime.block_on(|| async { std::thread::current().name().map(str::to_owned) })
    });
    assert_eq!(name.as_deref(), Some(DRIVER_NAME));
}

#[test]
fn work_deeper_than_the_callers_stack_completes() {
    // About 2 MiB, four times the host thread's stack. Polled on the caller,
    // this ends the test process with a stack overflow rather than failing an
    // assertion.
    let runtime = runtime();
    let used = from_a_host_thread(move || runtime.block_on(|| async { deep(32) }));
    assert_eq!(used, 33 * 64 * 1024);
}

#[test]
// The large stack value is the point; boxing it would shrink the future the
// test needs to be large.
#[allow(clippy::large_stack_arrays)]
fn a_future_larger_than_the_callers_stack_is_built_where_it_runs() {
    // A deep call path in a debug build is a future of hundreds of KiB. Built
    // on the caller and handed over by value, copying it towards the driver
    // overflows the caller first.
    let runtime = runtime();
    let first = from_a_host_thread(move || {
        runtime.block_on(|| async {
            let held = [7u8; 1024 * 1024];
            tokio::task::yield_now().await;
            black_box(&held)[HOST_STACK]
        })
    });
    assert_eq!(first, 7);
}

#[test]
fn a_panic_in_the_call_reaches_the_caller() {
    let runtime = runtime();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.block_on(|| async { panic!("the provider panicked") });
    }));
    let payload = outcome.expect_err("the panic resumes on the caller");
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&"the provider panicked")
    );
}

#[test]
fn a_call_from_a_runtime_worker_still_completes() {
    // A scheduled pass reaches the host from a worker: waiting there must not
    // take the worker the call itself needs.
    let runtime = runtime();
    let handle = runtime.handle().clone();
    let answer = drive(&runtime.handle(), move || async move {
        tokio::spawn(async move { drive(&handle, || async { 42 }) })
            .await
            .expect("the worker task completes")
    });
    assert_eq!(answer, 42);
}
