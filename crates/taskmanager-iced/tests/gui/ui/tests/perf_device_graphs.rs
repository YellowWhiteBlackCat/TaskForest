//! Per-device mini-graph wiring tests: each device row's graph plots that
//! device's OWN per-device window from the shared `MetricHistory` (never a
//! sibling's, never the system aggregate), and the GPU / Disk / Network sections
//! compose for both the collecting (<2 samples) and plotted (>=2 samples)
//! states. Split out of [`super::devices`] so neither file breaches the line
//! budget.

use super::super::perf_devices::battery::battery_section_state;
use super::super::tables::ListState;
use super::super::*;

#[test]
fn per_device_windows_resolve_for_the_exact_keys_the_renderer_uses() {
    // The renderer resolves each device's window through the SAME stable key the
    // recorder writes (disk: name→model; network: interface→device_id; gpu:
    // device_id→brand). Drive the recorder N times, then assert each device's
    // window is exactly the N recorded samples — proving the per-device graph on
    // that row lines up with that device's own history (never a sibling's, never
    // the system aggregate).
    let shell = taskmanager_shell::demo_app();
    let snapshot = shell
        .projection()
        .snapshot
        .clone()
        .expect("demo snapshot fixture");
    let mut app = crate::IcedApp::default();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot.clone()))),
    );
    // The default frontend has no recorded history; record three fresh samples
    // so every per-device window crosses the 2-sample floor the graph needs.
    for _ in 0..3 {
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut app.shell,
            &snapshot,
            None,
            None,
        );
    }

    // GPU: demo xe reports 18% utilization → window of three 18.0 samples.
    let gpu = &snapshot.gpu[0];
    assert_eq!(
        app.shell
            .history
            .gpu_usage_pct_for(&gpu.device_id, gpu.device_generation.get()),
        vec![18.0, 18.0, 18.0],
        "per-GPU window must resolve by (device_id, brand)"
    );
    // A foreign device_id resolves to an empty window (honest, never a borrow).
    assert!(
        app.shell
            .history
            .gpu_usage_pct_for("gpu:pci:0000:01:00.0", gpu.device_generation.get())
            .is_empty()
    );

    // Disk: 84 + 31 MiB/s read+write = 120_586_240 B/s, three samples.
    const MIB: f32 = (1024.0 * 1024.0) as f32;
    let disk = &snapshot.disks[0];
    assert_eq!(
        app.shell
            .history
            .disk_bytes_per_sec_for(&disk.device_id, disk.device_generation.get()),
        vec![(84.0 + 31.0) * MIB; 3],
        "per-disk window must resolve by (name, model)"
    );
    // The same accepted frames feed the disk's activity ring: the demo disk
    // reports 12.7% active time, and the Disk page's percentage curve consumes
    // exactly that window.
    assert_eq!(
        app.shell
            .history
            .disk_active_time_pct_for(&disk.device_id, disk.device_generation.get()),
        vec![12.7; 3],
        "per-disk active-time window resolves from the same recorded frames"
    );
    assert!(
        app.shell
            .history
            .disk_active_time_pct_for("disk:wwid:never-seen", disk.device_generation.get())
            .is_empty(),
        "an unknown disk resolves to an honest empty window"
    );

    // Network: 12 + 2 MiB/s rx+tx = 14_680_064 B/s, three samples.
    let nic = &snapshot.networks[0];
    assert_eq!(
        app.shell
            .history
            .network_bytes_per_sec_for(&nic.device_id, nic.device_generation.get()),
        vec![(12.0 + 2.0) * MIB; 3],
        "per-NIC window must resolve by (interface_name, device_id)"
    );
}

