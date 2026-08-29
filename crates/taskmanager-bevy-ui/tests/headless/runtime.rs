//! test-intent: behavior
//!
//! Headless behavior tests for the process-wide runtime cache.
//!
//! The [`super::RuntimeCache`] must reproduce the app-host lazy-runtime
//! contract: the first caller's start attempt (success OR failure) is the one
//! every later caller observes, and the spawn closure is never re-entered. The
//! tests inject scripted spawn closures and count their invocations — no
//! native composition, no OS paths.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};

use taskmanager_application::{PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle};
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
};

use super::{RuntimeCache, RuntimeStartFailure};

struct EmptyCapabilities;

impl CapabilityCatalog for EmptyCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot::default()
    }
}

struct EmptyEvents;

impl EventPort for EmptyEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}

fn fake_client() -> PlatformClient {
    PlatformClient::new(PlatformHandle::new(
        std::sync::Arc::new(EmptyCapabilities),
        std::sync::Arc::new(EmptyEvents),
        PlatformFacets::default(),
    ))
}

#[test]
fn successful_start_is_cached_and_shared_verbatim() {
    let cache = RuntimeCache::new();
    let spawns = AtomicUsize::new(0);
    let first = cache
        .get_or_init(|| {
            spawns.fetch_add(1, Ordering::SeqCst);
            Ok(fake_client())
        })
        .expect("first start succeeds");
    let second = cache
        .get_or_init(|| {
            spawns.fetch_add(1, Ordering::SeqCst);
            Ok(fake_client())
        })
        .expect("cached start succeeds");
    assert_eq!(
        spawns.load(Ordering::SeqCst),
        1,
        "a window rebuild must reuse the cached runtime, never re-spawn"
    );
    assert!(
        std::ptr::eq(first, second),
        "both callers must hold the one shared handle"
    );
}

#[test]
fn failed_start_is_cached_and_never_retried() {
    let cache = RuntimeCache::new();
    let spawns = AtomicUsize::new(0);
    let failure = cache
        .get_or_init(|| {
            spawns.fetch_add(1, Ordering::SeqCst);
            Err(RuntimeStartFailure::composition("adapter missing"))
        })
        .err()
        .expect("a failed start must surface as the typed failure");
    assert_eq!(
        failure.message(),
        "adapter missing",
        "the failure text must cross unchanged"
    );
    let second = cache.get_or_init(|| {
        spawns.fetch_add(1, Ordering::SeqCst);
        Ok(fake_client())
    });
    assert!(
        second.is_err(),
        "a cached failure must NOT be replaced by a later successful spawn"
    );
    assert_eq!(
        spawns.load(Ordering::SeqCst),
        1,
        "the spawn closure must never be re-entered after a failed first attempt"
    );
}

#[test]
fn poisoned_client_lock_recovers_instead_of_panicking() {
    let cache = RuntimeCache::new();
    let shared = cache
        .get_or_init(|| Ok(fake_client()))
        .expect("start succeeds");
    // Poison the inner mutex deliberately: a holder that panicked must not
    // turn every later frame's `lock_client` into a UI-thread panic. The
    // client stays structurally valid; the typed event port keeps reporting
    // its own failures.
    let poisoned = catch_unwind(AssertUnwindSafe(|| {
        let _guard = shared.client.lock();
        panic!("poison the drain lock");
    }))
    .is_err();
    assert!(poisoned, "the fixture must actually poison the lock");
    let recovered = shared.lock_client();
    assert_eq!(
        recovered.capabilities().snapshot().iter().count(),
        0,
        "the recovered guard must serve real client reads"
    );
}
