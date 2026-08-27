//! Process-wide platform runtime handoff (charter boundary 5).
//!
//! The bevy `World` owns no platform worker and spawns no provider: exactly
//! one [`PlatformClient`] exists per process, acquired through the same
//! `OnceLock<Result<…>>` cache shape `NativeAppHost` uses for its lazy
//! runtimes — a successful start is handed to every later caller, and a
//! failed start is reported unchanged instead of being retried behind the
//! UI. A window rebuild therefore observes the cached handle and never
//! re-spawns the runtime.
//!
//! The client sits behind a `Mutex` because the bevy drain system borrows it
//! once per frame while the handle itself is shared through the `'static`
//! cache; the critical section covers only non-blocking `try_recv` draining
//! and request submission, so no blocking collection ever runs on the UI
//! thread (boundary 4).

use std::fmt;
use std::sync::{Mutex, MutexGuard, OnceLock};

use taskmanager_app_host::NativeAppHost;
use taskmanager_application::{
    CapabilityCatalog, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
    HostTelemetryRequest, PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle,
    RequestEnvelope, RequestPort, SubmissionError,
};

/// Typed failure for the one process-wide runtime start attempt.
///
/// `NativeAppHost::spawn_client` answers with the composition error of the
/// native adapter, which app-host does not re-export through its public edge;
/// the bevy-ui whitelist (charter boundary 1) forbids depending on
/// `taskmanager-platform-runtime` just to name it, so the failure crosses as
/// its `Display` text wrapped in this typed enum. The cache semantics — not
/// the error shape — are what this module owns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeStartFailure {
    message: String,
}

impl RuntimeStartFailure {
    fn composition<E: fmt::Display>(error: E) -> Self {
        Self {
            message: error.to_string(),
        }
    }

    /// The composition failure text observed by the first start attempt.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RuntimeStartFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native platform composition failed: {}",
            self.message
        )
    }
}

/// The started process-wide runtime: the shared client behind a short-lock
/// mutex. Cloning is impossible by design — every consumer borrows the one
/// `'static` handle.
pub struct SharedRuntime {
    client: Mutex<PlatformClient>,
}

impl SharedRuntime {
    fn new(client: PlatformClient) -> Self {
        Self {
            client: Mutex::new(client),
        }
    }

    /// Lock the shared client for one frame's drain.
    ///
    /// Poison is recovered via `into_inner` (the workspace lock-poisoning
    /// contract): a panicking holder leaves the client structurally valid,
    /// and the typed event port keeps its own failure reporting.
    pub fn lock_client(&self) -> MutexGuard<'_, PlatformClient> {
        self.client
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// One lazily-started runtime slot with app-host cache semantics:
/// the first `get_or_init` call performs the only spawn attempt, and every
/// later call observes that attempt's result — success or failure — unchanged.
///
/// Tests build their own cache with an injected spawn closure instead of
/// touching the process-wide slot, keeping the singleton semantics observable
/// without any native composition.
pub struct RuntimeCache {
    cell: OnceLock<Result<SharedRuntime, RuntimeStartFailure>>,
}

impl RuntimeCache {
    /// An empty cache; `const` so the process-wide slot is a plain `static`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cell: OnceLock::new(),
        }
    }

    /// Return the cached runtime, starting it with `spawn` on the first call.
    ///
    /// A failed first attempt is cached exactly like a successful one: later
    /// callers see the same typed failure and `spawn` is never re-entered,
    /// mirroring `NativeAppHost`'s lazy-runtime contract.
    pub fn get_or_init(
        &self,
        spawn: impl FnOnce() -> Result<PlatformClient, RuntimeStartFailure>,
    ) -> Result<&SharedRuntime, &RuntimeStartFailure> {
        match self.cell.get_or_init(|| spawn().map(SharedRuntime::new)) {
            Ok(runtime) => Ok(runtime),
            Err(failure) => Err(failure),
        }
    }
}

impl Default for RuntimeCache {
    fn default() -> Self {
        Self::new()
    }
}

static PROCESS_RUNTIME: RuntimeCache = RuntimeCache::new();

struct DemoCapabilities;

impl CapabilityCatalog for DemoCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot::default()
    }
}

struct DemoEvents;

impl EventPort for DemoEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}

struct DemoRequests;

impl RequestPort for DemoRequests {
    type Request = HostTelemetryRequest;

    fn try_submit(&self, _request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        Ok(())
    }
}

static DEMO_RUNTIME: RuntimeCache = RuntimeCache::new();

/// A no-I/O runtime handle for the capture/demo composition. It exists only
/// to satisfy the same typed plugin shape as production; the demo plugin does
/// not install the platform drain system.
pub fn demo_platform_runtime() -> &'static SharedRuntime {
    DEMO_RUNTIME
        .get_or_init(|| {
            Ok(PlatformClient::new(PlatformHandle::new(
                std::sync::Arc::new(DemoCapabilities),
                std::sync::Arc::new(DemoEvents),
                PlatformFacets::default().with_system(
                    taskmanager_application::SystemFacets::default()
                        .with_host(std::sync::Arc::new(DemoRequests)),
                ),
            )))
        })
        .expect("the Bevy demo runtime is an in-memory no-op")
}

/// Resolve the process-wide platform runtime, spawning it on first use.
///
/// Production-only seam (tests inject their own [`RuntimeCache`]); building
/// the host reads the native composition paths exactly once per process.
pub fn shared_platform_runtime() -> Result<&'static SharedRuntime, &'static RuntimeStartFailure> {
    PROCESS_RUNTIME.get_or_init(|| {
        NativeAppHost::production()
            .spawn_client()
            .map_err(RuntimeStartFailure::composition)
    })
}

#[cfg(test)]
#[path = "../tests/headless/runtime.rs"]
mod tests;
