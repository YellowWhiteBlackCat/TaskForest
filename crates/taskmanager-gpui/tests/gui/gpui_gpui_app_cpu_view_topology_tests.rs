use crate::core::hardware::CoreBreakdown;

use super::{
    cpu_frequency_readout, cpu_frequency_readout_for_source, cpu_temperature_readout,
    cpu_temperature_readout_for_source, heterogeneous_core_rows,
};
use crate::core::metrics::{CpuFrequencySource, CpuTemperatureSource};

#[test]
fn optional_cpu_readouts_distinguish_missing_from_measured_zero() {
    assert_eq!(cpu_frequency_readout(None), "—");
    assert_eq!(cpu_frequency_readout(Some(0)), "0.00 GHz");
    assert_eq!(cpu_frequency_readout(Some(3_200)), "3.20 GHz");
    assert_eq!(
        cpu_frequency_readout_for_source(Some(2_400), CpuFrequencySource::BogoMips),
        "2400.00 BogoMIPS"
    );
    assert_eq!(cpu_temperature_readout(None), "—");
    assert_eq!(cpu_temperature_readout(Some(0.0)), "0 °C");
    assert_eq!(cpu_temperature_readout(Some(54.4)), "54 °C");
}

#[test]
fn temperature_readout_qualifies_labeled_fallback_tiers_only() {
    taskmanager_test_support::pin_english();
    // Dedicated CPU sensor chips keep the plain reading.
    assert_eq!(
        cpu_temperature_readout_for_source(Some(54.0), CpuTemperatureSource::Coretemp),
        "54 °C"
    );
    assert_eq!(
        cpu_temperature_readout_for_source(Some(54.0), CpuTemperatureSource::K10temp),
        "54 °C"
    );
    // The fallback tiers append the source qualifier so a derived reading
    // never masquerades as a dedicated CPU sensor chip.
    assert_eq!(
        cpu_temperature_readout_for_source(Some(54.0), CpuTemperatureSource::PackageHwmon),
        "54 °C · hwmon fallback"
    );
    assert_eq!(
        cpu_temperature_readout_for_source(Some(54.0), CpuTemperatureSource::ThermalZone),
        "54 °C · ACPI thermal zone"
    );
    // A missing value never grows a qualifier.
    assert_eq!(
        cpu_temperature_readout_for_source(None, CpuTemperatureSource::PackageHwmon),
        "—"
    );
}

#[test]
fn hybrid_core_counts_render_as_separate_aligned_rows() {
    let rows = heterogeneous_core_rows(&CoreBreakdown {
        p_cores: 4,
        e_cores: 8,
        lp_cores: 2,
    });
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter().map(|row| row.1.as_str()).collect::<Vec<_>>(),
        ["4", "8", "2"]
    );
    assert!(rows.iter().all(|row| !row.0.contains('+')));
}

#[test]
fn homogeneous_cpu_does_not_add_redundant_type_row() {
    assert!(
        heterogeneous_core_rows(&CoreBreakdown {
            p_cores: 16,
            e_cores: 0,
            lp_cores: 0,
        })
        .is_empty()
    );
}
