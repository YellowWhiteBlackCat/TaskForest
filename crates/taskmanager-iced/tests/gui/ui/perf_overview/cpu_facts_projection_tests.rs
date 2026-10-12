use super::*;
use taskmanager_core::core::metrics::ScalarObservationGroup;

fn stat_value<'a>(stats: &'a [StatRow], key: &'static str) -> Option<&'a str> {
    stats
        .iter()
        .find(|row| row.label() == t(key))
        .and_then(StatRow::value)
}

#[test]
fn cpu_stats_project_observed_cache_hybrid_and_power_facts() {
    pin_english();
    let mut app = crate::IcedApp::demo();
    edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        snapshot.cpu.l1d_cache_kb = Some(64);
        snapshot.cpu.l2_cache_kb = Some(1_024);
        snapshot.cpu.physical_cores = Some(16);
        snapshot.cpu.logical_cores = Some(22);
        let mut observations = snapshot.cpu.scalar_observations().clone();
        observations.per_core_temperature_group =
            ScalarObservationGroup::available(vec![51.0, 59.0], 1);
        snapshot.cpu.apply_scalar_observations(observations);
    });
    edit_hardware(&mut app.shell, |hardware| {
        let hardware = hardware.as_mut().expect("demo hardware");
        hardware.base_freq_mhz = Some(2_400);
        hardware.virt = Some("VT-x".to_owned());
        hardware.core_breakdown = CoreBreakdown {
            p_cores: 6,
            e_cores: 8,
            lp_cores: 2,
        };
    });

    let (_, _, stats) = cpu_memory_header_and_stats(&app, PerfDevice::Cpu);
    assert_eq!(stat_value(&stats, "common.l1_data_cache"), Some("64 KiB"));
    assert_eq!(stat_value(&stats, "common.l2_cache"), Some("1 MiB"));
    assert_eq!(
        stat_value(&stats, "system.field.base_frequency"),
        Some("2400 MHz")
    );
    assert_eq!(stat_value(&stats, "common.virtualization"), Some("VT-x"));
    assert_eq!(stat_value(&stats, "common.avg"), Some("55 °C · Max 59 °C"));
    assert_eq!(stat_value(&stats, "cpu.performance_cores"), Some("6"));
    assert_eq!(stat_value(&stats, "cpu.efficiency_cores"), Some("8"));
    assert_eq!(stat_value(&stats, "cpu.low_power_cores"), Some("2"));
    assert_eq!(
        stat_value(&stats, "cpu.cpufreq_driver"),
        Some("intel_pstate")
    );
    assert_eq!(
        stat_value(&stats, "cpu.cpufreq_governor"),
        Some("powersave")
    );
    assert_eq!(
        stat_value(&stats, "cpu.power_preference"),
        Some("balance_performance")
    );
    assert_eq!(stat_value(&stats, "system.field.sockets"), Some("1"));

    edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        snapshot.cpu.physical_cores = None;
        snapshot.cpu.logical_cores = None;
    });
    edit_hardware(&mut app.shell, |hardware| {
        hardware.as_mut().expect("demo hardware").cpu_cores = Some(12);
    });
    let (_, _, topology_fallback_stats) = cpu_memory_header_and_stats(&app, PerfDevice::Cpu);
    assert_eq!(
        stat_value(&topology_fallback_stats, "common.cores"),
        Some("— / 12"),
        "the typed hardware topology supplies an observed logical count without inventing physical cores"
    );

    edit_hardware(&mut app.shell, |hardware| {
        hardware.as_mut().expect("demo hardware").core_breakdown = CoreBreakdown::default();
    });
    let (_, _, homogeneous_stats) = cpu_memory_header_and_stats(&app, PerfDevice::Cpu);
    assert!(
        homogeneous_stats.iter().all(|row| {
            ![
                "cpu.performance_cores",
                "cpu.efficiency_cores",
                "cpu.low_power_cores",
            ]
            .into_iter()
            .any(|key| row.label() == t(key))
        }),
        "an unclassified homogeneous inventory must not fabricate hybrid rows"
    );

    edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        let mut observations = snapshot.cpu.scalar_observations().clone();
        observations.per_core_temperature_group = ScalarObservationGroup::available(vec![54.0], 2);
        snapshot.cpu.apply_scalar_observations(observations);
    });
    let (_, _, single_core_stats) = cpu_memory_header_and_stats(&app, PerfDevice::Cpu);
    assert_eq!(
        stat_value(&single_core_stats, "common.avg"),
        None,
        "one real core temperature does not claim an average"
    );
    assert_eq!(
        stat_value(&single_core_stats, "common.cores"),
        Some("— / 12"),
        "the observed logical topology remains visible independently of one temperature channel"
    );
}
