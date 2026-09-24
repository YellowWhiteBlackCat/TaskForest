//! Programmable provider fixture shared by the port contract scenarios.
//!
//! The `FakeProvider` struct and its trait implementations are split by
//! port/domain: the struct and shared helpers live here, each provider group
//! implements its traits in a sibling module under [`self`], and the registry
//! composition lives in [`registry`].

use super::*;
use taskmanager_platform_runtime::ProviderRegistration;

#[path = "fixture/environment.rs"]
mod environment;
#[path = "fixture/integration.rs"]
mod integration;
#[path = "fixture/observation.rs"]
mod observation;
#[path = "fixture/process.rs"]
mod process;
#[path = "fixture/registry.rs"]
pub(super) mod registry;
#[path = "fixture/sensors_power.rs"]
mod sensors_power;
#[path = "fixture/services.rs"]
mod services;
#[path = "fixture/storage.rs"]
mod storage;

pub(super) use registry::fake_registry;
use taskmanager_core::ProcessSignal;

#[derive(Clone, Default)]
pub(super) struct FakeProvider {
    pub(super) delay: Duration,
    pub(super) process_refresh_started: Arc<AtomicBool>,
    pub(super) service_error: Option<ProviderFailure>,
    pub(super) service_operation_error: Option<ProviderFailure>,
    pub(super) ended: Arc<Mutex<Vec<FrozenProcessIdentity>>>,
    pub(super) signaled: Arc<Mutex<Vec<(u32, ProcessSignal)>>>,
    pub(super) revealed: Arc<Mutex<Vec<FrozenProcessIdentity>>>,
    pub(super) startup_controls: Arc<Mutex<Vec<(String, bool)>>>,
    pub(super) startup_evidence_times: Arc<Mutex<Vec<u64>>>,
    pub(super) session_controls: Arc<Mutex<Vec<(String, SessionControlAction)>>>,
    pub(super) smart_starts: Arc<Mutex<Vec<SmartSelfTestIntent>>>,
    pub(super) smart_control_delay: Duration,
    pub(super) smart_control_report: Option<SmartSelfTestReport>,
    pub(super) smart_refresh_delay: Duration,
    pub(super) smart_refresh_started: Arc<AtomicBool>,
    pub(super) smart_refresh_targets: Arc<Mutex<Vec<StorageDeviceTarget>>>,
    pub(super) smart_refresh_errors: Arc<Mutex<Vec<(String, ProviderFailure)>>>,
    pub(super) smart_refresh_reports: Arc<Mutex<Vec<(String, SmartSelfTestReport)>>>,
    pub(super) process_telemetry_delay: Duration,
    pub(super) process_gpu_delay: Duration,
    pub(super) process_telemetry_targets: Arc<Mutex<Vec<FrozenProcessIdentity>>>,
    pub(super) process_control_delay: Duration,
    pub(super) process_control_started: Arc<AtomicBool>,
    pub(super) sensor_enrichment_error: Option<FailureKind>,
    pub(super) observation_source_failure: Option<FailureKind>,
    pub(super) storage_observation_delay: Duration,
    pub(super) gpu_observation_delay: Duration,
    pub(super) process_telemetry_failure: Option<FailureKind>,
}

fn fixture_source(
    provider: &'static str,
    item_count: usize,
    failure: Option<FailureKind>,
) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed(provider),
        outcome: failure.map_or_else(
            || {
                if item_count == 0 {
                    SourceOutcome::Empty
                } else {
                    SourceOutcome::Available
                }
            },
            SourceOutcome::Partial,
        ),
        item_count,
    }
}

pub(super) fn frozen_process(pid: u32) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(pid, "worker", 7_500, 9_000 + u64::from(pid))
        .expect("fixture identity")
}

/// Poll `flag` (published by a fixture provider worker thread) until it is
/// set, bounded by wall-clock time rather than by scheduler turns. A loaded
/// parallel suite only stretches the wait; a real regression still fails
/// naming the condition and the elapsed time.
pub(super) fn wait_for_flag(flag: &AtomicBool, condition: &str) {
    const WAIT_LIMIT: Duration = Duration::from_secs(10);
    const POLL_INTERVAL: Duration = Duration::from_millis(1);
    let started = Instant::now();
    while !flag.load(Ordering::Acquire) {
        let waited = started.elapsed();
        assert!(
            waited < WAIT_LIMIT,
            "timed out after {waited:?} waiting for {condition}",
        );
        thread::sleep(POLL_INTERVAL);
    }
}

pub(super) fn wait_event(handle: &PlatformHandle) -> EventEnvelope<PlatformEvent> {
    const WAIT_LIMIT: Duration = Duration::from_secs(10);
    const POLL_INTERVAL: Duration = Duration::from_millis(1);
    let started = Instant::now();
    loop {
        if let Some(event) = handle.events().try_recv().expect("connected event port") {
            return event;
        }
        let waited = started.elapsed();
        assert!(
            waited < WAIT_LIMIT,
            "timed out after {waited:?} waiting for a platform event",
        );
        thread::sleep(POLL_INTERVAL);
    }
}

pub(super) fn submit_process_list(
    handle: &PlatformHandle,
    ids: &mut RequestIdGenerator,
    capability: CapabilityId,
    payload: ProcessListRequest,
) -> RequestId {
    let id = ids.next_id();
    handle
        .process_list()
        .expect("process list facet")
        .try_submit(RequestEnvelope {
            id,
            capability,
            submitted_at_ms: 1,
            payload,
        })
        .expect("bounded process-list request accepted");
    id
}

pub(super) fn submit_process_control(
    handle: &PlatformHandle,
    ids: &mut RequestIdGenerator,
    capability: CapabilityId,
    payload: ProcessControlRequest,
) -> RequestId {
    let id = ids.next_id();
    handle
        .process_control()
        .expect("process control facet")
        .try_submit(RequestEnvelope {
            id,
            capability,
            submitted_at_ms: 1,
            payload,
        })
        .expect("bounded process-control request accepted");
    id
}

pub(super) fn submit_process_affinity_control(
    handle: &PlatformHandle,
    ids: &mut RequestIdGenerator,
    capability: CapabilityId,
    payload: ProcessAffinityControlRequest,
) -> RequestId {
    let id = ids.next_id();
    handle
        .process_affinity_control()
        .expect("process affinity control facet")
        .try_submit(RequestEnvelope {
            id,
            capability,
            submitted_at_ms: 1,
            payload,
        })
        .expect("bounded process-affinity-control request accepted");
    id
}
