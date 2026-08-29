//! Memory-page labelled stats rows (parity inventory §2.2) and the resource
//! selector's live per-instance strip (§2.6 B-1..3). Every expectation is a
//! painted frame fact read through the same typed projection accessors the
//! gpui truth files use (`perf_views/memory_stats.rs`, `sidebar/captions.rs`)
//! — behavior, never source text — and every render pins its language through
//! the shared guard.

use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::power::{BatteryInfo, PowerSupplySnapshot};
use taskmanager_core::core::sensors::{
    SensorCenterSnapshot, SensorDescriptor, SensorMagnitude, SensorMeasurementObservation,
    SensorReading,
};
use taskmanager_test_support::MemoryMetricsFixtureBuilder;

use super::frame_text;

const WIDE: (u16, u16) = (140, 48);
const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * 1024 * 1024;

/// Layer the seven measured facts onto the demo memory (which already
/// carries total/used/available/swap), so each stats row renders a real
/// fixture value.
fn seed_measured_memory(app: &mut crate::TuiApp) {
    app.perf_device = crate::PerfDevice::Memory;
    let mut snapshot = app.projection().snapshot.clone().expect("demo snapshot");
    snapshot.memory = MemoryMetricsFixtureBuilder::from_item(snapshot.memory.clone())
        .current_available_bytes(19 * GIB + 384 * MIB)
        .hardware_reserved_bytes(256 * MIB)
        .speed_mhz(6400)
        .slots_used(2)
        .slots_total(4)
        .committed_bytes(8 * GIB)
        .commit_limit_bytes(16 * GIB)
        .current_used_rate_mib_per_sec(128.5)
        .buffers_bytes(512 * MIB)
        .build();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
}

/// §2.2: every measured fact renders under its localized label through the
/// same typed accessors the gpui stats rows read — including the signed
/// MiB/s usage rate and the conditional Buffers row.
#[test]
fn memory_stats_rows_render_every_measured_fact() {
    let mut app = crate::demo_app();
    seed_measured_memory(&mut app);

    let text = frame_text(&app, WIDE.0, WIDE.1);

    for expected in [
        "Available 19.4 GiB",
        "Hardware reserved 256.0 MiB",
        "Speed 6400 MT/s",
        "Slots 2 / 4",
        "Committed 8.0 GiB / 16.0 GiB",
        "Usage rate +128.5 MiB/s",
        "Buffers 512.0 MiB",
    ] {
        assert!(
            text.contains(expected),
            "the memory stats row set lost {expected:?}:\n{text}"
        );
    }
}

/// §2.2 honesty: a fact the host never reported is the shared dash on its
/// own labelled row — never a fabricated zero — and the Buffers row keeps
/// its gpui conditional semantics (absent counter, absent row).
#[test]
fn memory_stats_rows_render_honest_dashes_when_unavailable() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Memory;
    // The demo memory carries total/used/available/swap/cached only: no
    // hardware reservation, module speed, slots, commit counters, usage
    // rate or buffers.

    let text = frame_text(&app, WIDE.0, WIDE.1);

    assert!(
        text.contains("Available 19.4 GiB"),
        "the measured fact renders:\n{text}"
    );
    for dashed in [
        "Hardware reserved —",
        "Speed —",
        "Slots —",
        "Committed —",
        "Usage rate —",
    ] {
        assert!(
            text.contains(dashed),
            "an unreported fact must dash, not zero: {dashed:?}:\n{text}"
        );
    }
    assert!(
        !text.contains("  Buffers "),
        "the conditional Buffers row must stay hidden without a counter:\n{text}"
    );
}

/// The signed usage rate renders both directions through the shared ladder,
/// and a rate below the gpui noise floor (|rate| < 0.05 MiB/s) is honest
/// absence rather than a believable `+0`.
#[test]
fn memory_usage_rate_is_signed_and_noise_gated() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Memory;
    let mut snapshot = app.projection().snapshot.clone().expect("demo snapshot");
    snapshot.memory = MemoryMetricsFixtureBuilder::from_item(snapshot.memory.clone())
        .current_used_rate_mib_per_sec(-40.0)
        .build();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
    let draining = frame_text(&app, WIDE.0, WIDE.1);
    assert!(
        draining.contains("Usage rate -40.0 MiB/s"),
        "a measured drain must carry its sign:\n{draining}"
    );

    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Memory;
    let mut snapshot = app.projection().snapshot.clone().expect("demo snapshot");
    snapshot.memory = MemoryMetricsFixtureBuilder::from_item(snapshot.memory.clone())
        .current_used_rate_mib_per_sec(0.01)
        .build();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
    let still = frame_text(&app, WIDE.0, WIDE.1);
    assert!(
        still.contains("Usage rate —"),
        "a sub-noise rate must dash instead of printing +0:\n{still}"
    );
}

