use super::*;
use taskmanager_application::i18n::Language::En;
use taskmanager_application::i18n::set_language;
use taskmanager_core::core::metrics::CpuFrequencySource;
use taskmanager_shell::demo_app;
use taskmanager_shell::fixture::ProjectionSeedFact;
use taskmanager_shell::fixture::seed_projection_fact;

#[test]
fn health_cpu_line_relabels_bogomips_instead_of_faking_mhz() {
    set_language(En);
    let shell = demo_app();
    let snapshot = shell.projection().snapshot.as_ref().expect("demo snapshot");
    let mut bogomips_only = snapshot.clone();
    bogomips_only.cpu.frequency_source = CpuFrequencySource::BogoMips;

    let rows = health_rows(&bogomips_only, Language::En);
    // A BogoMIPS-only host must read the BogoMIPS readout, never "MHz".
    let cpu = rows.iter().find(|row| row.label == "CPU").expect("CPU row");
    assert!(cpu.value.contains("BogoMIPS"));
    assert!(!cpu.value.contains("MHz"));

    // The untouched native fixture stays an MHz clock.
    let native = health_rows(snapshot, Language::En);
    let native_cpu = native
        .iter()
        .find(|row| row.label == "CPU")
        .expect("native CPU row");
    assert!(native_cpu.value.contains("MHz"));
}

#[test]
fn health_rows_cover_every_domain_with_fixture_values() {
    set_language(En);
    let shell = demo_app();
    let snapshot = shell.projection().snapshot.as_ref().expect("demo snapshot");
    let rows = health_rows(snapshot, Language::En);

    assert_eq!(rows.len(), 8);
    let row = |label: &str| rows.iter().find(|row| row.label == label).expect(label);
    assert!(row("CPU").healthy);
    assert!(row("CPU").value.contains("37.4%"));
    assert!(row("Memory").healthy);
    assert!(row("Memory").value.contains("GiB"));
    assert!(row("Network").healthy);
    assert!(row("Network").value.contains("wlan0"));
    assert!(row("GPU").healthy);
    assert!(row("GPU").value.contains("Intel Graphics (xe)"));
    assert!(row("System").healthy);
    assert!(row("System").value.contains("347 processes"));
}

#[test]
fn health_rows_stay_honest_when_domains_are_absent() {
    let snapshot = SystemSnapshot::default();
    let rows = health_rows(&snapshot, Language::En);
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
    seed_projection_fact(&mut app.shell, ProjectionSeedFact::Snapshot(Box::new(None)));
    let _view = render(&app);
}

/// The device-summary labels and composed values resolve through the active
/// locale: the shared domain terms come from the shared catalog, the count
/// templates and the System row from the iced dictionary. These assert the
/// resolved rows (not the source key literals) in both tongues.
#[test]
fn health_rows_resolve_labels_and_values_in_both_locales() {
    let shell = demo_app();
    let snapshot = shell.projection().snapshot.as_ref().expect("demo snapshot");

    let labels = |language: Language| {
        health_rows(snapshot, language)
            .into_iter()
            .map(|row| row.label)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        labels(Language::En),
        [
            "Health", "CPU", "Memory", "Swap", "Disk", "Network", "GPU", "System"
        ]
    );
    assert_eq!(
        labels(Language::Zh),
        [
            "健康度",
            "CPU",
            "内存",
            "交换空间",
            "磁盘",
            "网络",
            "GPU",
            "系统"
        ]
    );

    let value = |language: Language, label: &str| {
        health_rows(snapshot, language)
            .into_iter()
            .find(|row| row.label == label)
            .expect(label)
            .value
    };
    assert_eq!(value(Language::En, "Disk"), "1 device · TiPro9000 2TB");
    assert_eq!(value(Language::En, "Network"), "1 interface · wlan0");
    assert_eq!(value(Language::En, "GPU"), "1 GPU · Intel Graphics (xe)");
    assert_eq!(value(Language::Zh, "磁盘"), "1 个设备 · TiPro9000 2TB");
    assert_eq!(value(Language::Zh, "网络"), "1 个接口 · wlan0");
    assert_eq!(value(Language::Zh, "GPU"), "1 个 GPU · Intel Graphics (xe)");

    let en_system = value(Language::En, "System");
    assert!(en_system.starts_with("uptime "), "{en_system}");
    assert!(en_system.contains(" · 347 processes · "), "{en_system}");
    assert!(en_system.ends_with(" threads"), "{en_system}");
    let zh_system = value(Language::Zh, "系统");
    assert!(zh_system.starts_with("运行时间 "), "{zh_system}");
    assert!(zh_system.contains(" · 347 个进程 · "), "{zh_system}");
    assert!(zh_system.ends_with(" 个线程"), "{zh_system}");
}

