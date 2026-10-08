// SPDX-License-Identifier: GPL-3.0-only
//! The app's async runtime, and the thread every blocking host call is polled on.
//!
//! A blocking call from the shell never polls its future on the thread that made it. That thread is the host's, and so is its stack size: the main thread, or a thread the windowing framework sized, can be far under what a debug build's deep call path needs, and overflowing it ends the process. The future is polled on a thread of our own instead, sized by [`DRIVER_STACK`], and the caller only waits. Whatever thread the host calls from, the stack the app runs on is the same.
//!
//! The future is also **built** there: a caller passes a closure that makes it, never the future itself. A debug build's deep call path is a future of hundreds of KiB, and passing one by value copies it through each frame on the way to the driver, which on its own can overflow a small host thread. The closure holds only what the future borrows or takes.
//!
//! `clippy.toml` forbids tokio's own `block_on` everywhere else in this crate, so a call site cannot go back to polling on the caller's stack: [`AppRuntime::block_on`] and [`drive`] are the only way to wait on a future here.
//!
//! The runtime sits behind a mutex so the exit hook can take it out from a shared handle and shut it down with a bound; while it lives, the mutex is only ever held to clone the handle out, never across a poll.

use std::{future::Future, sync::Mutex, time::Duration};

use tokio::runtime::{Builder, Handle, Runtime, RuntimeFlavor};

/// The stack of the thread a blocking host call is polled on. A deep call path
/// (dial, provider connect, TLS) is the deepest, and a debug build's futures
/// are several times the size of a release build's. The memory is reserved,
/// not committed: a thread touches only the pages it uses.
const DRIVER_STACK: usize = 16 * 1024 * 1024;

/// The stack of each runtime worker, which runs the same paths for boot and
/// reconnect. tokio's default is 2 MiB.
const WORKER_STACK: usize = 8 * 1024 * 1024;

/// The name of the thread [`drive`] polls on, as a crash report or a debugger
/// shows it.
const DRIVER_NAME: &str = "astra-desktop-call";

/// The app's runtime, with the only `block_on` the host may call.
#[derive(Debug)]
pub struct AppRuntime {
    /// The runtime every engine call runs on, taken by
    /// [`shutdown_timeout`](AppRuntime::shutdown_timeout): the one mutation the
    /// type has, so every other method finds it present.
    runtime: Mutex<Option<Runtime>>,
    /// The worker count the runtime was built with, so the caller can state
    /// (and a test can check) how many cores the app took.
    workers: usize,
}

impl AppRuntime {
    /// Builds the future `make` returns on a thread of our own, runs it to
    /// completion there, and returns its output; see the module docs.
    ///
    /// # Panics
    ///
    /// Panics if the runtime has been shut down; nothing calls it after the
    /// exit hook, which is the only taker.
    pub fn block_on<M, F>(&self, make: M) -> F::Output
    where
        M: FnOnce() -> F + Send,
        F: Future,
        F::Output: Send,
    {
        drive(&self.handle(), make)
    }

    /// The runtime's handle, for spawning onto it without blocking the caller.
    pub fn handle(&self) -> Handle {
        self.runtime
            .lock()
            .expect("runtime mutex poisoned")
            .as_ref()
            .expect("the runtime is alive until the exit hook shuts it down")
            .handle()
            .clone()
    }

    /// Spawns a task onto the runtime and returns its join handle.
    pub fn spawn<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.handle().spawn(future)
    }

    /// The worker count the runtime was built with: one fewer than the core
    /// count, at least one.
    pub fn workers(&self) -> usize {
        self.workers
    }

    /// Shuts the runtime down, giving in-flight sync at most `timeout` before it
    /// stops waiting: the exit hook's bound, so closing the window never hangs on
    /// a stalled pass. Idempotent: a second call finds nothing left to shut down.
    pub fn shutdown_timeout(&self, timeout: Duration) {
        if let Some(runtime) = self.runtime.lock().expect("runtime mutex poisoned").take() {
            runtime.shutdown_timeout(timeout);
        }
    }
}

/// Builds the future `make` returns and polls it on `handle`'s runtime, both
/// on a new [`DRIVER_STACK`]-sized thread, and waits for it. A panic in
/// either resumes on the caller, where the host turns it into an error.
///
/// Called from a runtime worker (a scheduled pass), the wait moves the worker
/// out of the scheduler first ([`tokio::task::block_in_place`]), so the work
/// it waits for still has somewhere to run.
#[allow(clippy::disallowed_methods)] // the one place tokio's `block_on` is called
pub fn drive<M, F>(handle: &Handle, make: M) -> F::Output
where
    M: FnOnce() -> F + Send,
    F: Future,
    F::Output: Send,
{
    std::thread::scope(|scope| {
        let driver = std::thread::Builder::new()
            .name(DRIVER_NAME.to_owned())
            .stack_size(DRIVER_STACK)
            .spawn_scoped(scope, || handle.block_on(make()))
            .expect("the system starts a thread for a host call");
        let on_worker = Handle::try_current()
            .is_ok_and(|current| current.runtime_flavor() == RuntimeFlavor::MultiThread);
        let joined = if on_worker {
            tokio::task::block_in_place(|| driver.join())
        } else {
            driver.join()
        };
        joined.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

/// Builds the app's async runtime, capping worker threads to **one fewer than
/// the core count** so a heavy multi-folder sync never saturates every core
/// and starves the host's UI thread. At least one worker.
///
/// # Errors
///
/// Fails when the runtime's threads cannot start, which leaves nothing to run
/// the app on; a caller that cannot go on panics on the error.
pub fn build_runtime() -> std::io::Result<AppRuntime> {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let workers = cores.saturating_sub(1).max(1);
    let runtime = Builder::new_multi_thread()
        .worker_threads(workers)
        .thread_stack_size(WORKER_STACK)
        .enable_all()
        .build()?;
    Ok(AppRuntime {
        runtime: Mutex::new(Some(runtime)),
        workers,
    })
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
