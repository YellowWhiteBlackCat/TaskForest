//! Device-page history + SMART-verdict render tests (F04/F07/F08 ledger
//! gaps): the disk SMART status verdict line, the disk SMART temperature
//! history, the battery power history, and the fan temperature history.
//! Extracted as a small topic file so `device_render.rs` stays inside its
//! line budget; each test asserts rendered text/state, never source text.

use taskmanager_application::{
    BatteryInfo, DeviceId, DeviceState, PowerSupplySnapshot, SensorCenterSnapshot,
    SensorDescriptor, SensorMagnitude, SensorMeasurementObservation, SensorReading, SensorScale,
    SmartAvailability,
};

use super::frame_text;

fn observed_battery() -> BatteryInfo {
    let mut battery = BatteryInfo::new("BAT0", DeviceState::healthy(1_000));
    battery.status = "Discharging".into();
    battery.apply_scalar_observations(taskmanager_application::BatteryScalarObservations {
        capacity_pct: taskmanager_application::ScalarObservation::available(80, 1_000),
        power_w: taskmanager_application::ScalarObservation::available(9.5, 1_000),
        ..Default::default()
    });
    battery
}

fn sensor_reading(
    device_id: &str,
    id: &str,
    label: &str,
    descriptor: SensorDescriptor,
    magnitude: SensorMagnitude,
) -> SensorReading {
    SensorReading::from_measurement_observation(
        DeviceId::new(device_id),
        id.into(),
        label.into(),
        SensorMeasurementObservation::available(descriptor, magnitude, 1_000)
            .expect("valid sensor fixture"),
    )
}

fn record_dynamic_history(
    app: &mut crate::TuiApp,
    power: Option<&PowerSupplySnapshot>,
    sensors: Option<&SensorCenterSnapshot>,
) {
    let system = app.projection().snapshot.clone().unwrap_or_default();
    taskmanager_shell::fixture::record_demo_history_frame(&mut app.shell, &system, power, sensors);
}

/// F04: a disk whose SMART provider never opened hides the whole SMART section
/// (nothing to show); an available provider that has not produced scalars yet
/// keeps the honest status verdict alongside any recorded temperature history;
/// once the live disk carries a SMART field the verdict yields to readouts.
#[test]
fn disk_block_renders_smart_verdict_and_temperature_history() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Disk;

    // The demo disk exposes no SMART field and its provider never opened
    // (availability Unavailable): the SMART section is hidden entirely.
    let verdict_text = frame_text(&app, 140, 48);
    assert!(
        !verdict_text.contains("SMART status"),
        "an unavailable SMART provider must hide the section, not render a verdict"
    );
    assert!(
        !verdict_text.contains("Latest 42°C"),
        "no temperature history may render before any sample exists"
    );

    // Record two distinct authoritative SMART samples through the same
    // correlated per-device history used in production. The live provider is
    // then left available without a current scalar, so verdict and retained
    // device history coexist honestly.
    let snapshot = app
        .projection()
        .snapshot
        .as_ref()
        .expect("demo app should carry a snapshot")
        .clone();
    for timestamp_ms in [1_000_u64, 2_000_u64] {
        let mut measured = snapshot.clone();
        measured.timestamp_ms = timestamp_ms;
        measured.disks[0].smart_availability = SmartAvailability::Available;
        measured.disks[0].smart_state = DeviceState::healthy(timestamp_ms);
        measured.disks[0].smart_temperature_c = Some(42.0);
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut app.shell,
            &measured,
            None,
            None,
        );
    }
    let mut live_snapshot = app.projection().snapshot.clone().expect("demo snapshot");
    let live_disk = &mut live_snapshot.disks[0];
    live_disk.smart_availability = SmartAvailability::Available;
    live_disk.smart_state = DeviceState::healthy(2_000);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(live_snapshot))),
    );
    let history_text = frame_text(&app, 140, 48);
    assert!(
        history_text.contains("Latest 42°C"),
        "the temperature history summary must render from the shared window"
    );
    assert!(
        history_text.contains("SMART status"),
        "an available provider without scalars keeps the honest status verdict"
    );

    // Once the live disk carries a SMART field the verdict yields to the
    // readouts (mirroring iced's `!has_smart_fields` gate).
    let mut snapshot = app.projection().snapshot.clone().expect("demo snapshot");
    snapshot.disks[0].smart_temperature_c = Some(42.0);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
    let measured_text = frame_text(&app, 140, 48);
    assert!(
        !measured_text.contains("SMART status"),
        "the verdict must yield once a SMART readout exists"
    );
    assert!(
        measured_text.contains("Latest 42°C"),
        "the temperature history must survive the verdict yield"
    );
}

