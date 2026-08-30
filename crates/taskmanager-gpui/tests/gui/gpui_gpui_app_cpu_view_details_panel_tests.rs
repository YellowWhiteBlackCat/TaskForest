use super::*;

fn value_of(rows: &[(String, String)], key: &'static str) -> String {
    rows.iter()
        .find(|(k, _)| k == i18n::t(key))
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

/// Absent facts keep their row slots as the shared dash (ADR-020
/// `missing_value`) — an uncollected clock/socket/cache never renders a
/// fabricated number.
#[test]
fn cpu_spec_rows_render_shared_dash_for_missing_facts() {
    let rows = cpu_spec_rows(
        &CpuMetrics::default(),
        &HardwareInfo::default(),
        taskmanager_core::core::units::UnitPreferences::default(),
    );
    for key in ["cpu.base_speed", "common.sockets"] {
        assert_eq!(value_of(&rows, key), formatting::missing_value(), "{key}");
    }
    for key in [
        "common.l1_data_cache",
        "common.l1_instruction_cache",
        "common.l2_cache",
        "common.l3_cache",
    ] {
        assert_eq!(value_of(&rows, key), formatting::missing_value(), "{key}");
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
        cpu_identity: taskmanager_core::core::hardware::CpuIdentity::from_cpuid_parts(
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
        taskmanager_core::core::units::UnitPreferences::default(),
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
    let rows = cpu_spec_rows(
        &cpu,
        &hardware,
        taskmanager_core::core::units::UnitPreferences::default(),
    );
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
    cpu.performance_policy = taskmanager_core::core::metrics::CpuPerformancePolicy {
        frequency_implementation: Some("driver".into()),
        active_policy: Some("governor".into()),
        energy_preference: Some("preference".into()),
    };
    let hardware = HardwareInfo {
        core_breakdown: CoreBreakdown {
            p_cores: 4,
            e_cores: 8,
            lp_cores: 0,
        },
        ..HardwareInfo::default()
    };
    let rows = cpu_spec_rows(
        &cpu,
        &hardware,
        taskmanager_core::core::units::UnitPreferences::default(),
    );
    let expected_keys = [
        "cpu.base_speed",
        "cpu.multiplier",
        "common.sockets",
        "common.cores",
        "cpu.performance_cores",
        "cpu.efficiency_cores",
        "cpu.logical_processors",
        "common.virtualization",
        "common.l1_data_cache",
        "common.l1_instruction_cache",
        "common.l2_cache",
        "common.l3_cache",
        "cpu.cpufreq_driver",
        "cpu.cpufreq_governor",
        "cpu.power_preference",
    ];
    assert_eq!(rows.len(), expected_keys.len());
    for (row, key) in rows.iter().zip(expected_keys) {
        assert_eq!(row.0, i18n::t(key).to_string(), "row order: {key}");
    }
    assert_eq!(rows[4].1, "4", "P-core count");
    assert_eq!(rows[5].1, "8", "E-core count");
}

#[test]
fn missing_policy_rows_are_omitted_instead_of_dashed() {
    let rows = cpu_spec_rows(
        &CpuMetrics::default(),
        &HardwareInfo::default(),
        taskmanager_core::core::units::UnitPreferences::default(),
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