#[test]
fn device_sections_render_per_device_graphs_with_and_without_history() {
    // The device sections must compose for both honest states a device page
    // reaches: (a) a fresh frontend with a snapshot but no recorded per-device
    // window (<2 samples → the "· collecting" caption, canvas strokes nothing),
    // and (b) a session that has accrued samples (>=2 → the per-device graph
    // actually plots). Both paths must render without panicking.
    let mut app = crate::IcedApp::default();
    let demo = taskmanager_shell::demo_app();
    let snapshot = demo
        .projection()
        .snapshot
        .clone()
        .expect("demo snapshot fixture");

    // (a) Snapshot present, history empty: each device's window has 0 samples.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot.clone()))),
    );
    assert_eq!(gpu_section_state(Some(&snapshot)), ListState::Ready);
    assert!(
        app.shell
            .history
            .gpu_usage_pct_for(
                &snapshot.gpu[0].device_id,
                snapshot.gpu[0].device_generation.get()
            )
            .is_empty(),
        "fresh frontend has no per-device samples yet"
    );
    for device in [
        PerfDevice::Gpu(0),
        PerfDevice::Disk(0),
        PerfDevice::Network(0),
    ] {
        let _ = app.update(Message::SelectPerfDevice(device));
        assert_eq!(app.perf_device(), device);
        let _ = view(&app); // collecting-caption path; render-and-drop.
    }

    // (b) Record enough snapshots that every per-device window crosses the
    // 2-sample floor; the per-device graph now has data to plot.
    for _ in 0..4 {
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut app.shell,
            &snapshot,
            None,
            None,
        );
    }
    assert!(
        app.shell
            .history
            .disk_bytes_per_sec_for(
                &snapshot.disks[0].device_id,
                snapshot.disks[0].device_generation.get()
            )
            .len()
            >= 2
    );
    for device in [
        PerfDevice::Gpu(0),
        PerfDevice::Disk(0),
        PerfDevice::Network(0),
    ] {
        let _ = app.update(Message::SelectPerfDevice(device));
        assert_eq!(app.perf_device(), device);
        let _ = view(&app); // graph-plots path; render-and-drop.
    }
}

/// The hover-ready main graphs (GraphPrefs.hover = true) resolve their
/// windows, the hover mapping reaches the latest sample, and the readout pill
/// formats that sample in the graph's unit — then the section renders. The
/// round trip proves the hover data path the draw pass consumes lines up with
/// the section render path (never a fabricated reading).
#[test]
fn device_main_graphs_read_out_the_hovered_sample_and_render() {
    let mut app = crate::IcedApp::default();
    let demo = taskmanager_shell::demo_app();
    let snapshot = demo
        .projection()
        .snapshot
        .clone()
        .expect("demo snapshot fixture");
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot.clone()))),
    );
    for _ in 0..3 {
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut app.shell,
            &snapshot,
            None,
            None,
        );
    }

    // The GPU main graph plots the per-GPU window; an in-frame cursor at the
    // right edge maps to the newest sample, whose readout is the formatted
    // value the pill would draw (18% — the demo xe's utilization).
    let gpu = &snapshot.gpu[0];
    let window = app
        .shell
        .history
        .gpu_usage_pct_for(&gpu.device_id, gpu.device_generation.get());
    assert_eq!(
        window,
        vec![18.0; 3],
        "the per-GPU window has three samples"
    );
    let index = crate::perf_chart::hovered_index(199.0, 800.0, window.len())
        .expect("an in-frame cursor maps to a sample");
    assert_eq!(
        super::super::device_chart::device_readout_text(
            super::super::device_chart::DeviceMetricScale::Percent,
            &window,
            index,
        ),
        Some("18%".to_string()),
        "the pill must read out the hovered sample in the graph's unit"
    );

    let _ = app.update(Message::SelectPerfDevice(PerfDevice::Gpu(0)));
    assert_eq!(app.perf_device(), PerfDevice::Gpu(0));
    let _ = view(&app); // hover-enabled GPU main graph; render-and-drop.
}

