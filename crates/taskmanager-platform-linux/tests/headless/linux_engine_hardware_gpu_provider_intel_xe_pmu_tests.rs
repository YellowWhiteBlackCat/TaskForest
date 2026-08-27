use std::time::{Duration, Instant};

use super::*;

const DEVICE: &str = "gpu:pci:0000:00:02.0";

/// A non-empty upstream read short-circuits the xe cascade: the xe PMU is
/// NEVER probed (no `perf_event_open`), so this test runs on a host with no
/// xe GPU. The sysfs/i915 winner is returned verbatim, including its
/// failure payload.
#[test]
fn non_empty_upstream_short_circuits_xe_probe() {
    let mut fallback = XePmuFallback::default();
    let upstream = GpuFieldRead::available(vec![IntelEngineRead {
        name: "Render/3D".to_string(),
        busy: EngineBusySource::NanoSeconds(123),
    }]);

    // A nonexistent device path is fine: the xe probe must not run.
    let out = fallback.fallback_if_empty(
        DEVICE,
        Path::new("/nonexistent/xe"),
        upstream,
        Instant::now(),
    );
    let engines = out.value.expect("non-empty upstream returned verbatim");
    assert_eq!(engines.len(), 1);
    assert_eq!(engines[0].name, "Render/3D");
    // No device entry recorded → no probe happened.
    assert!(fallback.devices.is_empty());
}

/// An empty-but-available upstream (zero engines) DOES delegate to the xe
/// probe; on a host with no matching xe PMU the probe records an absent
/// entry and the sample yields None, so the original empty read is returned.
/// This exercises the delegate branch without asserting any open success.
#[test]
fn empty_upstream_delegates_to_xe_probe_and_records_state() {
    let mut fallback = XePmuFallback::default();
    let empty = GpuFieldRead::available(Vec::new());

    let out =
        fallback.fallback_if_empty(DEVICE, Path::new("/nonexistent/xe"), empty, Instant::now());
    assert!(
        out.value.as_ref().is_some_and(|engines| engines.is_empty()),
        "no xe PMU → original empty read preserved"
    );
    // The probe ran once and recorded an Absent state for this device.
    assert_eq!(fallback.devices.len(), 1);
    assert!(matches!(
        fallback.devices.get(DEVICE),
        Some(XePmuDeviceState::Absent { failure: None, .. })
    ));
}

/// A failed xe probe is not retried on every collection tick, but it is
/// retried exactly when its bounded retry time arrives. The nonexistent
/// path keeps the test independent of host hardware while proving the
/// recovery state machine.
#[test]
fn absent_probe_is_rate_limited_then_retried() {
    let mut fallback = XePmuFallback::default();
    let started_at = Instant::now();

    fallback.fallback_if_empty(
        DEVICE,
        Path::new("/nonexistent/xe"),
        GpuFieldRead::available(Vec::new()),
        started_at,
    );
    let first_retry_at = match fallback.devices.get(DEVICE) {
        Some(XePmuDeviceState::Absent { retry_at, .. }) => *retry_at,
        Some(XePmuDeviceState::Active { .. }) => {
            panic!("nonexistent xe path must not produce active counters")
        }
        None => panic!("first probe must record a device state"),
    };

    fallback.fallback_if_empty(
        DEVICE,
        Path::new("/nonexistent/xe"),
        GpuFieldRead::available(Vec::new()),
        started_at + Duration::from_secs(1),
    );
    let still_first_retry_at = match fallback.devices.get(DEVICE) {
        Some(XePmuDeviceState::Absent { retry_at, .. }) => *retry_at,
        Some(XePmuDeviceState::Active { .. }) => {
            panic!("nonexistent xe path must remain absent")
        }
        None => panic!("rate-limited state must remain cached"),
    };
    assert_eq!(
        still_first_retry_at, first_retry_at,
        "an absent PMU must not hot-loop probes before its retry deadline"
    );

    fallback.fallback_if_empty(
        DEVICE,
        Path::new("/nonexistent/xe"),
        GpuFieldRead::available(Vec::new()),
        first_retry_at,
    );
    let second_retry_at = match fallback.devices.get(DEVICE) {
        Some(XePmuDeviceState::Absent { retry_at, .. }) => *retry_at,
        Some(XePmuDeviceState::Active { .. }) => {
            panic!("nonexistent xe path must remain absent after retry")
        }
        None => panic!("retry must leave an absent state recorded"),
    };
    assert!(
        second_retry_at > first_retry_at,
        "the retry deadline must advance after a failed re-probe"
    );
}
