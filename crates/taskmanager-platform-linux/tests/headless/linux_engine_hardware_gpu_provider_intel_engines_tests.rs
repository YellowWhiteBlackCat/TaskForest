use std::time::Duration;

use super::*;

const DEVICE: &str = "gpu:pci:0000:00:02.0";

fn read_of(engines: &[(&str, u64)]) -> GpuFieldRead<Vec<IntelEngineRead>> {
    GpuFieldRead::available(
        engines
            .iter()
            .map(|(name, busy)| IntelEngineRead {
                name: (*name).to_string(),
                busy: EngineBusySource::NanoSeconds(*busy),
            })
            .collect(),
    )
}

/// xe ticks sample builder: `&[("Render/3D", active, total), …]`.
fn ticks_of(engines: &[(&str, u64, u64)]) -> GpuFieldRead<Vec<IntelEngineRead>> {
    GpuFieldRead::available(
        engines
            .iter()
            .map(|(name, active, total)| IntelEngineRead {
                name: (*name).to_string(),
                busy: EngineBusySource::Ticks {
                    active: *active,
                    total: *total,
                },
            })
            .collect(),
    )
}

/// A missing i915 layout is cached for the current bounded interval, then
/// probed again at the deadline. The nonexistent device path makes this a
/// host-independent recovery-state test without requiring a real PMU.
#[test]
fn absent_i915_probe_is_rate_limited_then_retried() {
    let mut fallback = IntelPmuFallback::default();
    let started_at = Instant::now();

    fallback.fallback_if_empty(
        DEVICE,
        Path::new("/nonexistent/i915"),
        GpuFieldRead::available(Vec::new()),
        started_at,
    );
    let first_retry_at = match fallback.devices.get(DEVICE) {
        Some(IntelPmuDeviceState::Absent { retry_at, .. }) => *retry_at,
        Some(IntelPmuDeviceState::Active { .. }) => {
            panic!("nonexistent i915 path must not produce active counters")
        }
        None => panic!("first probe must record a device state"),
    };

    fallback.fallback_if_empty(
        DEVICE,
        Path::new("/nonexistent/i915"),
        GpuFieldRead::available(Vec::new()),
        started_at + Duration::from_secs(1),
    );
    let still_first_retry_at = match fallback.devices.get(DEVICE) {
        Some(IntelPmuDeviceState::Absent { retry_at, .. }) => *retry_at,
        Some(IntelPmuDeviceState::Active { .. }) => {
            panic!("nonexistent i915 path must remain absent")
        }
        None => panic!("rate-limited state must remain cached"),
    };
    assert_eq!(
        still_first_retry_at, first_retry_at,
        "an absent PMU must not hot-loop probes before its retry deadline"
    );

    fallback.fallback_if_empty(
        DEVICE,
        Path::new("/nonexistent/i915"),
        GpuFieldRead::available(Vec::new()),
        first_retry_at,
    );
    let second_retry_at = match fallback.devices.get(DEVICE) {
        Some(IntelPmuDeviceState::Absent { retry_at, .. }) => *retry_at,
        Some(IntelPmuDeviceState::Active { .. }) => {
            panic!("nonexistent i915 path must remain absent after retry")
        }
        None => panic!("retry must leave an absent state recorded"),
    };
    assert!(
        second_retry_at > first_retry_at,
        "the retry deadline must advance after a failed re-probe"
    );
}

#[test]
fn counter_delta_produces_rate_and_first_tick_only_seeds() {
    let started_at = Instant::now();
    let mut tracker = IntelEngineTracker::default();

    // Tick 1: baseline seed — no rate yet, empty engine list, no failure.
    let first = tracker.observe(DEVICE, read_of(&[("Render/3D", 0)]), started_at);
    assert!(first.engines.is_empty());
    assert!(first.failure.is_none());

    // Tick 2: +500 ms busy over +1 s → 50% render utilization.
    let second = tracker.observe(
        DEVICE,
        read_of(&[("Render/3D", 500_000_000)]),
        started_at + Duration::from_secs(1),
    );
    assert_eq!(second.engines.len(), 1);
    assert_eq!(second.engines[0].name, "Render/3D");
    assert_eq!(second.engines[0].kind, GpuEngineKind::Render);
    assert!(
        (second.engines[0].usage_pct - 50.0).abs() < 0.01,
        "expected ~50%, got {}",
        second.engines[0].usage_pct
    );
    assert!(second.failure.is_none());

    // Tick 3: a second engine appears (Video Decode) — it only seeds, while
    // Render keeps producing a rate from its established baseline.
    let third = tracker.observe(
        DEVICE,
        read_of(&[("Render/3D", 1_000_000_000), ("Video Decode", 0)]),
        started_at + Duration::from_secs(2),
    );
    let render = third
        .engines
        .iter()
        .find(|engine| engine.name == "Render/3D")
        .expect("render rate present");
    assert!((render.usage_pct - 50.0).abs() < 0.01);
    assert!(
        third
            .engines
            .iter()
            .all(|engine| engine.name != "Video Decode"),
        "first sighting of Video Decode must not emit a rate yet"
    );
}

