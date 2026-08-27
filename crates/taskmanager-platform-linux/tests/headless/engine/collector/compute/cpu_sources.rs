use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct FixtureDir(PathBuf);

impl FixtureDir {
    fn new() -> Self {
        let path = crate::test_support::repo_temp_dir().join(format!(
            "taskmanager-cpu-provenance-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create CPU provenance fixture");
        Self(path)
    }
}

impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn status_does_not_infer_failure_from_a_zero_measurement() {
    let fixture = FixtureDir::new();
    let cpufreq = fixture.0.join("cpu0").join("cpufreq");
    fs::create_dir_all(&cpufreq).expect("create cpufreq fixture");
    fs::write(cpufreq.join("scaling_cur_freq"), "0\n").expect("write zero frequency");

    let observation = observe_cpufreq_at(&fixture.0, 1);

    assert_eq!(observation.current_mhz, Some(0));
    assert_eq!(observation.per_core_mhz, [Some(0)]);
    assert_eq!(observation.status.outcome, SourceOutcome::Available);
    assert!(observation.status.item_count >= 1);
}

#[test]
fn missing_per_core_frequency_preserves_logical_index_as_none() {
    let fixture = FixtureDir::new();
    let cpufreq = fixture.0.join("cpu0").join("cpufreq");
    fs::create_dir_all(&cpufreq).expect("create cpufreq fixture");
    fs::write(cpufreq.join("scaling_cur_freq"), "3200000\n").expect("write boot CPU frequency");

    let observation = observe_cpufreq_at(&fixture.0, 2);

    assert_eq!(observation.current_mhz, Some(3_200));
    assert_eq!(observation.per_core_mhz, [Some(3_200), None]);
    assert_eq!(
        observation.status.outcome,
        SourceOutcome::Partial(FailureKind::Unsupported)
    );
}

#[test]
fn missing_source_is_typed_unsupported_not_successful_zero() {
    let mut failures = FailureSummary::default();
    failures.record(FailureKind::Unsupported);
    let status = source_status(CPUFREQ_PROVIDER, 0, false, failures);
    assert_eq!(
        status.outcome,
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(status.item_count, 0);
}

#[test]
fn bogomips_parser_accepts_positive_values_and_reports_count() {
    let fixture = FixtureDir::new();
    let cpuinfo = fixture.0.join("cpuinfo");
    fs::write(
        &cpuinfo,
        "processor : 0\nbogomips : 2399.87\nprocessor : 1\nbogomips : 2400.01\n",
    )
    .expect("write bogomips fixture");

    let observation = observe_bogomips_at(&cpuinfo);

    assert_eq!(observation.value, Some(2399.87));
    assert_eq!(observation.status.outcome, SourceOutcome::Available);
    assert_eq!(observation.status.item_count, 2);
}

#[test]
fn bogomips_parser_rejects_zero_and_malformed_values_without_fabricating_speed() {
    let fixture = FixtureDir::new();
    let cpuinfo = fixture.0.join("cpuinfo");
    fs::write(&cpuinfo, "bogomips : 0\nbogomips : not-a-number\n")
        .expect("write invalid bogomips fixture");

    let observation = observe_bogomips_at(&cpuinfo);

    assert_eq!(observation.value, None);
    assert_eq!(
        observation.status.outcome,
        SourceOutcome::Unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(observation.status.item_count, 0);
}

#[test]
fn bogomips_frequency_value_keeps_raw_calibration_scale() {
    assert_eq!(bogomips_to_frequency_value(2399.87), Some(2_400));
    assert_eq!(bogomips_to_frequency_value(0.0), None);
    assert_eq!(bogomips_to_frequency_value(f32::NAN), None);
}

#[test]
fn partial_source_keeps_successful_fields_and_failure() {
    let mut failures = FailureSummary::default();
    failures.record(FailureKind::PermissionDenied);
    let status = source_status(TEMPERATURE_PROVIDER, 2, true, failures);
    assert_eq!(
        status.outcome,
        SourceOutcome::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(status.item_count, 2);
}

#[test]
fn temperature_value_and_status_share_one_hwmon_observation() {
    let fixture = FixtureDir::new();
    // Single logical CPU = physical core 0; topology is required for the new
    // per-logical-core mapping (without it the source degrades to Partial).
    let topology = fixture.0.join("cpu0").join("topology");
    fs::create_dir_all(&topology).expect("create topology dir");
    fs::write(topology.join("core_id"), "0\n").expect("write core_id");
    let hwmon = fixture.0.join("hwmon").join("hwmon0");
    fs::create_dir_all(&hwmon).expect("create hwmon fixture");
    fs::write(hwmon.join("name"), "coretemp\n").expect("write hwmon name");
    fs::write(hwmon.join("temp1_input"), "42000\n").expect("write package temp");
    fs::write(hwmon.join("temp1_label"), "Package id 0\n").expect("write package label");
    fs::write(hwmon.join("temp2_input"), "39000\n").expect("write core temp");
    fs::write(hwmon.join("temp2_label"), "Core 0\n").expect("write core label");

    let observation = observe_temperatures_at(
        &fixture.0.join("thermal"),
        &fixture.0.join("hwmon"),
        &fixture.0,
        1,
    );

    assert_eq!(observation.package_c, Some(42.0));
    assert_eq!(
        observation.package_source,
        CpuTemperatureSource::Coretemp,
        "a coretemp package reading must record its chip tier"
    );
    assert_eq!(
        per_core_temps_for_legacy_field(&observation.per_core_c),
        [39.0]
    );
    assert_eq!(observation.status.outcome, SourceOutcome::Available);
    assert_eq!(observation.status.item_count, 2);
}

#[test]
fn die_temperature_is_not_fabricated_into_per_core_channels() {
    let fixture = FixtureDir::new();
    let hwmon = fixture.0.join("hwmon").join("hwmon0");
    fs::create_dir_all(&hwmon).expect("create k10temp fixture");
    fs::write(hwmon.join("name"), "k10temp\n").expect("write hwmon name");
    fs::write(hwmon.join("temp1_input"), "50000\n").expect("write die temp");
    fs::write(hwmon.join("temp1_label"), "Tdie\n").expect("write die label");

    let observation = observe_temperatures_at(
        &fixture.0.join("thermal"),
        &fixture.0.join("hwmon"),
        &fixture.0,
        8,
    );

    assert_eq!(observation.package_c, Some(50.0));
    assert_eq!(
        observation.package_source,
        CpuTemperatureSource::K10temp,
        "a k10temp die reading must record its chip tier"
    );
    assert!(observation.per_core_c.is_empty());
    assert_eq!(observation.status.outcome, SourceOutcome::Available);
}

#[test]
fn unavailable_temperature_is_none_instead_of_a_fabricated_zero() {
    let fixture = FixtureDir::new();

    let observation = observe_temperatures_at(
        &fixture.0.join("thermal"),
        &fixture.0.join("hwmon"),
        &fixture.0,
        4,
    );

    assert_eq!(observation.package_c, None);
    assert!(observation.per_core_c.is_empty());
    assert_eq!(
        observation.status.outcome,
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    );
}

#[test]
fn smt_topology_maps_physical_core_temp_to_sibling_logical_cores() {
    let fixture = FixtureDir::new();
    // 16 logical / 8 physical SMT layout. cpuN's topology/core_id is N/2, so
    // cpu0+cpu1 share physical core 0, cpu2+cpu3 share physical core 1, ...
    for logical in 0..16u32 {
        let core_id = logical / 2;
        let topology = fixture.0.join(format!("cpu{logical}")).join("topology");
        fs::create_dir_all(&topology).expect("create topology dir");
        fs::write(topology.join("core_id"), format!("{core_id}\n")).expect("write core_id");
    }
    let hwmon = fixture.0.join("hwmon").join("hwmon0");
    fs::create_dir_all(&hwmon).expect("create coretemp fixture");
    fs::write(hwmon.join("name"), "coretemp\n").expect("write hwmon name");
    fs::write(hwmon.join("temp1_input"), "50000\n").expect("write package temp");
    fs::write(hwmon.join("temp1_label"), "Package id 0\n").expect("write package label");
    // Each physical core carries a distinct temp so the mapping is provable.
    for core in 0..8u32 {
        let temp_c = 40 + core;
        let index = core + 2; // temp2..temp9 -> Core 0..Core 7
        fs::write(
            hwmon.join(format!("temp{index}_input")),
            format!("{}\n", temp_c * 1000),
        )
        .expect("write core temp input");
        fs::write(
            hwmon.join(format!("temp{index}_label")),
            format!("Core {core}\n"),
        )
        .expect("write core temp label");
    }

    let observation = observe_temperatures_at(
        &fixture.0.join("thermal"),
        &fixture.0.join("hwmon"),
        &fixture.0,
        16,
    );

    // Padded to the logical CPU count, not the physical core count.
    assert_eq!(observation.per_core_c.len(), 16);
    assert_eq!(observation.package_c, Some(50.0));
    // SMT siblings share the parent physical core's reading: logical 0 and 1
    // both report physical core 0's 40°C, but only logical 0 is the canonical
    // `DirectlyMeasured` index — logical 1 is inherited.
    let logical_0 = observation.per_core_c[0].expect("logical 0 has a reading");
    let logical_1 = observation.per_core_c[1].expect("logical 1 has a reading");
    assert_eq!(logical_0.temperature_c, 40.0);
    assert_eq!(logical_1.temperature_c, 40.0);
    assert_eq!(
        logical_0.provenance,
        TemperatureProvenance::DirectlyMeasured
    );
    assert_eq!(
        logical_1.provenance,
        TemperatureProvenance::PhysicalSiblingShared
    );
    // Every logical index 8..=15 carries its parent physical core's reading.
    for logical in 8..=15usize {
        let expected_physical = (logical / 2) as u32;
        let expected_c = 40.0 + expected_physical as f32;
        let reading = observation.per_core_c[logical].expect("logical core has a reading");
        assert_eq!(reading.temperature_c, expected_c);
    }
    // Topology present and every logical core mapped = Available (the SMT
    // sibling sharing is a per-entry availability distinction, surfaced through
    // ScalarObservation::Partial downstream, not a source-level degradation).
    assert_eq!(observation.status.outcome, SourceOutcome::Available);
}

#[test]
fn missing_topology_degrades_to_short_vec_with_honest_partial_status() {
    let fixture = FixtureDir::new();
    // No topology/core_id files — observe_temperatures_at must detect the
    // missing mapping and fall back to the short physical-core Vec.
    let hwmon = fixture.0.join("hwmon").join("hwmon0");
    fs::create_dir_all(&hwmon).expect("create coretemp fixture");
    fs::write(hwmon.join("name"), "coretemp\n").expect("write hwmon name");
    fs::write(hwmon.join("temp1_input"), "50000\n").expect("write package temp");
    fs::write(hwmon.join("temp1_label"), "Package id 0\n").expect("write package label");
    for core in 0..4u32 {
        let temp_c = 40 + core;
        let index = core + 2;
        fs::write(
            hwmon.join(format!("temp{index}_input")),
            format!("{}\n", temp_c * 1000),
        )
        .expect("write core temp input");
        fs::write(
            hwmon.join(format!("temp{index}_label")),
            format!("Core {core}\n"),
        )
        .expect("write core temp label");
    }

    let observation = observe_temperatures_at(
        &fixture.0.join("thermal"),
        &fixture.0.join("hwmon"),
        &fixture.0,
        8,
    );

    // Topology missing — short Vec of the 4 measured physical cores, NOT
    // padded to logical_cpu_count (8) and NOT fabricated as logical-indexed.
    assert_eq!(observation.per_core_c.len(), 4);
    for (index, reading) in observation.per_core_c.iter().enumerate() {
        let reading = reading.expect("short-Vec entry carries a reading");
        assert_eq!(reading.temperature_c, 40.0 + index as f32);
        assert_eq!(
            reading.provenance,
            TemperatureProvenance::DirectlyMeasured,
            "topology-missing fallback must not fabricate SMT claims"
        );
    }
    // Honest Partial(ProviderFault) — topology unavailable, not faked as
    // Available.
    assert_eq!(
        observation.status.outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
}

#[test]
fn partial_sensor_coverage_keeps_mapped_cores_and_emits_none_for_unmapped() {
    let fixture = FixtureDir::new();
    // 4 logical / 2 physical, but the sensor only exposes Core 0 (Core 1
    // missing). Logical 2 and 3 (physical 1) must be `None`, logical 0 and 1
    // must share physical 0's reading.
    for logical in 0..4u32 {
        let core_id = logical / 2;
        let topology = fixture.0.join(format!("cpu{logical}")).join("topology");
        fs::create_dir_all(&topology).expect("create topology dir");
        fs::write(topology.join("core_id"), format!("{core_id}\n")).expect("write core_id");
    }
    let hwmon = fixture.0.join("hwmon").join("hwmon0");
    fs::create_dir_all(&hwmon).expect("create coretemp fixture");
    fs::write(hwmon.join("name"), "coretemp\n").expect("write hwmon name");
    fs::write(hwmon.join("temp1_input"), "45000\n").expect("write package temp");
    fs::write(hwmon.join("temp1_label"), "Package id 0\n").expect("write package label");
    // Only Core 0 sensor — Core 1 intentionally absent.
    fs::write(hwmon.join("temp2_input"), "41000\n").expect("write core 0 temp");
    fs::write(hwmon.join("temp2_label"), "Core 0\n").expect("write core 0 label");

    let observation = observe_temperatures_at(
        &fixture.0.join("thermal"),
        &fixture.0.join("hwmon"),
        &fixture.0,
        4,
    );

    assert_eq!(observation.per_core_c.len(), 4);
    let logical_0 = observation.per_core_c[0].expect("logical 0 maps to Core 0");
    let logical_1 = observation.per_core_c[1].expect("logical 1 maps to Core 0");
    assert_eq!(logical_0.temperature_c, 41.0);
    assert_eq!(logical_1.temperature_c, 41.0);
    assert_eq!(
        logical_0.provenance,
        TemperatureProvenance::DirectlyMeasured
    );
    assert_eq!(
        logical_1.provenance,
        TemperatureProvenance::PhysicalSiblingShared
    );
    assert!(
        observation.per_core_c[2].is_none(),
        "unmapped physical core emits None, not a fabricated value"
    );
    assert!(observation.per_core_c[3].is_none());
}

/// Legacy-field projection helper mirroring how `compute.rs` rebuilds the
/// historical short `Vec<f32>` from the new typed per-logical-core Vec. Tests
/// assert against this projection so the legacy contract stays green.
fn per_core_temps_for_legacy_field(readings: &[Option<LogicalCoreTemperature>]) -> Vec<f32> {
    readings
        .iter()
        .filter_map(|reading| match reading {
            Some(LogicalCoreTemperature {
                temperature_c,
                provenance: TemperatureProvenance::DirectlyMeasured,
            }) => Some(*temperature_c),
            _ => None,
        })
        .collect()
}

fn write_hwmon_chip(root: &std::path::Path, index: usize, name: &str) -> std::path::PathBuf {
    let chip = root.join("hwmon").join(format!("hwmon{index}"));
    fs::create_dir_all(&chip).expect("create hwmon chip fixture");
    fs::write(chip.join("name"), format!("{name}\n")).expect("write hwmon name");
    chip
}

fn write_thermal_zone(root: &std::path::Path, zone: &str, sensor_type: &str, milli_c: u32) {
    let zone_dir = root.join("thermal").join(zone);
    fs::create_dir_all(&zone_dir).expect("create thermal zone fixture");
    fs::write(zone_dir.join("type"), format!("{sensor_type}\n")).expect("write zone type");
    fs::write(zone_dir.join("temp"), format!("{milli_c}\n")).expect("write zone temp");
}

#[test]
fn exact_chip_tier_beats_labeled_fallback_and_thermal_zone() {
    let fixture = FixtureDir::new();
    let coretemp = write_hwmon_chip(&fixture.0, 0, "coretemp");
    fs::write(coretemp.join("temp1_input"), "42000\n").expect("write package temp");
    fs::write(coretemp.join("temp1_label"), "Package id 0\n").expect("write label");
    // Steam-Deck shape: the package temp lives on another chip, behind a
    // CPU-package-labeled channel.
    let apu_chip = write_hwmon_chip(&fixture.0, 1, "amdgpu");
    fs::write(apu_chip.join("temp1_input"), "99000\n").expect("write APU temp");
    fs::write(apu_chip.join("temp1_label"), "APU\n").expect("write APU label");
    write_thermal_zone(&fixture.0, "thermal_zone0", "x86_pkg_temp", 77_000);

    let observation = observe_temperatures_at(
        &fixture.0.join("thermal"),
        &fixture.0.join("hwmon"),
        &fixture.0,
        2,
    );

    assert_eq!(observation.package_c, Some(42.0));
    assert_eq!(
        observation.package_source,
        CpuTemperatureSource::Coretemp,
        "a dedicated CPU sensor chip outranks the labeled fallback and zones"
    );
}

#[test]
fn labeled_hwmon_fallback_accepts_cpu_package_labels() {
    for (chip_name, label) in [
        ("amdgpu", "APU"),
        ("nct6775", "Package id 0"),
        ("amdgpu", "Tctl"),
        ("amdgpu", "Tdie"),
    ] {
        let fixture = FixtureDir::new();
        let chip = write_hwmon_chip(&fixture.0, 0, chip_name);
        fs::write(chip.join("temp1_input"), "54000\n").expect("write temp");
        fs::write(chip.join("temp1_label"), format!("{label}\n")).expect("write label");

        let observation = observe_temperatures_at(
            &fixture.0.join("thermal"),
            &fixture.0.join("hwmon"),
            &fixture.0,
            4,
        );

        assert_eq!(observation.package_c, Some(54.0), "{chip_name}/{label}");
        assert_eq!(
            observation.package_source,
            CpuTemperatureSource::PackageHwmon,
            "{chip_name}/{label} must record the labeled fallback tier"
        );
        assert_eq!(observation.status.outcome, SourceOutcome::Available);
    }
}

#[test]
fn unlabeled_channel_uses_the_chip_name_as_the_effective_label() {
    let fixture = FixtureDir::new();
    let chip = write_hwmon_chip(&fixture.0, 0, "apu_thermal");
    fs::write(chip.join("temp1_input"), "48000\n").expect("write temp");

    let observation = observe_temperatures_at(
        &fixture.0.join("thermal"),
        &fixture.0.join("hwmon"),
        &fixture.0,
        4,
    );

    assert_eq!(observation.package_c, Some(48.0));
    assert_eq!(
        observation.package_source,
        CpuTemperatureSource::PackageHwmon
    );
}

#[test]
fn labeled_hwmon_fallback_rejects_gpu_and_board_labels() {
    for (chip_name, label) in [
        ("amdgpu", "edge"),
        ("amdgpu", "junction"),
        ("amdgpu", "mem"),
        ("nct6775", "VRM"),
    ] {
        let fixture = FixtureDir::new();
        let chip = write_hwmon_chip(&fixture.0, 0, chip_name);
        fs::write(chip.join("temp1_input"), "88000\n").expect("write temp");
        fs::write(chip.join("temp1_label"), format!("{label}\n")).expect("write label");

        let observation = observe_temperatures_at(
            &fixture.0.join("thermal"),
            &fixture.0.join("hwmon"),
            &fixture.0,
            4,
        );

        assert_eq!(
            observation.package_c, None,
            "{chip_name}/{label} must never feed the CPU package readout"
        );
        // The rejected channel was never read as an observed item, so the
        // source reached but observed nothing.
        assert_eq!(
            observation.status.outcome,
            SourceOutcome::Empty,
            "{chip_name}/{label}"
        );
    }
}

#[test]
fn unlabeled_gpu_chip_channels_are_rejected_by_the_chip_name() {
    let fixture = FixtureDir::new();
    let chip = write_hwmon_chip(&fixture.0, 0, "amdgpu");
    fs::write(chip.join("temp1_input"), "88000\n").expect("write temp");

    let observation = observe_temperatures_at(
        &fixture.0.join("thermal"),
        &fixture.0.join("hwmon"),
        &fixture.0,
        4,
    );

    assert_eq!(
        observation.package_c, None,
        "an unlabeled channel on a GPU chip must not be blindly taken"
    );
}

#[test]
fn labeled_fallback_outranks_thermal_zone_and_zone_records_provenance() {
    let fixture = FixtureDir::new();
    let chip = write_hwmon_chip(&fixture.0, 0, "amdgpu");
    fs::write(chip.join("temp1_input"), "54000\n").expect("write temp");
    fs::write(chip.join("temp1_label"), "APU\n").expect("write label");
    write_thermal_zone(&fixture.0, "thermal_zone0", "x86_pkg_temp", 77_000);

    let observation = observe_temperatures_at(
        &fixture.0.join("thermal"),
        &fixture.0.join("hwmon"),
        &fixture.0,
        4,
    );
    assert_eq!(observation.package_c, Some(54.0));
    assert_eq!(
        observation.package_source,
        CpuTemperatureSource::PackageHwmon,
        "the labeled fallback outranks every thermal zone"
    );

    let zone_only = FixtureDir::new();
    write_thermal_zone(&zone_only.0, "thermal_zone0", "cpu_thermal", 61_000);
    let observation = observe_temperatures_at(
        &zone_only.0.join("thermal"),
        &zone_only.0.join("hwmon"),
        &zone_only.0,
        4,
    );
    assert_eq!(observation.package_c, Some(61.0));
    assert_eq!(
        observation.package_source,
        CpuTemperatureSource::ThermalZone,
        "a zone-only host records the thermal-zone tier"
    );
}

#[test]
fn zenpower_is_an_exact_chip_tier() {
    let fixture = FixtureDir::new();
    let chip = write_hwmon_chip(&fixture.0, 0, "zenpower");
    fs::write(chip.join("temp1_input"), "48000\n").expect("write die temp");
    fs::write(chip.join("temp1_label"), "Tdie\n").expect("write die label");

    let observation = observe_temperatures_at(
        &fixture.0.join("thermal"),
        &fixture.0.join("hwmon"),
        &fixture.0,
        8,
    );

    assert_eq!(observation.package_c, Some(48.0));
    assert_eq!(observation.package_source, CpuTemperatureSource::Zenpower);
    assert!(observation.per_core_c.is_empty());
    assert_eq!(observation.status.outcome, SourceOutcome::Available);
}
