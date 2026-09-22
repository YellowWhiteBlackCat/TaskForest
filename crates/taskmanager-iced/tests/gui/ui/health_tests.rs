use super::*;
use taskmanager_shell::demo_app;

#[test]
fn health_cpu_line_relabels_bogomips_instead_of_faking_mhz() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let shell = demo_app();
    let snapshot = shell.projection().snapshot.as_ref().expect("demo snapshot");
    let mut bogomips_only = snapshot.clone();
    bogomips_only.cpu.frequency_source =
        taskmanager_core::core::metrics::CpuFrequencySource::BogoMips;

    let rows = health_rows(&bogomips_only);
    // A BogoMIPS-only host must read the BogoMIPS readout, never "MHz".
    let cpu = rows.iter().find(|row| row.label == "CPU").expect("CPU row");
    assert!(cpu.value.contains("BogoMIPS"));
    assert!(!cpu.value.contains("MHz"));

    // The untouched native fixture stays an MHz clock.
    let native = health_rows(snapshot);
    let native_cpu = native
        .iter()
        .find(|row| row.label == "CPU")
        .expect("native CPU row");
    assert!(native_cpu.value.contains("MHz"));
}

#[test]
fn health_rows_cover_every_domain_with_fixture_values() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let shell = demo_app();
    let snapshot = shell.projection().snapshot.as_ref().expect("demo snapshot");
    let rows = health_rows(snapshot);

    assert_eq!(rows.len(), 8);
    let row = |label: &str| rows.iter().find(|row| row.label == label).expect(label);
    assert!(row("CPU").healthy);
    assert!(row("CPU").value.contains("37.4%"));
    assert!(row("Memory").healthy);
    assert!(row("Memory").value.contains("GiB"));
    assert!(row("Networks").healthy);
    assert!(row("Networks").value.contains("wlan0"));
    assert!(row("GPU").healthy);
    assert!(row("GPU").value.contains("Intel Graphics (xe)"));
    assert!(row("System").healthy);
    assert!(row("System").value.contains("347 processes"));
}

#[test]
fn health_rows_stay_honest_when_domains_are_absent() {
    let snapshot = SystemSnapshot::default();
    let rows = health_rows(&snapshot);
    assert_eq!(rows.len(), 7);
    for row in &rows {
        assert!(!row.healthy, "{} must not claim health", row.label);
    }
    assert!(rows[0].value.contains("—"));
    assert!(rows[1].value == "—");
}

#[test]
fn health_modal_renders_with_and_without_telemetry() {
    let app = crate::IcedApp::demo();
    let _view = render(&app);
    drop(_view);

    let mut app = crate::IcedApp::demo();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(None)),
    );
    let _view = render(&app);
}

mod thermal_zone_tests {
    use super::*;
    use taskmanager_core::core::failure::FailureKind;
    use taskmanager_core::core::identity::DeviceGeneration;
    use taskmanager_core::core::sensors::{
        SensorCenterSnapshot, SensorDescriptor, SensorMagnitude, SensorMeasurementObservation,
        SensorReading, SensorScale,
    };
    use taskmanager_shell::presentation::missing_value;

    /// A temperature channel in exactly the wire shape the sensor-center
    /// provider publishes: `Some` is a read value, `None` an explicit typed
    /// failure (never a zero).
    fn zone_reading(id: &str, label: &str, temperature_c: Option<f64>) -> SensorReading {
        let descriptor = SensorDescriptor::temperature(SensorScale::IDENTITY);
        let observation = match temperature_c {
            Some(value) => SensorMeasurementObservation::available(
                descriptor,
                SensorMagnitude::Decimal(value),
                1_000,
            )
            .expect("valid thermal-zone fixture"),
            None => {
                SensorMeasurementObservation::unavailable(descriptor, FailureKind::PermissionDenied)
            }
        };
        SensorReading::from_measurement_observation(
            "thermal:x86_pkg_temp".into(),
            id.into(),
            label.into(),
            observation,
        )
        .with_device_generation(DeviceGeneration::new(2))
    }

    fn fan_reading() -> SensorReading {
        SensorReading::from_measurement_observation(
            "hwmon:cpu".into(),
            "fan1".into(),
            "cpu_fan".into(),
            SensorMeasurementObservation::available(
                SensorDescriptor::fan_speed(SensorScale::IDENTITY),
                SensorMagnitude::Unsigned(2_400),
                1_000,
            )
            .expect("valid fan fixture"),
        )
    }

