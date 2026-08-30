use super::*;
use crate::app::{Message, SettingsChange};
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::identity::DeviceGeneration;
use taskmanager_core::core::metrics::{
    CpuMetrics, CpuScalarObservations, GpuMetrics, GpuScalarObservations, MemoryMetrics,
    MemoryScalarObservations, NetworkAdapterType, ScalarObservation, ScalarObservationGroup,
};
use taskmanager_core::core::power::{BatteryInfo, BatteryScalarObservations};
use taskmanager_core::core::sensors::{
    SensorDescriptor, SensorMagnitude, SensorMeasurementObservation, SensorQuantity, SensorScale,
};

fn rail_key(app: &crate::IcedApp, theme: &Theme) -> u64 {
    let devices = [PerfDevice::Cpu];
    let window = VirtualWindow::for_rows(1, 0.0, RAIL_CARD_HEIGHT, RAIL_CARD_HEIGHT, 0.0);
    rail_widget_key(app, theme, &devices, PerfDevice::Cpu, window)
}

fn pin_english() {
    // The shared catalog auto-detects the host locale on first use; pin
    // English so the label assertions are deterministic.
    set_language(Language::En);
}

#[test]
fn rail_lazy_key_tracks_unit_preferences() {
    let mut app = crate::IcedApp::demo();

    let before = rail_key(&app, app.theme());
    let _ = app.update(Message::SettingsChanged(SettingsChange::MemoryBytes(
        !app.memory_use_bytes(),
    )));
    assert_ne!(before, rail_key(&app, app.theme()));

    let before = rail_key(&app, app.theme());
    let _ = app.update(Message::SettingsChanged(SettingsChange::MemoryBase2(
        !app.memory_use_base2(),
    )));
    assert_ne!(before, rail_key(&app, app.theme()));

    let before = rail_key(&app, app.theme());
    let _ = app.update(Message::SettingsChanged(SettingsChange::DriveBytes(
        !app.drive_use_bytes(),
    )));
    assert_ne!(before, rail_key(&app, app.theme()));

    let before = rail_key(&app, app.theme());
    let _ = app.update(Message::SettingsChanged(SettingsChange::DriveBase2(
        !app.drive_use_base2(),
    )));
    assert_ne!(before, rail_key(&app, app.theme()));

    let before = rail_key(&app, app.theme());
    let _ = app.update(Message::SettingsChanged(SettingsChange::NetworkBytes(
        !app.network_use_bytes(),
    )));
    assert_ne!(before, rail_key(&app, app.theme()));

    let before = rail_key(&app, app.theme());
    let _ = app.update(Message::SettingsChanged(SettingsChange::NetworkBase2(
        !app.network_use_base2(),
    )));
    assert_ne!(before, rail_key(&app, app.theme()));
}

#[test]
fn rail_lazy_key_tracks_font_preferences() {
    let app = crate::IcedApp::demo();
    let before = rail_key(&app, app.theme());
    let mut changed = *app.theme();
    changed.ui_font = "Test UI Font";
    assert_ne!(before, rail_key(&app, &changed));

    let mut changed = *app.theme();
    changed.mono_font = "Test Mono Font";
    assert_ne!(before, rail_key(&app, &changed));
}

#[test]
fn cpu_caption_reads_typed_usage_temperature_and_core_range() {
    pin_english();
    let unavailable = CpuMetrics::default();
    let (cap1, cap2) = cpu_rail_caption(&unavailable);
    assert_eq!(cap1, "—");
    assert!(cap2.is_empty(), "no core readings → no core line");

    let mut measured = CpuMetrics::from_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(42.0, 1),
        temperature_c: ScalarObservation::available(55.0, 1),
        per_core_temperature_group: ScalarObservationGroup::available(vec![45.0, 47.0], 1),
        ..Default::default()
    });
    measured.brand = Some("Test CPU".into());
    let (cap1, cap2) = cpu_rail_caption(&measured);
    assert_eq!(cap1, "42% · 55 \u{b0}C");
    assert_eq!(cap2, "cores 45..47 \u{b0}C");
    assert_eq!(cpu_rail_heading(&measured), "CPU");
    assert_eq!(cpu_rail_heading(&unavailable), "CPU");
    assert_eq!(cpu_rail_subtitle(&measured), "Test CPU");
    assert_eq!(cpu_rail_subtitle(&unavailable), "");

    let collapsed = CpuMetrics::from_observations(CpuScalarObservations {
        per_core_temperature_group: ScalarObservationGroup::available(vec![46.0, 46.2], 1),
        ..measured.scalar_observations().clone()
    });
    let (_, cap2) = cpu_rail_caption(&collapsed);
    assert_eq!(cap2, "cores 46 \u{b0}C");
}