/// §2.6 B-3: the strip carries the live caption fields per resource class —
/// each assertion proves the selector (not only the detail panel) paints
/// real data for the active class.
#[test]
fn selector_strip_paints_live_captions_for_every_resource_class() {
    // CPU: brand + utilization / clock / package temperature.
    let mut app = crate::demo_app();
    let cpu_text = frame_text(&app, WIDE.0, WIDE.1);
    for expected in ["Intel(R) Core(TM)", "37% · 3.28 GHz · 54 °C"] {
        assert!(
            cpu_text.contains(expected),
            "the CPU strip caption lost {expected:?}:\n{cpu_text}"
        );
    }

    // Memory: used / total plus the observed percentage.
    app.perf_device = crate::PerfDevice::Memory;
    let memory_text = frame_text(&app, WIDE.0, WIDE.1);
    assert!(
        memory_text.contains("12.6 GiB / 32.0 GiB · 39%"),
        "the memory strip caption must render:\n{memory_text}"
    );

    // Disk: model identity, activity and the summed read+write rate. The
    // strip caption follows the gpui sidebar rounding (whole percent), while
    // the detail panel keeps its one-decimal readout.
    app.perf_device = crate::PerfDevice::Disk;
    let disk_text = frame_text(&app, WIDE.0, WIDE.1);
    for expected in ["TiPro9000 2TB", "13% · 115.0 MiB/s"] {
        assert!(
            disk_text.contains(expected),
            "the disk strip caption lost {expected:?}:\n{disk_text}"
        );
    }

    // NIC: send/recv rates under the applied network unit pair (pinned to
    // bytes/base-2 like the unit-preference battery).
    app.perf_device = crate::PerfDevice::Network;
    app.prefs.units[4] = true;
    app.prefs.units[5] = true;
    let network_text = frame_text(&app, WIDE.0, WIDE.1);
    for expected in ["wlan0", "S: 2.0 MiB/s · R: 12.0 MiB/s"] {
        assert!(
            network_text.contains(expected),
            "the NIC strip caption lost {expected:?}:\n{network_text}"
        );
    }

    // GPU: identity, clock (the demo exposes no VRAM pair) and the live
    // utilization / temperature fields.
    app.perf_device = crate::PerfDevice::Gpu;
    let gpu_text = frame_text(&app, WIDE.0, WIDE.1);
    for expected in ["Intel Graphics (xe)", "900 MHz · 18% · 48 °C"] {
        assert!(
            gpu_text.contains(expected),
            "the GPU strip caption lost {expected:?}:\n{gpu_text}"
        );
    }
}

/// §2.6 B-1/B-2: every projected instance of the active class gets its own
/// segment, and each segment carries the device's own generation-scoped
/// history: the cold-start dotted placeholder before two samples exist, the
/// shared ramp once the recorder has fed the ring.
#[test]
fn selector_strip_lists_instances_with_their_own_sparkline() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Disk;
    // A second disk joins the demo NVMe device, each with its own identity.
    let mut snapshot = app.projection().snapshot.clone().expect("demo snapshot");
    let mut second = snapshot.disks[0].clone();
    second.name = "sda".into();
    second.model = "TaskDisk 1TB".into();
    second.device_id = "disk:demo:sda".into();
    snapshot.disks.push(second);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );

    let cold = frame_text(&app, WIDE.0, WIDE.1);
    assert!(
        cold.contains("TiPro9000 2TB") && cold.contains("TaskDisk 1TB"),
        "both disk instances must own a strip segment:\n{cold}"
    );
    assert!(
        cold.contains("TaskDisk 1TB ···· 13% · 115.0 MiB/s"),
        "a device history below two samples must render the honest collecting placeholder \
         inside its own strip segment (the demo-seeded device keeps its live ramp):\n{cold}"
    );

    // Two recorded frames feed the shared rings; the same generation-scoped
    // window that drives the detail panel now drives the strip segment's
    // inline ramp.
    let recorded = app.projection().snapshot.clone().expect("demo snapshot");
    for timestamp_ms in [1_000_u64, 2_000_u64] {
        let mut measured = recorded.clone();
        measured.timestamp_ms = timestamp_ms;
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut app.shell,
            &measured,
            None,
            None,
        );
    }
    let live = frame_text(&app, WIDE.0, WIDE.1);
    assert!(
        live.contains("TaskDisk 1TB ▅▅ 13% · 115.0 MiB/s"),
        "two recorded samples must replace the placeholder with the shared ramp \
         inside the strip segment:\n{live}"
    );
}

