//! Layer-B capability-surface contract for the Linux adapter (P5 M3.2/M3.4).
//!
//! The declared surface lives at the real route/provider registration site
//! (`src/provider.rs`). This scenario proves the declaration and the live
//! runtime catalog publish the same fact: every declared `Present` lane is a
//! registered `linux.*` lane, every registered lane is declared, no
//! product-expected identity answers `Undeclared`, and Linux registers no
//! lane whose provider can only answer a typed absence.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use taskmanager_application::{CpuThrottleEvent, CpuThrottleRequest, PlatformEvent};
use taskmanager_platform_conformance::assert_capability_surface_matches_catalog;
use taskmanager_platform_contract::{
    CapabilityId, CapabilityStatus, PlatformAxis, PlatformSource, RequestEnvelope, RequestId,
};
use taskmanager_platform_linux::{LinuxPlatformRuntime, capability_surface};

/// The Linux adapter registers 49 real lanes; the number is pinned so a
/// registration change must move the layer-B declaration in the same change.
const DECLARED_PRESENT_LANES: usize = 49;

#[test]
fn the_linux_capability_surface_is_the_live_catalog_registration_face() {
    let handle = LinuxPlatformRuntime::spawn().expect("complete Linux composition");
    let snapshot = handle.capabilities().snapshot();
    let surface = capability_surface();

    assert_eq!(
        assert_capability_surface_matches_catalog(
            &snapshot,
            &surface,
            PlatformAxis::Linux,
            "linux.",
        ),
        Ok(())
    );

    assert_eq!(
        surface.present_count(PlatformAxis::Linux),
        DECLARED_PRESENT_LANES
    );
    assert_eq!(snapshot.registered().count(), DECLARED_PRESENT_LANES);

    // Linux implements every lane it registers: no registered lane is declared
    // with an absence, and no declared `Present` lane is missing a provider.
    for descriptor in snapshot.registered() {
        assert_eq!(
            surface.source(PlatformAxis::Linux, &descriptor.id),
            PlatformSource::Present,
            "{} is registered but not declared Present",
            descriptor.id
        );
        assert!(
            descriptor
                .providers
                .iter()
                .all(|provider| provider.as_str().starts_with("linux.")),
            "{} must be attributed to a linux.* provider",
            descriptor.id
        );
    }

    // M3.2 `Undeclared` census: every product-expected identity is declared.
    let undeclared: Vec<String> = CapabilityId::EXPECTED_SURFACE
        .iter()
        .filter(|expected| {
            surface.source(PlatformAxis::Linux, expected) == PlatformSource::Undeclared
        })
        .map(|expected| expected.as_str().to_owned())
        .collect();
    assert!(undeclared.is_empty(), "{undeclared:?}");
}

/// The declared absence is typed: a lane without a Linux source answers the
/// static `RequiresEscalation`/`Unsupported` vocabulary, never a runtime
/// transient, and the runtime catalog agrees it is not registered.
#[test]
fn the_linux_absence_declarations_are_typed_and_catalog_consistent() {
    let handle = LinuxPlatformRuntime::spawn().expect("complete Linux composition");
    let snapshot = handle.capabilities().snapshot();
    let surface = capability_surface();

    let mut absent_with_source: BTreeSet<String> = BTreeSet::new();
    for expected in CapabilityId::EXPECTED_SURFACE {
        match surface.source(PlatformAxis::Linux, &expected) {
            PlatformSource::Present => {}
            PlatformSource::Absent(status) => {
                assert!(
                    PlatformSource::is_absence_projection(status),
                    "{expected} carries the runtime state {status:?} as a static absence"
                );
                assert_eq!(status, CapabilityStatus::Unsupported);
                if snapshot
                    .get(&expected)
                    .is_some_and(|descriptor| !descriptor.is_typed_absence())
                {
                    absent_with_source.insert(expected.as_str().to_owned());
                }
            }
            PlatformSource::Undeclared => panic!("{expected} must be declared"),
        }
    }
    assert!(
        absent_with_source.is_empty(),
        "Linux registers no pending lane, so no absent declaration may have a source: {absent_with_source:?}"
    );
}

/// The registered `telemetry.cpu.throttle` lane is a real read, not a
/// declaration: one live request answers with the per-package counters the
/// periodic CPU projection carries, or with a typed failure when the host
/// exposes no CPU counter tree at all — never a fabricated zero.
#[cfg(target_os = "linux")]
#[test]
fn the_linux_cpu_throttle_lane_answers_the_shared_counter_read() {
    let handle = LinuxPlatformRuntime::spawn().expect("complete Linux composition");
    let port = handle
        .facets()
        .system()
        .cpu_throttle()
        .expect("the registered lane must expose its request port");
    port.try_submit(RequestEnvelope {
        id: RequestId::new(7).expect("request id"),
        capability: CapabilityId::TELEMETRY_CPU_THROTTLE,
        submitted_at_ms: 1,
        payload: CpuThrottleRequest::Refresh,
    })
    .expect("the throttle lane accepts a refresh");

    let started = Instant::now();
    loop {
        if let Some(event) = handle.events().try_recv().expect("event port") {
            match event.outcome {
                Ok(PlatformEvent::CpuThrottle(CpuThrottleEvent::Update(snapshot))) => {
                    assert!(
                        snapshot.is_success(),
                        "a readable /sys CPU tree must answer with counter rows: {snapshot:?}"
                    );
                    assert!(
                        snapshot
                            .packages
                            .windows(2)
                            .all(|pair| pair[0].package_id < pair[1].package_id),
                        "package rows must stay sorted and unique: {snapshot:?}"
                    );
                    if std::fs::read_to_string(
                        "/sys/devices/system/cpu/cpu0/thermal_throttle/package_throttle_count",
                    )
                    .is_ok()
                    {
                        assert!(
                            snapshot
                                .packages
                                .iter()
                                .any(|row| row.package_throttle_count.is_some()),
                            "an exposed kernel counter must reach the lane answer: {snapshot:?}"
                        );
                    }
                    return;
                }
                Ok(other) => panic!("expected a CpuThrottle event, got {other:?}"),
                Err(failure) => panic!("the throttle lane reported a failure: {failure:?}"),
            }
        }
        let waited = started.elapsed();
        assert!(
            waited < Duration::from_secs(10),
            "timed out after {waited:?} waiting for a CpuThrottle update event from the live lane",
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