#[test]
fn mem_caption_formats_used_total_and_percentage() {
    let memory = MemoryMetrics::from_observations(
        MemoryScalarObservations {
            used_bytes: ScalarObservation::available(4 << 30, 1),
            total_bytes: ScalarObservation::available(16 << 30, 1),
            ..Default::default()
        },
        Default::default(),
    );
    let (cap1, cap2) = mem_rail_caption(&memory, UnitPrefs::default());
    assert_eq!(cap1, "4.0 GiB / 16.0 GiB");
    assert_eq!(cap2, "25%");
}

#[test]
fn disk_caption_combines_active_rate_type_temperature_and_badge() {
    pin_english();
    let disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .device_id("disk:test:nvme0".into())
        .name("/dev/nvme0n1".into())
        .model("ZHITAI TiPro9000 2TB".into())
        .disk_type("NVMe SSD".into())
        .current_read_bytes_per_sec(100 << 20)
        .current_write_bytes_per_sec(24 << 20)
        .current_active_time_pct(12.4)
        .smart_temperature_c(Some(41.0))
        .device_state(taskmanager_core::core::device_state::DeviceState {
            status: DeviceStatus::Healthy,
            ..Default::default()
        })
        .smart_availability(taskmanager_core::core::metrics::SmartAvailability::Available)
        .smart_state(taskmanager_core::core::device_state::DeviceState {
            status: DeviceStatus::Healthy,
            ..Default::default()
        })
        .build();
    assert_eq!(disk_rail_heading(&disk), "Drive (nvme0n1)");
    assert_eq!(disk_rail_subtitle(&disk), "ZHITAI TiPro9000 2TB");
    let (cap1, cap2) = disk_rail_caption(&disk, UnitPrefs::default());
    assert_eq!(cap1, "12% · 124.0 MiB/s");
    assert_eq!(cap2, "nvme0n1 · NVMe SSD · 41 \u{b0}C");

    let degraded = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .device_id("disk:test:sda".into())
        .name("sda".into())
        .smart_critical_warning(Some(true))
        .build();
    assert_eq!(disk_rail_heading(&degraded), "Drive (sda)");
    assert_eq!(disk_rail_subtitle(&degraded), "");
    let (cap1, cap2) = disk_rail_caption(&degraded, UnitPrefs::default());
    assert_eq!(cap1, "— · —");
    assert!(cap2.starts_with("sda"), "empty type omitted: {cap2}");
    assert!(cap2.contains(" · "), "degraded badge appended: {cap2}");
}

#[test]
fn nic_caption_reads_wireless_association_and_wired_link() {
    pin_english();
    let wifi = taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
        .device_id("net:test:wlan0".into())
        .interface_name("wlan0".into())
        .adapter_type(NetworkAdapterType::WiFi)
        .ssid_observation(match Some("office".into()) {
            Some(value) => taskmanager_core::core::metrics::OptionalObservation::present(value, 1),
            None => taskmanager_core::core::metrics::OptionalObservation::default(),
        })
        .signal_observation(match Some(-50) {
            Some(value) => taskmanager_core::core::metrics::OptionalObservation::present(value, 1),
            None => taskmanager_core::core::metrics::OptionalObservation::default(),
        })
        .link_speed_observation(match Some(866) {
            Some(value) => taskmanager_core::core::metrics::ScalarObservation::available(value, 1),
            None => taskmanager_core::core::metrics::ScalarObservation::default(),
        })
        .device_state(taskmanager_core::core::device_state::DeviceState {
            status: DeviceStatus::Healthy,
            ..Default::default()
        })
        .build();
    assert_eq!(nic_rail_heading(&wifi), "Wireless (wlan0)");
    assert_eq!(nic_rail_subtitle(&wifi), "");
    let (cap1, cap2) = nic_rail_caption(&wifi, UnitPrefs::default());
    assert!(cap1.starts_with("S: "), "send label leads: {cap1}");
    assert!(cap1.contains(" R: "), "recv label follows: {cap1}");
    assert_eq!(cap2, "office · 67% · 866 Mbps");

    let wired = taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
        .interface_name("enp3s0".into())
        .adapter_type(NetworkAdapterType::Ethernet)
        .link_speed_observation(match Some(1000) {
            Some(value) => taskmanager_core::core::metrics::ScalarObservation::available(value, 1),
            None => taskmanager_core::core::metrics::ScalarObservation::default(),
        })
        .device_state(taskmanager_core::core::device_state::DeviceState {
            status: DeviceStatus::Healthy,
            ..Default::default()
        })
        .build();
    assert_eq!(nic_rail_heading(&wired), "Wired (enp3s0)");
    assert_eq!(nic_rail_subtitle(&wired), "");
    let (_, cap2) = nic_rail_caption(&wired, UnitPrefs::default());
    assert_eq!(cap2, "enp3s0 · 1000 Mbps");
}