/// The modal chrome (subtitle), status cells, thermal badge tags, and the two
/// sensor-panel headings resolve in both locales. The status cells reuse the
/// shared verdict/absence vocabulary the TUI health overlay already renders.
#[test]
fn health_modal_chrome_resolves_in_both_locales() {
    for (key, en, zh) in [
        (
            Key::HealthObservedHint,
            "Observed samples only · Esc closes",
            "仅显示已观测样本 · Esc 关闭",
        ),
        (
            Key::HealthThermalHeatmap,
            "Thermal Heatmap & Sensors",
            "温度热图与传感器",
        ),
        (
            Key::HealthSensorsThermal,
            "Sensors & Thermal Health",
            "传感器与温度健康",
        ),
        (Key::HealthVerdictOk, "ok", "正常"),
        (Key::HealthUnavailable, "Unavailable", "不可用"),
        (Key::HealthThermalCool, "Cool", "凉爽"),
        (Key::HealthThermalNormal, "Normal", "正常"),
        (Key::HealthThermalWarm, "Warm", "偏热"),
        (Key::HealthThermalHot, "Hot", "过热"),
    ] {
        assert_eq!(i18n::t(Language::En, key), en, "{key:?} en");
        assert_eq!(i18n::t(Language::Zh, key), zh, "{key:?} zh");
    }
}

/// One expectation per shared product term the iced summary renders: the iced
/// key, the shared catalog code it mirrors, and the string both dictionaries
/// must resolve in each locale. `health_dictionary_resolves_the_shared_terms`
/// checks the iced side and `health_shared_catalog_carries_the_same_terms`
/// checks the shared side against the same table, so either dictionary
/// drifting fails one of the two.
const HEALTH_SHARED_TERMS: [(Key, &str, &str, &str); 10] = [
    (Key::Cpu, "common.cpu", "CPU", "CPU"),
    (Key::Memory, "common.memory", "Memory", "内存"),
    (Key::Swap, "mem.swap", "Swap", "交换空间"),
    (Key::Disk, "common.disk", "Disk", "磁盘"),
    (Key::Network, "common.network", "Network", "网络"),
    (Key::Gpu, "common.gpu", "GPU", "GPU"),
    (Key::SystemDomain, "system.title", "System", "系统"),
    (Key::HealthScore, "system.health_score", "Health", "健康度"),
    (
        Key::HealthUnavailable,
        "health.unavailable",
        "Unavailable",
        "不可用",
    ),
    (Key::HealthVerdictOk, "health.verdict_ok", "ok", "正常"),
];

/// The iced side of [`HEALTH_SHARED_TERMS`]: every shared product term this
/// frontend renders names the catalog key it mirrors and resolves to the term
/// both dictionaries agree on.
#[test]
fn health_dictionary_resolves_the_shared_terms() {
    for (key, code, en, zh) in HEALTH_SHARED_TERMS {
        assert_eq!(key.code(), code, "{key:?} must name the shared catalog key");
        assert_eq!(i18n::t(Language::En, key), en, "{key:?} en");
        assert_eq!(i18n::t(Language::Zh, key), zh, "{key:?} zh");
    }
}

/// The shared-catalog side of [`HEALTH_SHARED_TERMS`]. The explicit imports
/// shadow this test tree's glob-imported iced `Language`/`t` for the body of
/// this function only, so the two dictionaries never share one scope — which
/// is what would otherwise force an alias (forbidden) or a qualified path
/// (also forbidden).
#[test]
fn health_shared_catalog_carries_the_same_terms() {
    use taskmanager_application::i18n::{Language, current_language, set_language, t};

    let prior = current_language();
    set_language(Language::En);
    for (_, code, en, _) in HEALTH_SHARED_TERMS {
        assert_eq!(t(code), en, "{code} en");
    }
    set_language(Language::Zh);
    for (_, code, _, zh) in HEALTH_SHARED_TERMS {
        assert_eq!(t(code), zh, "{code} zh");
    }
    set_language(prior);
}

