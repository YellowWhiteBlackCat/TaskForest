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
    let rows = cpu_spec_rows(&CpuMetrics::default(), &HardwareInfo::default());
    for key in ["cpu.base_speed", "common.sockets"] {
        assert_eq!(value_of(&rows, key), formatting::missing_value(), "{key}");
    }
    for key in ["common.l1_cache", "common.l2_cache", "common.l3_cache"] {
        assert_eq!(value_of(&rows, key), formatting::missing_value(), "{key}");
    }
}

/// Present facts use the shared spellings: base clock via
/// `optional_ghz` ("2.40 GHz" from 2400 MHz) and caches via KiB→bytes
/// `format_mib_2` ("2.00 MiB" from 2048 KiB).
#[test]
fn cpu_spec_rows_format_present_facts() {
    let mut cpu = CpuMetrics::default();
    cpu.physical_cores = Some(8);
    cpu.logical_cores = Some(16);
    cpu.l1_cache_kb = Some(2048);
    let hardware = HardwareInfo {
        base_freq_mhz: Some(2_400),
        sockets: Some(1),
        ..HardwareInfo::default()
    };
    let rows = cpu_spec_rows(&cpu, &hardware);
    assert_eq!(value_of(&rows, "cpu.base_speed"), "2.40 GHz");
    assert_eq!(value_of(&rows, "common.sockets"), "1");
    assert_eq!(value_of(&rows, "common.cores"), "8");
    assert_eq!(value_of(&rows, "cpu.logical_processors"), "16");
    assert_eq!(value_of(&rows, "common.l1_cache"), "2.00 MiB");
}

/// A hybrid part emits one row per non-zero class, ordered P then E,
/// between the Cores and Logical processors rows; the full row order is
/// the one `spec_grid` paints.
#[test]
fn cpu_spec_rows_emit_hybrid_rows_in_order() {
    let mut cpu = CpuMetrics::default();
    cpu.performance_policy = crate::core::metrics::CpuPerformancePolicy {
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
    let rows = cpu_spec_rows(&cpu, &hardware);
    let expected_keys = [
        "cpu.base_speed",
        "common.sockets",
        "common.cores",
        "cpu.performance_cores",
        "cpu.efficiency_cores",
        "cpu.logical_processors",
        "common.virtualization",
        "common.l1_cache",
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
    assert_eq!(rows[3].1, "4", "P-core count");
    assert_eq!(rows[4].1, "8", "E-core count");
}

#[test]
fn missing_policy_rows_are_omitted_instead_of_dashed() {
    let rows = cpu_spec_rows(&CpuMetrics::default(), &HardwareInfo::default());
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