    /// `power.thermal-zones` delivery: the thermal surface traverses the shared
    /// sensor center and names every temperature reading by its own source
    /// label. A sibling fan channel stays out of the thermal rows, and a zone
    /// whose read failed keeps its named row with the shared dash — never a
    /// fabricated `0.0 °C`.
    #[test]
    fn thermal_zone_rows_traverse_every_temperature_reading_and_name_its_source() {
        let sensors = SensorCenterSnapshot {
            readings: vec![
                zone_reading("thermal:acpitz:zone:0:temperature", "acpitz", Some(54.5)),
                zone_reading(
                    "thermal:x86_pkg_temp:zone:0:temperature",
                    "x86_pkg_temp",
                    Some(71.0),
                ),
                fan_reading(),
                zone_reading(
                    "thermal:acpitz:zone:1:temperature",
                    "thermal_zone_unreadable",
                    None,
                ),
            ],
            ..Default::default()
        };

        let rows = projection::thermal_zone_rows(&sensors);
        assert_eq!(
            rows.len(),
            3,
            "one row per temperature reading; the fan channel must not leak in"
        );
        let labels: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(
            labels,
            ["acpitz", "x86_pkg_temp", "thermal_zone_unreadable"],
            "each row names the reading's own source label, in projection order"
        );
        assert_eq!(rows[0].value, "54.5 °C");
        assert!(rows[0].present);
        assert_eq!(rows[1].value, "71.0 °C");
        assert!(rows[1].present);
        assert_eq!(rows[2].value, missing_value());
        assert!(
            !rows[2].present,
            "an unread thermal zone must not fabricate a value"
        );
        assert!(
            rows.iter().all(|row| !row.value.contains("0.0 °C")),
            "a failed read must stay a typed absence: {rows:?}"
        );
    }

    /// The panel is the surface that paints the folded rows: it appears for a
    /// projection with temperature channels and stays absent for a snapshot
    /// whose only channel is a fan. The seeded shared projection is the same
    /// one the health modal renders.
    #[test]
    fn thermal_zone_panel_paints_the_observed_zone_and_skips_a_fan_only_snapshot() {
        let mut app = crate::IcedApp::demo();
        let sensors = SensorCenterSnapshot {
            readings: vec![
                zone_reading(
                    "thermal:x86_pkg_temp:zone:0:temperature",
                    "x86_pkg_temp",
                    Some(71.0),
                ),
                fan_reading(),
            ],
            ..Default::default()
        };
        taskmanager_shell::fixture::seed_projection_fact(
            &mut app.shell,
            taskmanager_shell::fixture::ProjectionSeedFact::Sensors(Some(sensors)),
        );
        let projected = app
            .shell
            .projection()
            .sensors
            .as_ref()
            .expect("seeded sensor projection");
        assert!(
            thermal_zone_sensor_panel(projected, app.theme()).is_some(),
            "a temperature channel must produce the thermal-zone panel"
        );
        let _view = render(&app);

        let fan_only = SensorCenterSnapshot {
            readings: vec![fan_reading()],
            ..Default::default()
        };
        assert!(
            thermal_zone_sensor_panel(&fan_only, app.theme()).is_none(),
            "a snapshot without a temperature channel has no thermal-zone surface"
        );
    }

    /// The capture fixture (the projection the evidence run renders) seeds a
    /// named readable thermal zone, so a health-modal capture paints a real
    /// source instead of an unnamed placeholder.
    #[test]
    fn capture_fixture_seeds_a_named_thermal_zone() {
        let app = crate::IcedApp::demo_for_capture();
        let sensors = app
            .shell
            .projection()
            .sensors
            .as_ref()
            .expect("capture sensor projection");
        let rows = projection::thermal_zone_rows(sensors);
        assert!(
            rows.iter().any(|row| row.label == "Package" && row.present),
            "the capture fixture must seed a named readable thermal zone: {rows:?}"
        );
    }

    /// The evidence frame must show the traversal itself: more than one
    /// readable zone, each named by its own source, plus an unreadable zone
    /// that keeps its name and the shared dash. A single-zone fixture could
    /// not prove the per-reading rows in the pixels.
    #[test]
    fn capture_fixture_traverses_every_zone_and_keeps_the_unreadable_one_as_a_dash() {
        let app = crate::IcedApp::demo_for_capture();
        let sensors = app
            .shell
            .projection()
            .sensors
            .as_ref()
            .expect("capture sensor projection");
        let rows = projection::thermal_zone_rows(sensors);

        let readable: Vec<&str> = rows
            .iter()
            .filter(|row| row.present)
            .map(|row| row.label.as_str())
            .collect();
        assert!(
            readable.len() >= 2 && readable.contains(&"Package") && readable.contains(&"acpitz"),
            "the capture fixture must seed more than one readable named zone: {rows:?}"
        );

        let unreadable = rows
            .iter()
            .find(|row| !row.present)
            .expect("the capture fixture keeps an unreadable zone in the traversal");
        assert_eq!(unreadable.label, "nvme");
        assert_eq!(unreadable.value, missing_value());
        assert!(
            rows.iter().all(|row| !row.value.contains("0.0 °C")),
            "an unreadable zone must never fabricate a temperature: {rows:?}"
        );
    }
}