#[test]
fn percentage_snapshot_mode_passes_small_values_through() {
    let started_at = Instant::now();
    let mut tracker = IntelEngineTracker::default();

    // Seed with a percentage value (≤ 100).
    tracker.observe(DEVICE, read_of(&[("Copy", 42)]), started_at);
    // Next tick: snapshot 73% — passed through unchanged, no delta math.
    let second = tracker.observe(
        DEVICE,
        read_of(&[("Copy", 73)]),
        started_at + Duration::from_secs(1),
    );
    assert_eq!(second.engines.len(), 1);
    assert!(
        (second.engines[0].usage_pct - 73.0).abs() < 1e-6,
        "percentage snapshot must pass through: got {}",
        second.engines[0].usage_pct
    );
    assert!(second.failure.is_none());
}

#[test]
fn counter_reset_is_identity_change_and_reseeds() {
    let started_at = Instant::now();
    let mut tracker = IntelEngineTracker::default();
    tracker.observe(DEVICE, read_of(&[("Render/3D", 5_000_000_000)]), started_at);

    // Counter goes backwards → IdentityChanged, no rate emitted, baseline
    // reseeded to the new value so the next tick is clean.
    let reset = tracker.observe(
        DEVICE,
        read_of(&[("Render/3D", 100)]),
        started_at + Duration::from_secs(1),
    );
    assert!(reset.engines.is_empty());
    assert_eq!(reset.failure, Some(FailureKind::IdentityChanged));

    // After reseed, a forward step produces a normal rate again.
    // Baseline is 100 ns at +1 s; advancing to 500_000_100 ns at +2 s is a
    // +500 ms busy delta over +1 s → 50% render utilization.
    let resumed = tracker.observe(
        DEVICE,
        read_of(&[("Render/3D", 500_000_100)]),
        started_at + Duration::from_secs(2),
    );
    assert_eq!(resumed.engines.len(), 1);
    assert!(
        (resumed.engines[0].usage_pct - 50.0).abs() < 0.01,
        "expected ~50% after reseed, got {}",
        resumed.engines[0].usage_pct
    );
}

#[test]
fn absent_engine_tree_keeps_baselines_for_other_devices_and_prunes_own() {
    let started_at = Instant::now();
    let mut tracker = IntelEngineTracker::default();
    // Seed two devices.
    tracker.observe(
        "gpu:pci:0000:00:02.0",
        read_of(&[("Render/3D", 0)]),
        started_at,
    );
    tracker.observe(
        "gpu:pci:0000:01:00.0",
        read_of(&[("Render/3D", 0)]),
        started_at,
    );

    // Device A reappears with a rate; device B disappears for a tick.
    let second = tracker.observe(
        "gpu:pci:0000:00:02.0",
        read_of(&[("Render/3D", 1_000_000_000)]),
        started_at + Duration::from_secs(1),
    );
    assert_eq!(second.engines.len(), 1);
    assert_eq!(second.failure, None);

    // Generation prune drops only the named device's baselines.
    tracker.prune(&[DeviceId::new("gpu:pci:0000:01:00.0")]);
    assert!(
        tracker
            .previous
            .contains_key("gpu:pci:0000:00:02.0|Render/3D"),
        "unrelated device baseline must survive prune"
    );
    assert!(
        !tracker
            .previous
            .contains_key("gpu:pci:0000:01:00.0|Render/3D"),
        "pruned device baseline must be dropped"
    );
}

// ---- xe two-counter ticks path (TickRatio) ----------------------------

/// xe active/total deltas produce a rate as `active_delta / total_delta`,
/// IGNORING wall-elapsed — unlike i915. First tick seeds, no rate yet.
#[test]
fn xe_tick_ratio_produces_rate_ignoring_wall_elapsed() {
    let started_at = Instant::now();
    let mut tracker = IntelEngineTracker::default();

    // Tick 1: baseline seed (active 0, total 0) — no rate yet.
    let first = tracker.observe(DEVICE, ticks_of(&[("Render/3D", 0, 0)]), started_at);
    assert!(first.engines.is_empty());
    assert!(first.failure.is_none());

    // Tick 2: +250 active over +1000 total → 25%. A wall-clock reader would
    // instead divide by the 1-second interval and fabricate a unit-mismatched
    // number; the TickRatio arm must NOT.
    let second = tracker.observe(
        DEVICE,
        ticks_of(&[("Render/3D", 250, 1000)]),
        started_at + Duration::from_secs(1),
    );
    assert_eq!(second.engines.len(), 1);
    assert_eq!(second.engines[0].name, "Render/3D");
    assert!(
        (second.engines[0].usage_pct - 25.0).abs() < 0.01,
        "expected 25% active/total ratio, got {}",
        second.engines[0].usage_pct
    );

    // Tick 3: a sibling xe engine (Copy) appears — it only seeds.
    let third = tracker.observe(
        DEVICE,
        ticks_of(&[("Render/3D", 1000, 2000), ("Copy", 0, 0)]),
        started_at + Duration::from_secs(2),
    );
    let render = third
        .engines
        .iter()
        .find(|engine| engine.name == "Render/3D")
        .expect("render rate present");
    // Render: +750 active over +1000 total → 75%.
    assert!((render.usage_pct - 75.0).abs() < 0.01);
    assert!(
        third.engines.iter().all(|engine| engine.name != "Copy"),
        "first sighting of Copy must not emit a rate yet"
    );
}