/// The synthesized CPU/GPU thermal-pill labels resolve through the active
/// locale too: the CPU package sensor and the branded/unbranded GPU labels are
/// named in the same tongue as the rest of the panel.
#[test]
fn thermal_reading_labels_resolve_in_both_locales() {
    use taskmanager_core::core::metrics::{
        CpuMetrics, CpuScalarObservations, GpuMetrics, GpuScalarObservations, ScalarObservation,
    };

    let mut branded = GpuMetrics::new("", "Test GPU");
    branded.apply_scalar_observations(GpuScalarObservations {
        temperature_c: ScalarObservation::available(82.0, 1),
        ..Default::default()
    });
    let mut unbranded = GpuMetrics::new("", "");
    unbranded.apply_scalar_observations(GpuScalarObservations {
        temperature_c: ScalarObservation::available(50.0, 1),
        ..Default::default()
    });
    let snapshot = SystemSnapshot {
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            temperature_c: ScalarObservation::available(65.0, 1),
            ..Default::default()
        }),
        gpu: vec![branded, unbranded],
        ..Default::default()
    };

    let labels = |language: Language| {
        projection::thermal_readings(&snapshot, language)
            .into_iter()
            .map(|(label, _)| label)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        labels(Language::En),
        ["CPU Package", "GPU 0 (Test GPU)", "GPU 1"]
    );
    assert_eq!(
        labels(Language::Zh),
        ["CPU 封装", "GPU 0 (Test GPU)", "GPU 1"]
    );
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
        seed_projection_fact(&mut app.shell, ProjectionSeedFact::Sensors(Some(sensors)));
        let projected = app
            .shell
            .projection()
            .sensors
            .as_ref()
            .expect("seeded sensor projection");
        assert!(
            thermal_zone_sensor_panel(projected, app.theme(), Language::En).is_some(),
            "a temperature channel must produce the thermal-zone panel"
        );
        let _view = render(&app);

        let fan_only = SensorCenterSnapshot {
            readings: vec![fan_reading()],
            ..Default::default()
        };
        assert!(
            thermal_zone_sensor_panel(&fan_only, app.theme(), Language::En).is_none(),
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

    /// The zone heading is a localized system surface label, not the shared
    /// device-temperature quantity word: the iced dictionary resolves the
    /// `common.thermal_zones` key, and the shared catalog side of the same
    /// expectation is asserted in
    /// [`thermal_zone_surface_key_differs_from_the_quantity`].
    #[test]
    fn thermal_zone_heading_resolves_the_shared_zone_key() {
        assert_eq!(
            Key::ThermalZones.code(),
            "common.thermal_zones",
            "the iced key must spell the shared catalog key"
        );
        assert_eq!(i18n::t(Language::En, Key::ThermalZones), "Thermal zones");
        assert_eq!(i18n::t(Language::Zh, Key::ThermalZones), "温度区");
    }
}

/// The shared-catalog side of the zone-heading expectation: the surface key and
/// the device-temperature quantity word are distinct terms in both locales.
/// The explicit import shadows this tree's glob-imported iced `t` for the body
/// of this function only (see `health_shared_catalog_carries_the_same_terms`).
#[test]
fn thermal_zone_surface_key_differs_from_the_quantity() {
    use taskmanager_application::i18n::{Language, current_language, set_language, t};

    let prior = current_language();
    set_language(Language::En);
    let en_surface = t("common.thermal_zones");
    let en_quantity = t("common.temperature");
    set_language(Language::Zh);
    let zh_surface = t("common.thermal_zones");
    let zh_quantity = t("common.temperature");
    set_language(prior);

    assert_eq!(en_surface, "Thermal zones");
    assert_eq!(zh_surface, "温度区");
    assert_ne!(
        en_surface, en_quantity,
        "the zone heading must not reuse the device temperature quantity word"
    );
    assert_ne!(
        zh_surface, zh_quantity,
        "the zone heading must not reuse the device temperature quantity word"
    );
}
