use super::*;
use taskmanager_application::MsrReadoutSession;
use taskmanager_core::core::hardware::CpuIdentity;
use taskmanager_core::core::metrics::CpuPackageMetrics;
use taskmanager_core::core::metrics::CpuPerformancePolicy;
use taskmanager_core::core::metrics::{MsrReadoutSnapshot, MsrThermalStatusReadout};
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_platform_contract::RequestId;
use taskmanager_shell::presentation::MISSING_VALUE;
use taskmanager_shell::presentation::cpu_thermal_throttle_summary;
use taskmanager_shell::presentation::msr_thermal_status_summary;
use taskmanager_test_support::pin_english;

fn value_of(rows: &[(String, String)], key: &'static str) -> String {
    rows.iter()
        .find(|(k, _)| k == i18n::t(key))
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

/// Absent facts do not consume a row in the fixed, non-scrolling detail rail.
/// The permission center owns the explanation/action for optional data.
#[test]
fn cpu_spec_rows_omit_missing_facts() {
    let rows = cpu_spec_rows(
        &CpuMetrics::default(),
        &HardwareInfo::default(),
        UnitPreferences::default(),
        None,
    );
    for key in ["cpu.base_speed", "common.sockets"] {
        assert_eq!(value_of(&rows, key), "", "{key} must be omitted");
    }
    for key in [
        "common.l1_data_cache",
        "common.l1_instruction_cache",
        "common.l2_cache",
        "common.l3_cache",
    ] {
        assert_eq!(value_of(&rows, key), "", "{key} must be omitted");
    }
    // An unprobed CPUID identity (non-x86 host, fixture inventory) renders no
    // row at all rather than a dash slot — same discipline as policy rows.
    for key in ["system.cpu_vendor", "system.cpu_identity"] {
        assert_eq!(
            value_of(&rows, key),
            "",
            "{key} must be absent when the identity was never probed"
        );
    }
}

/// A probed identity leads the spec list: the native vendor string renders
/// verbatim and the family/model/stepping code comes from the SDM display
/// combination (7 + B<<4 = 183 for the Raptor Lake field set).
#[test]
fn cpu_spec_rows_emit_identity_rows_first_when_probed() {
    let hardware = HardwareInfo {
        cpu_identity: CpuIdentity::from_cpuid_parts(
            Some("GenuineIntel".into()),
            0x6,
            0x0,
            0x7,
            0xB,
            0x1,
        ),
        base_freq_mhz: Some(2_400),
        ..HardwareInfo::default()
    };
    let rows = cpu_spec_rows(
        &CpuMetrics::default(),
        &hardware,
        UnitPreferences::default(),
        None,
    );
    assert_eq!(rows[0].0, i18n::t("system.cpu_codename"));
    assert_eq!(rows[0].1, "Raptor Lake-S/HX (13th/14th gen)");
    assert_eq!(rows[1].0, i18n::t("system.cpu_process"));
    assert_eq!(rows[1].1, "Intel 7");
    assert_eq!(rows[2].0, i18n::t("system.cpu_vendor"));
    assert_eq!(rows[2].1, "GenuineIntel");
    assert_eq!(rows[3].0, i18n::t("system.cpu_identity"));
    assert_eq!(rows[3].1, "6 / 183 / 1");
    assert_eq!(
        rows[4].0,
        i18n::t("cpu.base_speed"),
        "identity leads the list"
    );
}

/// Present facts use the shared spellings: base clock via
/// `optional_ghz` ("2.40 GHz" from 2400 MHz) and caches via KiB→bytes
/// The core Memory ladder ("2.0 MiB" from 2048 KiB at the default prefs).
#[test]
fn cpu_spec_rows_format_present_facts() {
    let mut cpu = CpuMetrics::default();
    cpu.physical_cores = Some(8);
    cpu.logical_cores = Some(16);
    cpu.l1d_cache_kb = Some(2048);
    let hardware = HardwareInfo {
        base_freq_mhz: Some(2_400),
        sockets: Some(1),
        ..HardwareInfo::default()
    };
    let rows = cpu_spec_rows(&cpu, &hardware, UnitPreferences::default(), None);
    assert_eq!(value_of(&rows, "cpu.base_speed"), "2.40 GHz");
    assert_eq!(value_of(&rows, "common.sockets"), "1");
    assert_eq!(value_of(&rows, "common.cores"), "8");
    assert_eq!(value_of(&rows, "cpu.logical_processors"), "16");
    assert_eq!(value_of(&rows, "common.l1_data_cache"), "2.0 MiB");
}

/// A hybrid part emits one row per non-zero class, ordered P then E,
/// between the Cores and Logical processors rows; the full row order is
/// the one `spec_grid` paints.
#[test]
fn cpu_spec_rows_emit_hybrid_rows_in_order() {
    let mut cpu = CpuMetrics::default();
    cpu.performance_policy = CpuPerformancePolicy {
        frequency_implementation: Some("driver".into()),
        active_policy: Some("governor".into()),
        energy_preference: Some("preference".into()),
        ..Default::default()
    };
    let hardware = HardwareInfo {
        core_breakdown: CoreBreakdown {
            p_cores: 4,
            e_cores: 8,
            lp_cores: 0,
        },
        ..HardwareInfo::default()
    };
    let rows = cpu_spec_rows(&cpu, &hardware, UnitPreferences::default(), None);
    let expected_keys = [
        "cpu.performance_cores",
        "cpu.efficiency_cores",
        "common.virtualization",
        "cpu.cpufreq_driver",
        "cpu.cpufreq_governor",
        "cpu.power_preference",
    ];
    assert_eq!(rows.len(), expected_keys.len());
    for (row, key) in rows.iter().zip(expected_keys) {
        assert_eq!(row.0, i18n::t(key).to_string(), "row order: {key}");
    }
    assert_eq!(rows[0].1, "4", "P-core count");
    assert_eq!(rows[1].1, "8", "E-core count");
}

#[test]
fn missing_policy_rows_are_omitted_instead_of_dashed() {
    let rows = cpu_spec_rows(
        &CpuMetrics::default(),
        &HardwareInfo::default(),
        UnitPreferences::default(),
        None,
    );
    for key in [
        "cpu.cpufreq_driver",
        "cpu.cpufreq_governor",
        "cpu.power_preference",
    ] {
        assert_eq!(
            value_of(&rows, key),
            "",
            "{key} must be absent when the platform reports no such policy fact"
        );
    }
}

/// The `power.thermal-throttle-events` delivery: the spec list renders the
/// shared [`taskmanager_shell::presentation::cpu_thermal_throttle_summary`]
/// fold, whose exact value form — one `S{package_id}` segment per observed
/// package, the labeled dash for an unobserved sibling counter, never a
/// fabricated `0` — is pinned once by the shell rule test. This anchor proves
/// the panel paints that shared fold and keeps the whole row absent when no
/// package observed a counter.
#[test]
fn cpu_spec_rows_render_the_thermal_throttle_counters_with_honest_absence() {
    pin_english();
    let units = UnitPreferences::default();
    let hardware = HardwareInfo::default();
    let mut cpu = CpuMetrics::default();
    let mut cold_package = CpuPackageMetrics::new(0);
    cold_package.package_throttle_count = None;
    cold_package.core_throttle_count = None;
    cpu.packages = vec![cold_package];
    let rows = cpu_spec_rows(&cpu, &hardware, units, None);
    assert_eq!(
        value_of(&rows, "cpu.thermal_throttle"),
        "",
        "an unobserved counter family must not grow a row (never a fabricated 0)"
    );

    let mut observed = CpuPackageMetrics::new(0);
    observed.package_throttle_count = Some(7);
    observed.core_throttle_count = Some(3);
    let mut package_only = CpuPackageMetrics::new(1);
    package_only.package_throttle_count = Some(12);
    package_only.core_throttle_count = None;
    cpu.packages = vec![observed, package_only];
    let rows = cpu_spec_rows(&cpu, &hardware, units, None);
    let value = value_of(&rows, "cpu.thermal_throttle");
    let expected =
        cpu_thermal_throttle_summary(&cpu).expect("observed counters must produce the shared fold");
    assert_eq!(
        value, expected,
        "the spec row must paint the shared thermal-throttle fold"
    );
    assert!(
        value.contains(MISSING_VALUE),
        "the unobserved sibling counter must keep the labeled dash: {value}"
    );
    assert!(
        !value.contains("Core 0"),
        "an unobserved core counter must stay a labeled dash: {value}"
    );
}

/// The real-time thermal-status / PROCHOT delivery: the spec list paints the
/// shared [`taskmanager_shell::presentation::msr_thermal_status_summary`] fold
/// beside the cumulative counters — one `CPU {n} {state}` segment per node
/// with the shared asserted/clear words and the honest dash for an
/// unreadable/unimplemented register. The whole row is absent while the
/// privileged `telemetry.cpu.msr` lane produced no accepted readout.
#[test]
fn cpu_spec_rows_render_the_real_time_thermal_status_with_honest_absence() {
    pin_english();
    let units = UnitPreferences::default();
    let hardware = HardwareInfo::default();
    let cpu = CpuMetrics::default();

    // No accepted MSR readout: the real-time row is absent, not a dash slot.
    let rows = cpu_spec_rows(&cpu, &hardware, units, None);
    assert_eq!(
        value_of(&rows, "cpu.thermal_status"),
        "",
        "an unrun privileged lane must not grow a real-time row"
    );

    // A session with an accepted readout folds to the shared value: asserted,
    // clear, and the honest dash for the unreadable register.
    let mut session = MsrReadoutSession::default();
    let attempt = session.begin_attempt();
    let request_id = RequestId::new(1).expect("fixture request id");
    assert!(session.accept_attempt(attempt, request_id));
    assert!(session.complete(
        request_id,
        MsrReadoutSnapshot::success(Vec::new()).with_thermal(vec![
            MsrThermalStatusReadout {
                cpu: 0,
                thermal_status: Some(true),
                ..MsrThermalStatusReadout::default()
            },
            MsrThermalStatusReadout {
                cpu: 1,
                thermal_status: Some(false),
                ..MsrThermalStatusReadout::default()
            },
            MsrThermalStatusReadout {
                cpu: 2,
                thermal_status: None,
                ..MsrThermalStatusReadout::default()
            },
        ]),
    ));
    let expected =
        msr_thermal_status_summary(session.state()).expect("an accepted readout must fold");
    let rows = cpu_spec_rows(&cpu, &hardware, units, Some(&expected));
    assert_eq!(
        value_of(&rows, "cpu.thermal_status"),
        expected,
        "the spec row must paint the shared real-time status fold"
    );
    assert!(
        expected.contains(i18n::t("cpu.thermal_status_asserted"))
            && expected.contains(i18n::t("cpu.thermal_status_clear")),
        "the fold must carry the shared asserted/clear vocabulary: {expected}"
    );
    assert!(
        expected.contains(MISSING_VALUE),
        "an unreadable register must keep the honest dash: {expected}"
    );
}