/// F07: the power scalar row gains a power-flow history (trend + summary)
/// from the battery's OWN window only once at least two samples exist — a
/// single sample cannot show a SHAPE, so no line renders for it.
#[test]
fn battery_block_renders_power_history_at_two_or_more_samples() {
    let battery = observed_battery();
    let supply = PowerSupplySnapshot {
        state: DeviceState::healthy(1_000),
        timestamp_ms: 1_000,
        batteries: vec![battery],
        ..Default::default()
    };

    // One recorded power snapshot: the scalar power row renders, but the
    // power-history line must NOT (below the two-sample floor).
    let mut sparse = crate::demo_app();
    sparse.perf_device = crate::PerfDevice::Battery;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut sparse.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::PowerSupplies(Some(PowerSupplySnapshot {
            timestamp_ms: 1_000,
            batteries: vec![observed_battery()],
            ..Default::default()
        })),
    );
    record_dynamic_history(&mut sparse, Some(&supply), None);
    let sparse_text = frame_text(&sparse, 140, 48);
    assert!(
        sparse_text.contains("9.5 W"),
        "the scalar power row must render from the first sample"
    );
    assert!(
        !sparse_text.contains("Latest 9.5 W"),
        "the power history must not render below two samples"
    );

    // Two recorded power snapshots: the power-history summary renders with
    // the watts unit from the battery's OWN window.
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Battery;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::PowerSupplies(Some(PowerSupplySnapshot {
            timestamp_ms: 1_000,
            batteries: vec![observed_battery()],
            ..Default::default()
        })),
    );
    record_dynamic_history(&mut app, Some(&supply), None);
    record_dynamic_history(&mut app, Some(&supply), None);
    assert_eq!(
        app.history.battery_power_w_for("BAT0"),
        vec![9.5, 9.5],
        "the shared dynamic store retains this battery's typed power samples"
    );
    let text = frame_text(&app, 140, 48);
    assert!(
        text.contains("Latest 9.5 W"),
        "the power history summary must render at two samples"
    );
    assert!(
        text.contains("Discharging"),
        "the scalar status row must still render alongside the history"
    );
}

/// F08: the fan's device-temperature rows gain a temperature history (trend +
/// summary) keyed by the fan channel's OWN window once at least two samples
/// exist — a single sample cannot show a SHAPE, so no line renders for it.
#[test]
fn fan_block_renders_temperature_history_at_two_or_more_samples() {
    let fan = sensor_reading(
        "hwmon1",
        "fan1",
        "CPU Fan",
        SensorDescriptor::fan_speed(SensorScale::IDENTITY),
        SensorMagnitude::Unsigned(1_500),
    );
    let temperature = sensor_reading(
        "hwmon1",
        "temp1",
        "Package",
        SensorDescriptor::temperature(SensorScale::IDENTITY),
        SensorMagnitude::Decimal(51.0),
    );
    let sensors = SensorCenterSnapshot {
        timestamp_ms: 1_000,
        readings: vec![fan, temperature],
        ..Default::default()
    };

    // One recorded sensor snapshot: the scalar temperature row renders, but
    // the temperature-history line must NOT (below the two-sample floor).
    let mut sparse = crate::demo_app();
    sparse.perf_device = crate::PerfDevice::Fan;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut sparse.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sensors(Some(SensorCenterSnapshot {
            timestamp_ms: 1_000,
            readings: vec![
                sensor_reading(
                    "hwmon1",
                    "fan1",
                    "CPU Fan",
                    SensorDescriptor::fan_speed(SensorScale::IDENTITY),
                    SensorMagnitude::Unsigned(1_500),
                ),
                sensor_reading(
                    "hwmon1",
                    "temp1",
                    "Package",
                    SensorDescriptor::temperature(SensorScale::IDENTITY),
                    SensorMagnitude::Decimal(51.0),
                ),
            ],
            ..Default::default()
        })),
    );
    record_dynamic_history(&mut sparse, None, Some(&sensors));
    let sparse_text = frame_text(&sparse, 140, 48);
    assert!(
        sparse_text.contains("51.0 °C"),
        "the scalar device temperature must render from the first sample"
    );
    assert!(
        !sparse_text.contains("Latest 51°C"),
        "the temperature history must not render below two samples"
    );

    // Two recorded sensor snapshots: the temperature-history summary renders
    // with the °C unit from the fan channel's OWN window.
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Fan;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sensors(Some(SensorCenterSnapshot {
            timestamp_ms: 1_000,
            readings: vec![
                sensor_reading(
                    "hwmon1",
                    "fan1",
                    "CPU Fan",
                    SensorDescriptor::fan_speed(SensorScale::IDENTITY),
                    SensorMagnitude::Unsigned(1_500),
                ),
                sensor_reading(
                    "hwmon1",
                    "temp1",
                    "Package",
                    SensorDescriptor::temperature(SensorScale::IDENTITY),
                    SensorMagnitude::Decimal(51.0),
                ),
            ],
            ..Default::default()
        })),
    );
    record_dynamic_history(&mut app, None, Some(&sensors));
    record_dynamic_history(&mut app, None, Some(&sensors));
    let text = frame_text(&app, 140, 48);
    assert!(
        text.contains("Latest 51°C"),
        "the temperature history summary must render at two samples"
    );
    assert!(
        text.contains("1500 RPM"),
        "the scalar RPM row must still render alongside the history"
    );
}