/// xe `total_delta == 0` in an interval is a typed gap
/// (`IdentityChanged`), never a divide-by-zero.
#[test]
fn xe_tick_ratio_zero_total_delta_is_typed_gap_not_divide_by_zero() {
    let started_at = Instant::now();
    let mut tracker = IntelEngineTracker::default();
    tracker.observe(DEVICE, ticks_of(&[("Copy", 10, 100)]), started_at);

    // Same active, same total: deltas are both 0 → total_delta == 0 → gap.
    let stalled = tracker.observe(
        DEVICE,
        ticks_of(&[("Copy", 10, 100)]),
        started_at + Duration::from_secs(1),
    );
    assert!(stalled.engines.is_empty());
    assert_eq!(stalled.failure, Some(FailureKind::IdentityChanged));

    // After reseed, a forward step resumes normally: +5 active over +50
    // total → 10%.
    let resumed = tracker.observe(
        DEVICE,
        ticks_of(&[("Copy", 15, 150)]),
        started_at + Duration::from_secs(2),
    );
    assert_eq!(resumed.engines.len(), 1);
    assert!(
        (resumed.engines[0].usage_pct - 10.0).abs() < 0.01,
        "expected 10% after reseed, got {}",
        resumed.engines[0].usage_pct
    );
}

/// A xe ticks counter rolling backwards on EITHER active or total is a
/// reset/wrap reseed (`IdentityChanged`), mirroring the i915 ns rollback.
#[test]
fn xe_tick_counter_rollback_is_identity_change_and_reseeds() {
    let started_at = Instant::now();
    let mut tracker = IntelEngineTracker::default();
    tracker.observe(DEVICE, ticks_of(&[("Render/3D", 500, 1000)]), started_at);

    // Active goes backwards (driver reset) → IdentityChanged, no rate.
    let reset = tracker.observe(
        DEVICE,
        ticks_of(&[("Render/3D", 100, 1500)]),
        started_at + Duration::from_secs(1),
    );
    assert!(reset.engines.is_empty());
    assert_eq!(reset.failure, Some(FailureKind::IdentityChanged));

    // Total rolling backwards alone is ALSO a reseed, never a fabricated
    // negative ratio.
    let mut second = IntelEngineTracker::default();
    second.observe(DEVICE, ticks_of(&[("Copy", 100, 1000)]), started_at);
    let total_back = second.observe(
        DEVICE,
        ticks_of(&[("Copy", 200, 500)]),
        started_at + Duration::from_secs(1),
    );
    assert!(total_back.engines.is_empty());
    assert_eq!(total_back.failure, Some(FailureKind::IdentityChanged));
}

/// A source swap mid-session (sysfs ns → xe ticks for the same engine key)
/// is a unit change: an `IdentityChanged` reseed, never a mixed-unit rate.
/// The xe source then takes over cleanly on the next tick.
#[test]
fn mixed_unit_source_swap_is_identity_change_then_clean_reseed() {
    let started_at = Instant::now();
    let mut tracker = IntelEngineTracker::default();
    // Seed as nanoseconds (sysfs / i915 path).
    tracker.observe(DEVICE, read_of(&[("Render/3D", 1_000_000)]), started_at);

    // Same engine key arrives as xe ticks → mixed units → IdentityChanged.
    let swapped = tracker.observe(
        DEVICE,
        ticks_of(&[("Render/3D", 0, 0)]),
        started_at + Duration::from_secs(1),
    );
    assert!(swapped.engines.is_empty());
    assert_eq!(swapped.failure, Some(FailureKind::IdentityChanged));

    // Now the xe baseline is seeded; the next xe tick rates cleanly.
    let resumed = tracker.observe(
        DEVICE,
        ticks_of(&[("Render/3D", 300, 1000)]),
        started_at + Duration::from_secs(2),
    );
    assert_eq!(resumed.engines.len(), 1);
    assert!(
        (resumed.engines[0].usage_pct - 30.0).abs() < 0.01,
        "expected 30% after the xe reseed, got {}",
        resumed.engines[0].usage_pct
    );
}

/// The TickRatio rate is clamped to 100% if a runaway active delta ever
/// exceeds total (defensive — the rollback guard already catches true
/// backwards motion; this guards a driver reporting active > total).
#[test]
fn xe_tick_ratio_clamps_above_100() {
    let pct = engine_usage_pct(
        EngineBusyDelta::TickRatio {
            active_delta: 1500,
            total_delta: 1000,
        },
        Instant::now(),
        Instant::now(),
    )
    .expect("runaway active must not error, only clamp");
    assert!(
        (pct - 100.0).abs() < 1e-6,
        "expected clamp to 100%, got {pct}"
    );
}
