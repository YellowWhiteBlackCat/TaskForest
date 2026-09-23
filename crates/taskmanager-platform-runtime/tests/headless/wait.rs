//! Deterministic cross-thread waits shared by the runtime test modules.
//!
//! A test must never decide pass/fail from how many times the scheduler
//! happened to run another thread. `wait_for!` blocks the caller on a
//! wall-clock deadline instead: a real regression still fails with the named
//! condition, while a loaded parallel suite only stretches the wait.

/// Poll `$poll` until it yields a value or the deadline elapses.
///
/// The loop is bounded by wall time, never by scheduler turns. Panics naming
/// `$condition` and the elapsed time when the deadline passes.
#[macro_export]
macro_rules! wait_for {
    ($condition:expr, $poll:expr $(,)?) => {{
        const WAIT_LIMIT: ::std::time::Duration = ::std::time::Duration::from_secs(10);
        const POLL_INTERVAL: ::std::time::Duration = ::std::time::Duration::from_millis(1);
        let started = ::std::time::Instant::now();
        loop {
            if let Some(value) = ($poll)() {
                break value;
            }
            let waited = started.elapsed();
            assert!(
                waited < WAIT_LIMIT,
                "timed out after {waited:?} waiting for {}",
                $condition,
            );
            ::std::thread::sleep(POLL_INTERVAL);
        }
    }};
}
