//! Native process-termination observation for long-lived background workers.

use std::fmt;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};

/// The process-wide termination flag owned by the first successful
/// [`ProcessTermination::install`]. Later installs share this authority
/// instead of replacing the native handler or failing.
static INSTALLED: OnceLock<Result<Arc<AtomicBool>, ProcessTerminationInstallError>> =
    OnceLock::new();

#[derive(Clone, Debug)]
pub struct ProcessTermination {
    requested: Arc<AtomicBool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessTerminationInstallError {
    detail: Arc<str>,
}

impl ProcessTerminationInstallError {
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ProcessTerminationInstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("native process termination handler could not start")
    }
}

impl std::error::Error for ProcessTerminationInstallError {}

impl ProcessTermination {
    /// Install the process-wide native termination handler once. Unix maps
    /// SIGINT/SIGTERM/SIGHUP and Windows maps console termination events into
    /// the same lock-free observation; the callback performs no I/O.
    ///
    /// Installation is idempotent per process: the first successful call owns
    /// the native handler and its flag, and every later call returns another
    /// handle observing that same flag. The native handler registry admits
    /// exactly one closure, so a second independent instance could never
    /// receive signals — sharing the first flag is the only honest
    /// observation. A failed first attempt is cached and reported unchanged
    /// to later calls, matching the other process-wide install seams.
    pub fn install() -> Result<Self, ProcessTerminationInstallError> {
        INSTALLED
            .get_or_init(|| {
                let requested = Arc::new(AtomicBool::new(false));
                let handler = Arc::clone(&requested);
                // This executable is the sole termination-handler owner.
                // Background shells may deliberately start children with
                // SIGINT ignored; using the replacing form normalizes that
                // inherited process disposition before observing
                // SIGINT/SIGTERM/SIGHUP through one typed flag.
                ctrlc::set_handler(move || handler.store(true, Ordering::Release))
                    .map_err(|error| ProcessTerminationInstallError {
                        detail: Arc::from(error.to_string()),
                    })
                    .map(|()| requested)
            })
            .clone()
            .map(|requested| Self { requested })
    }

    #[must_use]
    pub fn is_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }
}

#[cfg(test)]
#[path = "../tests/headless/process_termination_tests.rs"]
mod tests;