#[test]
fn gpu_caption_follows_dedicated_vram_truth_and_clock_fallback() {
    let with_vram = GpuMetrics::from_observations(GpuScalarObservations {
        dedicated_vram_used_bytes: ScalarObservation::available(2 << 30, 1),
        dedicated_vram_total_bytes: ScalarObservation::available(8 << 30, 1),
        utilization_pct: ScalarObservation::available(12.0, 1),
        temperature_c: ScalarObservation::available(41.0, 1),
        power_w: ScalarObservation::available(30.0, 1),
        ..Default::default()
    });
    let (cap1, cap2) = gpu_rail_caption(&with_vram, UnitPrefs::default());
    assert_eq!(cap1, "VRAM 2.0 GiB / 8.0 GiB");
    assert_eq!(cap2, "12% · VRAM 25% · 41 \u{b0}C · 30 W");

    let igpu = GpuMetrics::from_observations(GpuScalarObservations {
        frequency_mhz: ScalarObservation::available(1300, 1),
        max_frequency_mhz: ScalarObservation::available(2500, 1),
        ..Default::default()
    });
    let (cap1, cap2) = gpu_rail_caption(&igpu, UnitPrefs::default());
    assert_eq!(cap1, "1300 / 2500 MHz");
    assert_eq!(cap2, "—");
    assert_eq!(gpu_rail_heading(&igpu, 0), "GPU 0");
    assert_eq!(gpu_rail_subtitle(&igpu), "");
    let mut branded = GpuMetrics::new("", "Intel Arc Graphics");
    branded.marketing_name = Some("Arc B390".into());
    assert_eq!(gpu_rail_subtitle(&branded), "Arc B390");
}

#[test]
fn battery_and_fan_captions_read_capacity_rpm_and_badges() {
    pin_english();
    let mut battery = BatteryInfo::new(
        "BAT0",
        DeviceState {
            status: DeviceStatus::Healthy,
            last_success_ms: Some(1_000),
        },
    );
    battery.model_name = "DELL 1P6KD".into();
    battery.status = "Discharging".into();
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(85, 1_000),
        ..Default::default()
    });
    assert_eq!(battery_rail_heading(&battery, 0), "Battery 0");
    assert_eq!(battery_rail_subtitle(&battery), "DELL 1P6KD");
    assert_eq!(
        battery_rail_caption(&battery),
        ("85%".to_string(), "Discharging".to_string())
    );

    let fan = SensorReading::from_measurement_observation(
        "hwmon0".into(),
        "fan1".into(),
        "CPU Fan".into(),
        SensorMeasurementObservation::available(
            SensorDescriptor::fan_speed(SensorScale::IDENTITY),
            SensorMagnitude::Unsigned(1200),
            1,
        )
        .expect("valid fan fixture"),
    )
    .with_device_generation(DeviceGeneration::new(1));
    assert_eq!(fan_rail_heading(0), "Fan 0");
    assert_eq!(
        fan_rail_caption(&fan),
        ("1200 RPM".to_string(), "CPU Fan".to_string())
    );
}