#[test]
fn battery_and_fan_per_device_windows_resolve_and_render() {
    // The battery + fan graphs resolve their windows through keys the disk/net/
    // gpu graphs do not share: battery → `battery_capacity_pct_for(battery.id)`;
    // fan → `fan_rpm_for(label, device_id)`. Drive each per-device recorder
    // (the power-supply feed and the sensor feed — separate from
    // `record_snapshot`), then assert each device's window is exactly the
    // recorded samples — proving the per-device graph on that row lines up with
    // that device's own history (never a sibling's, never fabricated). Both
    // sections must then compose for the collecting (<2 samples) and plotted
    // (>=2 samples) states.
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::power::{BatteryInfo, PowerSupplySnapshot};
    use taskmanager_core::core::sensors::SensorCenterSnapshot;

    let mut app = crate::IcedApp::default();

    // Battery: record BAT0 at 72% three times.
    let mut battery = BatteryInfo::new("BAT0", DeviceState::healthy(10));
    battery.apply_scalar_observations(taskmanager_core::core::power::BatteryScalarObservations {
        capacity_pct: taskmanager_core::core::metrics::ScalarObservation::available(72, 10),
        ..Default::default()
    });
    let power = PowerSupplySnapshot {
        state: DeviceState::healthy(10),
        timestamp_ms: 10,
        batteries: vec![battery],
        ..PowerSupplySnapshot::default()
    };
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::PowerSupplies(Some(power.clone())),
    );
    let dynamic_system = taskmanager_core::core::metrics::SystemSnapshot {
        timestamp_ms: 1_000,
        ..Default::default()
    };
    for _ in 0..3 {
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut app.shell,
            &dynamic_system,
            Some(&power),
            None,
        );
    }
    assert_eq!(
        app.shell.history.battery_capacity_pct_for("BAT0"),
        vec![72.0, 72.0, 72.0],
        "per-battery window must resolve by id"
    );
    // A foreign battery id resolves to an empty window (honest, never a borrow).
    assert!(
        app.shell
            .history
            .battery_capacity_pct_for("BAT9")
            .is_empty()
    );

    // Fan: record the cpu_fan channel at 1500 RPM three times.
    let sensors = SensorCenterSnapshot {
        state: DeviceState::healthy(1_000),
        timestamp_ms: 1_000,
        readings: vec![sample_fan_reading("cpu_fan", "hwmon:cpu", 1_500)],
        ..SensorCenterSnapshot::default()
    };
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sensors(Some(sensors.clone())),
    );
    for _ in 0..3 {
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut app.shell,
            &dynamic_system,
            None,
            Some(&sensors),
        );
    }
    assert_eq!(
        app.shell.history.fan_rpm_for("fan1"),
        vec![1_500.0, 1_500.0, 1_500.0],
        "per-fan window must resolve by (label, device_id)"
    );
    assert!(
        app.shell.history.fan_rpm_for("gpu_fan").is_empty(),
        "a foreign fan key resolves to an empty window"
    );

    // Both panels are Ready and render for the plotted (>=2 samples) state. The
    // collecting (<2 samples) caption path is exercised by the sibling gpu/disk/
    // net test; here the windows hold three samples each, so the per-device
    // graphs actually plot.
    assert_eq!(
        battery_section_state(app.shell.projection().power_supplies.as_ref()),
        ListState::Ready
    );
    for device in [PerfDevice::Battery(0), PerfDevice::Fan(0)] {
        let _ = app.update(Message::SelectPerfDevice(device));
        assert_eq!(app.perf_device(), device);
        let _ = view(&app); // graph-plots path; render-and-drop.
    }
}

/// Build a fan `SensorReading` whose `current_value()` projects to
/// `FanRpm(rpm)` through the typed measurement observation (the same shape
/// `fan_summary_lines` / `record_sensors` read). Used by the battery/fan
/// per-device test to seed the per-fan RPM window without re-deriving the full
/// observation builder inline.
fn sample_fan_reading(
    label: &str,
    device_id: &str,
    rpm: u32,
) -> taskmanager_core::core::sensors::SensorReading {
    use taskmanager_core::core::identity::DeviceGeneration;
    use taskmanager_core::core::sensors::{
        SensorDescriptor, SensorMagnitude, SensorMeasurementObservation, SensorReading, SensorScale,
    };

    SensorReading::from_measurement_observation(
        device_id.into(),
        "fan1".into(),
        label.into(),
        SensorMeasurementObservation::available(
            SensorDescriptor::fan_speed(SensorScale::IDENTITY),
            SensorMagnitude::Unsigned(u64::from(rpm)),
            1_000,
        )
        .expect("valid fan magnitude"),
    )
    .with_device_generation(DeviceGeneration::new(1))
}
