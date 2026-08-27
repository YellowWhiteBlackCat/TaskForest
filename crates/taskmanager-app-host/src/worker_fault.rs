//! Crate-private panic isolation for app-host background workers.
//!
//! Every bounded worker in this crate routes its request body (or its whole
//! loop, where backend state may be damaged) through [`catch_worker_panic`],
//! mirroring the platform runtime's `execute_isolated` seam without taking a
//! dependency on that crate. A fault degrades into one bounded owned detail
//! that the caller folds into its existing health/completion structures; the
//! seam never fabricates success and never lets a panic kill a thread before
//! the worker's exit bookkeeping has run.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

/// Panic details share the bounded budget of the runtime failure partitions,
/// so a hostile payload cannot grow an unbounded diagnostics entry.
const MAX_WORKER_FAULT_CHARS: usize = 512;

/// Run one worker body with panic isolation.
///
/// The closure is `AssertUnwindSafe` by contract: callers move their owned
/// worker state in and must treat it as potentially damaged when this returns
/// `Err`, which is why every call site registers an exit instead of
/// continuing on the same state.
pub(crate) fn catch_worker_panic<T, F>(body: F) -> Result<T, Arc<str>>
where
    F: FnOnce() -> T,
{
    catch_unwind(AssertUnwindSafe(body)).map_err(bounded_worker_fault_detail)
}

fn bounded_worker_fault_detail(payload: Box<dyn Any + Send>) -> Arc<str> {
    let message = payload
        .downcast_ref::<&'static str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "worker panicked with a non-string payload".to_owned());
    let detail = format!("isolated worker panic: {message}");
    Arc::from(
        detail
            .chars()
            .take(MAX_WORKER_FAULT_CHARS)
            .collect::<String>(),
    )
}