#[test]
fn rail_rows_project_every_visible_device_from_its_own_window() {
    pin_english();
    let mut shell = taskmanager_shell::demo_app();
    let snapshot = shell
        .projection()
        .snapshot
        .clone()
        .expect("demo snapshot fixture");
    for _ in 0..3 {
        taskmanager_shell::fixture::record_demo_history_frame(&mut shell, &snapshot, None, None);
    }
    let history = &shell.history;
    let inputs = RailInputs {
        snapshot: Some(&snapshot),
        power: shell.projection().power_supplies.as_ref(),
        sensors: shell.projection().sensors.as_ref(),
        shell: &shell,
        device_samples: None,
        cpu_samples: std::rc::Rc::from(
            taskmanager_shell::presentation::trend::cpu_usage_percent(history).into_boxed_slice(),
        ),
        memory_samples: std::rc::Rc::from(
            taskmanager_shell::presentation::trend::memory_usage_percent(history)
                .into_boxed_slice(),
        ),
        memory_units: UnitPrefs::default(),
        drive_units: UnitPrefs::default(),
        network_units: UnitPrefs::default(),
    };
    let mut devices = vec![
        PerfDevice::Cpu,
        PerfDevice::Memory,
        PerfDevice::Disk(0),
        PerfDevice::Network(0),
        PerfDevice::Gpu(0),
    ];
    if let Some(power) = inputs.power {
        devices.extend((0..power.batteries.len()).map(PerfDevice::Battery));
    }
    if let Some(sensors) = inputs.sensors {
        let fan_count = sensors
            .readings
            .iter()
            .filter(|reading| reading.quantity() == &SensorQuantity::FanSpeed)
            .count();
        devices.extend((0..fan_count).map(PerfDevice::Fan));
    }
    let rows = rail_rows(&devices, &inputs);
    assert_eq!(
        rows.len(),
        devices.len(),
        "every device in the list projects a rail row"
    );
    for (row, device) in rows.iter().zip(&devices) {
        assert_eq!(row.device, *device);
        assert!(
            !row.heading.trim().is_empty(),
            "identity heading for {device:?}"
        );
    }
    assert_eq!(
        &*rows[4].samples,
        &[18.0, 18.0, 18.0, 18.0],
        "the demo seed and three added frames share one bounded authority"
    );
    assert_eq!(rows[0].max, 100.0);
    for row in &rows {
        match row.category {
            RailCategory::Disk | RailCategory::Network | RailCategory::Fan => {
                assert_ne!(row.max, 100.0, "magnitude series must auto-scale");
            }
            _ => assert_eq!(row.max, 100.0),
        }
    }
}

#[test]
fn rail_tooltip_lines_report_current_average_and_peak_in_unit_families() {
    pin_english();
    let percent = RailRow {
        device: PerfDevice::Cpu,
        heading: "CPU".into(),
        subtitle: String::new(),
        cap1: String::new(),
        cap2: String::new(),
        samples: Rc::from([10.0_f32, 20.0, 30.0].as_slice()),
        max: 100.0,
        category: RailCategory::Cpu,
        value_format: RailValueFormat::Percent,
    };
    assert_eq!(
        rail_tooltip_lines(&percent),
        ["Current: 30%", "Average: 20%", "Peak: 30%"]
    );

    let bytes = RailRow {
        device: PerfDevice::Disk(0),
        heading: "Drive (nvme0n1)".into(),
        subtitle: "ZHITAI TiPro9000 2TB".into(),
        cap1: String::new(),
        cap2: String::new(),
        samples: Rc::from([0.0_f32, 1_048_576.0, 2_097_152.0].as_slice()),
        max: 2_097_152.0,
        category: RailCategory::Disk,
        value_format: RailValueFormat::BytesPerSec,
    };
    let lines = rail_tooltip_lines(&bytes);
    assert_eq!(lines[0], "Current: 2.0 MiB/s");
    assert_eq!(lines[1], "Average: 1.0 MiB/s");
    assert_eq!(lines[2], "Peak: 2.0 MiB/s");

    let rpm = RailRow {
        device: PerfDevice::Fan(0),
        heading: "Fan 0".into(),
        subtitle: "CPU Fan".into(),
        cap1: String::new(),
        cap2: String::new(),
        samples: Rc::from([1000.0_f32, 1200.0].as_slice()),
        max: 1200.0,
        category: RailCategory::Fan,
        value_format: RailValueFormat::Rpm,
    };
    assert_eq!(
        rail_tooltip_lines(&rpm),
        ["Current: 1200 RPM", "Average: 1100 RPM", "Peak: 1200 RPM"]
    );

    let empty = RailRow {
        samples: Rc::from(Vec::<f32>::new().into_boxed_slice()),
        ..percent
    };
    assert!(rail_tooltip_lines(&empty).is_empty());
}

#[test]
fn rail_spark_fingerprint_tracks_snapshot_generation_and_scale() {
    let samples: Rc<[f32]> = Rc::from([10.0, 20.0, 30.0].as_slice());
    let base = RailSpark {
        samples: Rc::clone(&samples),
        color: Color::WHITE,
        max: 100.0,
    }
    .fingerprint();
    assert_eq!(
        base,
        RailSpark {
            samples: Rc::clone(&samples),
            color: Color::BLACK,
            max: 100.0,
        }
        .fingerprint(),
        "same generation and scale reuse geometry regardless of color"
    );
    let shifted: Rc<[f32]> = Rc::from([99.0, 20.0, 30.0].as_slice());
    assert_ne!(
        base,
        RailSpark {
            samples: shifted,
            color: Color::WHITE,
            max: 100.0,
        }
        .fingerprint(),
        "same len/tail with changed history must invalidate"
    );
    assert_ne!(
        base,
        RailSpark {
            samples,
            color: Color::WHITE,
            max: 200.0,
        }
        .fingerprint()
    );
}