/// §2.6: the battery and fan-channel classes list their projected instances
/// (power_supplies.batteries / sensors.readings) with capacity and RPM
/// captions read through the typed accessors.
#[test]
fn selector_strip_lists_battery_and_fan_instances() {
    let mut charged = BatteryInfo::new("power-supply:BAT0", DeviceState::healthy(1_000));
    charged.status = "Discharging".into();
    charged.apply_scalar_observations(taskmanager_core::core::power::BatteryScalarObservations {
        capacity_pct: taskmanager_core::core::metrics::ScalarObservation::available(82, 1_000),
        ..Default::default()
    });
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Battery;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::PowerSupplies(Some(PowerSupplySnapshot {
            state: DeviceState::healthy(1_000),
            timestamp_ms: 1_000,
            batteries: vec![charged],
            ..Default::default()
        })),
    );
    let battery_text = frame_text(&app, WIDE.0, WIDE.1);
    for expected in ["Battery 0", "82% · Discharging"] {
        assert!(
            battery_text.contains(expected),
            "the battery strip caption lost {expected:?}:\n{battery_text}"
        );
    }

    let fan = SensorReading::from_measurement_observation(
        "hwmon:cpu".into(),
        "fan1".into(),
        "cpu_fan".into(),
        SensorMeasurementObservation::available(
            SensorDescriptor::fan_speed(taskmanager_core::core::sensors::SensorScale::IDENTITY),
            SensorMagnitude::Unsigned(2_400),
            1_000,
        )
        .expect("valid fan magnitude"),
    );
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Fan;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sensors(Some(SensorCenterSnapshot {
            state: DeviceState::healthy(1_000),
            timestamp_ms: 1_000,
            readings: vec![fan],
            ..Default::default()
        })),
    );
    let fan_text = frame_text(&app, WIDE.0, WIDE.1);
    for expected in ["Fan 1", "cpu_fan · 2400 RPM"] {
        assert!(
            fan_text.contains(expected),
            "the fan strip caption lost {expected:?}:\n{fan_text}"
        );
    }
}

/// The applied device visibility reaches the strip: a hidden VPN class drops
/// out of the NIC strip, and a hidden disk family drops out of the disk
/// strip, so no ghost instance renders beside the filtered detail panel.
#[test]
fn selector_strip_honors_the_applied_device_visibility() {
    use taskmanager_core::core::metrics::NetworkAdapterType;

    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Network;
    let mut snapshot = app.projection().snapshot.clone().expect("demo snapshot");
    let mut wired = snapshot.networks[0].clone();
    wired.interface_name = "eth0".into();
    let wired_scalars = *wired.scalar_observations();
    wired.apply_observations(
        NetworkAdapterType::Ethernet,
        wired_scalars,
        taskmanager_core::core::metrics::NetworkWirelessObservations::not_applicable(1),
    );
    let mut vpn = snapshot.networks[0].clone();
    vpn.interface_name = "tun0".into();
    let vpn_scalars = *vpn.scalar_observations();
    vpn.apply_observations(
        NetworkAdapterType::Vpn,
        vpn_scalars,
        taskmanager_core::core::metrics::NetworkWirelessObservations::not_applicable(1),
    );
    snapshot.networks = vec![wired, vpn];
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
    app.prefs.units[4] = true;
    app.prefs.units[5] = true;

    let both = frame_text(&app, WIDE.0, WIDE.1);
    assert!(
        both.contains("eth0") && both.contains("tun0"),
        "both visible NIC classes strip-render:\n{both}"
    );

    app.prefs.show[6] = false;
    let filtered = frame_text(&app, WIDE.0, WIDE.1);
    assert!(
        filtered.contains("eth0"),
        "the visible class stays:\n{filtered}"
    );
    assert!(
        !filtered.contains("tun0"),
        "a hidden VPN class must drop out of the strip too:\n{filtered}"
    );
}

/// Narrow-frame honesty: below the width where the resource tab row fits on
/// one line, the strip collapses (the wrapped tab row keeps the band) and no
/// truncated caption fragments can render. The memory stats rows likewise
/// yield to the overview's height gate instead of clipping mid-value.
#[test]
fn narrow_frames_collapse_the_strip_and_stats_honestly() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Memory;
    seed_measured_memory(&mut app);

    let compact = frame_text(&app, 54, 16);
    assert!(
        compact.contains("Memory"),
        "the resource tab row still renders at the floor:\n{compact}"
    );
    assert!(
        !compact.contains("12.6 GiB / 32.0 GiB"),
        "the strip must collapse before truncating a caption:\n{compact}"
    );
    assert!(
        !compact.contains("Hardware reserved"),
        "the stats rows must yield to the height gate, never clip mid-value:\n{compact}"
    );

    // A medium tier whose tab row still wraps stays strip-free as well.
    let medium = frame_text(&app, 70, 18);
    assert!(
        !medium.contains("12.6 GiB / 32.0 GiB"),
        "the strip stays collapsed while the tab row wraps:\n{medium}"
    );
}

/// Both locales resolve the stats labels through the shared catalog (current
/// values via `t()`, never hardcoded copy) around the same painted facts.
#[test]
fn memory_stats_rows_translate_through_the_shared_catalog() {
    use taskmanager_application::i18n::{self, Language};

    let mut app = crate::demo_app();
    seed_measured_memory(&mut app);

    for language in [Language::En, Language::Zh] {
        super::acceptance_support::with_frame_in_language(
            &app,
            WIDE.0,
            WIDE.1,
            language,
            |frame| {
                let available = i18n::t("mem.available");
                assert_ne!(
                    available, "mem.available",
                    "{language:?} must carry the key"
                );
                assert!(
                    frame.contains(available) && frame.contains("19.4 GiB"),
                    "{language:?} must paint the available stats row ({available:?}):\n{frame}"
                );
                let speed = i18n::t("common.speed");
                assert!(
                    frame.contains(speed) && frame.contains("6400 MT/s"),
                    "{language:?} must paint the speed stats row ({speed:?}):\n{frame}"
                );
            },
        );
    }
}
