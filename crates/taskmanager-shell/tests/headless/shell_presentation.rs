use super::*;
use taskmanager_application::AppAction;
use taskmanager_application::MsrReadoutSession;
use taskmanager_core::SystemLoadAverage;
use taskmanager_core::core::metrics::{
    CpuInterruptSnapshot, CpuMetrics, CpuPackageMetrics, MsrReadoutSnapshot,
    MsrThermalStatusReadout, NetworkAdapterType, NetworkMetrics,
};
use taskmanager_core::core::services::ServiceDiagnostics;
use taskmanager_core::core::time::{LocalTimeRules, LocalTimeRulesObservation};
use taskmanager_platform_contract::RequestId;

#[test]
fn service_exit_diagnostics_explain_configuration_failure_in_both_locales() {
    let diagnostics = ServiceDiagnostics {
        exec_main_status: Some(78),
        ..Default::default()
    };
    for (language, explanation) in [
        (i18n::Language::En, "Invalid configuration"),
        (i18n::Language::Zh, "配置错误"),
    ] {
        i18n::set_language(language);
        let rows = service_diagnostics_rows(&diagnostics);
        for key in ["svc.exit_status", "svc.failure_cause"] {
            let value = &rows
                .iter()
                .find(|row| row.0 == i18n::t(key))
                .expect("diagnostic row")
                .1;
            assert_eq!(value, &format!("78 (EX_CONFIG) · {explanation}"));
        }
    }
    assert_eq!(service_exit::exit_status_text(123), "123");
}

#[test]
fn every_command_has_discoverable_help() {
    let help = command_help();
    assert_eq!(help.len(), CommandId::ALL.len());
    for row in help {
        assert!(!row.icon.is_empty());
        assert!(!row.label.is_empty());
        assert!(!row.description.is_empty());
        assert!(
            !row.shortcut.is_empty(),
            "missing shortcut for {:?}",
            row.command
        );
    }
}

/// Command copy resolves through the shared i18n catalog using the keys
/// carried by the application spec table — a spec row whose locale keys
/// are missing would degrade the help overlay to raw key literals.
#[test]
fn command_help_labels_and_descriptions_resolve_through_the_catalog() {
    i18n::set_language(i18n::Language::En);
    for row in command_help() {
        assert_ne!(
            row.label,
            row.command.label_key(),
            "label key {:?} missing from the en catalog",
            row.command.label_key()
        );
        assert_ne!(
            row.description,
            row.command.description_key(),
            "description key {:?} missing from the en catalog",
            row.command.description_key()
        );
    }
}

#[test]
fn every_shared_page_has_typed_presentation_and_shortcut() {
    let pages = page_help();

    assert_eq!(pages.len(), AppPage::ALL.len());
    for page in pages {
        assert!(!page.label.is_empty());
        assert!(!page.description.is_empty());
        assert!(!page.shortcut.is_empty());
        assert_eq!(page.command.action(), AppAction::SelectPage(page.page));
    }
}

#[test]
fn byte_and_duration_formatting_is_binary_and_deterministic() {
    assert_eq!(bytes(0), "0 B");
    assert_eq!(bytes(1536), "1.5 KiB");
    assert_eq!(bytes(2 * 1024 * 1024), "2.0 MiB");
    assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
    assert_eq!(duration(90), "00h 01m");
    assert_eq!(duration(86_400 + 3_600), "1d 01h 00m");
    assert_eq!(optional_bytes(Some(0)), "0 B");
    assert_eq!(optional_bytes(None), "—");
}

#[test]
fn normalized_load_summary_uses_the_shared_typed_fact() {
    i18n::set_language(i18n::Language::En);
    let load = SystemLoadAverage::from_raw(8.0, 4.0, 2.0, 4).expect("fixture load is valid");
    assert_eq!(
        load_average_summary(&load),
        "Load 1m 2.00× · 5m 1.00× · 15m 0.50×"
    );
}

#[test]
fn topology_and_interrupt_summaries_keep_pairing_and_distribution_visible() {
    i18n::set_language(i18n::Language::En);
    let mut cpu = CpuMetrics::default();
    cpu.brand = Some("AuthenticAMD Ryzen".into());
    let mut package = CpuPackageMetrics::new(0);
    package.chiplet_ids = vec![0, 1];
    package.smt_sibling_groups = vec![vec![0, 8], vec![1, 9]];
    package.logical_core_ids = (0..16).collect();
    package.physical_core_count = Some(8);
    cpu.packages = vec![package];
    cpu.interrupts = Some(CpuInterruptSnapshot {
        total: Some(100),
        per_logical_cpu: vec![80, 20],
    });
    let topology = cpu_topology_summary(&cpu).expect("topology summary");
    assert!(topology.contains("CCD0/1"));
    assert!(topology.contains("SMT 0/8,1/9"));
    let interrupts = cpu_interrupt_summary(&cpu).expect("interrupt summary");
    assert!(interrupts.contains("CPU0 80%") && interrupts.contains("CPU1 20%"));
    let compact = cpu_interrupt_compact_summary(&cpu).expect("compact interrupt summary");
    assert_eq!(compact, "Total 100 · CPU0 80 (80%)");
}

/// The `power.thermal-throttle-events` counters fold into one shared value:
/// one `S{package_id}` segment per observed package joined with ` | `, the
/// labeled dash for an unobserved sibling counter, and `None` when no package
/// observed a counter. A measured zero is an observation and stays visible —
/// only an unobserved counter may become the dash.
#[test]
fn thermal_throttle_summary_keeps_package_ids_dashes_and_honest_absence() {
    i18n::set_language(i18n::Language::En);

    // No package observed any counter: the whole summary is absent, not a
    // list of fabricated zeros.
    let mut cpu = CpuMetrics::default();
    cpu.packages = vec![CpuPackageMetrics::new(0), CpuPackageMetrics::new(3)];
    assert_eq!(cpu_thermal_throttle_summary(&cpu), None);

    // Per-package segments carry the package id and both counters; the
    // unobserved sibling keeps its labeled dash and the package with no
    // observed counter contributes no segment at all.
    let mut observed = CpuPackageMetrics::new(0);
    observed.package_throttle_count = Some(7);
    observed.core_throttle_count = Some(3);
    let mut package_only = CpuPackageMetrics::new(1);
    package_only.package_throttle_count = Some(12);
    let mut cold = CpuPackageMetrics::new(2);
    cold.package_throttle_count = None;
    cold.core_throttle_count = None;
    cpu.packages = vec![observed, package_only, cold];
    let summary = cpu_thermal_throttle_summary(&cpu).expect("observed counters must fold");
    assert_eq!(summary, "S0 Package 7 · Core 3 | S1 Package 12 · Core —");
    assert!(
        !summary.contains("Core 0"),
        "an unobserved counter must not become a fabricated zero: {summary}"
    );

    // A real zero counter is an observation: it stays a number beside its
    // own package id rather than collapsing into the dash.
    let mut measured_zero = CpuPackageMetrics::new(4);
    measured_zero.package_throttle_count = Some(0);
    measured_zero.core_throttle_count = Some(0);
    cpu.packages = vec![measured_zero];
    assert_eq!(
        cpu_thermal_throttle_summary(&cpu).as_deref(),
        Some("S4 Package 0 · Core 0")
    );
}

/// The real-time thermal-status / PROCHOT fold: one `CPU {n} {state}` segment
/// per readout node, the shared asserted/clear words for a verified bit, the
/// shared honest dash for an unreadable/unimplemented register, and `None`
/// when the privileged `telemetry.cpu.msr` lane produced no thermal rows. The
/// session-aware entry reads the application-owned request session, so a fresh
/// session has no real-time row and a refresh keeps the last accepted read.
#[test]
fn thermal_status_summary_renders_asserted_clear_and_absent_per_node() {
    i18n::set_language(i18n::Language::En);
    let thermal = vec![
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
    ];
    assert_eq!(
        cpu_thermal_status_summary(&thermal).as_deref(),
        Some("CPU 0 Asserted | CPU 1 Clear | CPU 2 —"),
        "asserted, clear, and the honest dash are the three shared states"
    );
    // The same words split one node per row, so a narrow value column can
    // paint each node whole instead of clipping the joined summary.
    assert_eq!(
        cpu_thermal_status_segments(&thermal),
        vec!["CPU 0 Asserted", "CPU 1 Clear", "CPU 2 —"],
        "each node must be its own segment with the shared state word"
    );
    assert_eq!(
        cpu_thermal_status_summary(&[]),
        None,
        "no thermal rows at all is an absent fact, never a fabricated clear"
    );
    assert!(
        cpu_thermal_status_segments(&[]).is_empty(),
        "an absent lane yields no per-node segments"
    );

    let mut session = MsrReadoutSession::default();
    assert_eq!(
        msr_thermal_status_summary(session.state()),
        None,
        "a session that never ran has no real-time row"
    );
    let attempt = session.begin_attempt();
    let request_id = RequestId::new(1).expect("fixture request id");
    assert!(session.accept_attempt(attempt, request_id));
    assert!(session.complete(
        request_id,
        MsrReadoutSnapshot::success(Vec::new()).with_thermal(thermal),
    ));
    let expected = Some("CPU 0 Asserted | CPU 1 Clear | CPU 2 —");
    assert_eq!(
        msr_thermal_status_summary(session.state()).as_deref(),
        expected
    );
    assert_eq!(
        msr_thermal_status_segments(session.state()),
        vec!["CPU 0 Asserted", "CPU 1 Clear", "CPU 2 —"],
        "the session-aware segments must match the accepted snapshot"
    );
    let _ = session.begin_attempt();
    assert_eq!(
        msr_thermal_status_summary(session.state()).as_deref(),
        expected,
        "a refresh keeps the last accepted real-time read"
    );
}

#[test]
fn priority_tier_labels_resolve_non_empty_and_distinct_for_every_tier() {
    // Pin English so the "not the raw key" check is deterministic on any
    // runner (a missing catalog entry degrades to the key literal).
    i18n::set_language(i18n::Language::En);
    let mut seen: Vec<&'static str> = Vec::new();
    for tier in PriorityTier::ALL {
        let label = priority_tier_label(tier);
        assert!(!label.is_empty(), "empty label for {tier:?}");
        assert_ne!(
            label,
            tier.i18n_key(),
            "{tier:?} must resolve to a catalog entry, not the raw key"
        );
        assert!(
            !seen.contains(&label),
            "label {label:?} is shared by two tiers"
        );
        seen.push(label);
    }
    assert_eq!(seen.len(), PriorityTier::ALL.len());
}

#[test]
fn process_batch_action_labels_are_complete_and_action_specific() {
    use taskmanager_core::core::process::ProcessBatchAction;

    i18n::set_language(i18n::Language::En);
    let actions = [
        (ProcessBatchAction::End, "End task"),
        (ProcessBatchAction::EndProcessTree, "End process tree"),
        (ProcessBatchAction::Kill, "Force kill"),
        (ProcessBatchAction::Suspend, "Suspend"),
        (ProcessBatchAction::Resume, "Resume"),
        (
            ProcessBatchAction::SetPriority(PriorityTier::High),
            "Priority (High)",
        ),
    ];
    for (action, expected) in actions {
        assert_eq!(process_batch_action_label(action), expected);
    }
}

#[test]
fn nice_formats_purely_without_a_wall_clock() {
    // Nice signs a positive priority and leaves zero/negative bare.
    assert_eq!(optional_nice(Some(10)), "+10");
    assert_eq!(optional_nice(Some(0)), "0");
    assert_eq!(optional_nice(Some(-5)), "-5");
    assert_eq!(optional_nice(None), "—");
}

#[test]
fn local_clock_requires_injected_rules() {
    let unavailable = LocalTimeRulesObservation::unsupported(1);
    assert_eq!(start_clock_local(Some(10_921), &unavailable), "—");
    assert_eq!(local_timestamp(10_921_000, &unavailable), "—");

    let utc = LocalTimeRulesObservation::current(LocalTimeRules::utc(), 1);
    assert_eq!(start_clock_local(Some(10_921), &utc), "03:02");
    assert_eq!(local_timestamp(0, &utc), "1970-01-01 00:00:00");
}

#[test]
fn sensor_unit_formatting_matches_the_exact_conventions() {
    // Badge/graph convention: whole °C / RPM / MHz, one-decimal watts.
    assert_eq!(temperature_c(54.4), "54 °C");
    assert_eq!(fan_rpm(1234.6), "1235 RPM");
    assert_eq!(power_w(12.34), "12.3 W");
    assert_eq!(megahertz(2400.4), "2400 MHz");
    // Health-page convention: deliberately different precisions.
    assert_eq!(temperature_c_precise(36.76), "36.8 °C");
    assert_eq!(fan_rpm_i(1234), "1234 RPM");
    assert_eq!(power_w_precise(3.146), "3.15 W");
}

#[test]
fn gpu_identity_prefers_the_resolved_product_without_promoting_the_driver() {
    let mut resolved = GpuMetrics::default();
    resolved.brand = " Intel Xe Graphics ".into();
    resolved.marketing_name = Some(" Arc B390 ".into());
    resolved.driver = Some("xe".into());
    assert_eq!(
        gpu_display_identity(&resolved),
        GpuDisplayIdentity {
            headline: Some("Arc B390"),
            qualifier: Some("Intel Xe Graphics"),
        }
    );

    let mut generic = GpuMetrics::default();
    generic.brand = "Intel Xe Graphics".into();
    generic.marketing_name = Some("  ".into());
    generic.driver = Some("xe".into());
    assert_eq!(
        gpu_display_identity(&generic),
        GpuDisplayIdentity {
            headline: Some("Intel Xe Graphics"),
            qualifier: None,
        }
    );

    let mut driver_only = GpuMetrics::default();
    driver_only.driver = Some("xe".into());
    assert_eq!(
        gpu_display_identity(&driver_only),
        GpuDisplayIdentity::default(),
        "a driver name is not a hardware product identity"
    );
}

#[test]
fn graph_summary_ignores_gaps_and_uses_the_newest_finite_sample() {
    assert_eq!(
        graph_summary(&[20.0, f32::NAN, 0.0, 40.0, f32::INFINITY]),
        Some(GraphSummary {
            latest: 40.0,
            average: 20.0,
            minimum: 0.0,
            maximum: 40.0,
            sample_count: 3,
        })
    );
}

#[test]
fn graph_summary_is_absent_for_empty_or_all_gap_windows() {
    assert_eq!(graph_summary(&[]), None);
    assert_eq!(graph_summary(&[f32::NAN, f32::NEG_INFINITY]), None);
}

#[test]
fn graph_summary_keeps_a_single_real_sample_visible() {
    assert_eq!(
        graph_summary(&[f32::NAN, 7.5]),
        Some(GraphSummary {
            latest: 7.5,
            average: 7.5,
            minimum: 7.5,
            maximum: 7.5,
            sample_count: 1,
        })
    );
}

/// The page-tab label resolves through the shared i18n catalog, so it
/// follows the active language rather than a frozen English literal. Mutates
/// the process-global language, so it restores En at the end (other shell
/// tests only assert non-emptiness, which holds in any language).
#[test]
fn page_tab_label_localizes_with_the_active_language() {
    i18n::set_language(i18n::Language::En);
    let en_label = page_help()
        .iter()
        .find(|p| p.page == AppPage::Performance)
        .map(|p| p.label)
        .expect("Performance page present");
    i18n::set_language(i18n::Language::Zh);
    let zh_label = page_help()
        .iter()
        .find(|p| p.page == AppPage::Performance)
        .map(|p| p.label)
        .expect("Performance page present");
    assert_eq!(en_label, "Performance");
    assert_eq!(zh_label, i18n::t("tab.performance"));
    assert_ne!(en_label, zh_label, "zh tab label must differ from en");
    i18n::set_language(i18n::Language::En);
}

#[test]
fn device_status_keys_are_distinct_and_non_empty() {
    for status in [
        DeviceStatus::Healthy,
        DeviceStatus::Stale,
        DeviceStatus::PermissionDenied,
        DeviceStatus::MissingTool,
        DeviceStatus::Unsupported,
    ] {
        let key = device_status_i18n_key(status);
        assert!(!key.is_empty());
        assert!(key.starts_with("device."));
    }
    // The action key (footer hint) differs from the status label except for
    // the healthy sentinel, where both collapse to "device.healthy".
    assert_ne!(
        device_action_i18n_key(DeviceStatus::PermissionDenied),
        device_status_i18n_key(DeviceStatus::PermissionDenied)
    );
    assert_eq!(
        device_action_i18n_key(DeviceStatus::Healthy),
        device_status_i18n_key(DeviceStatus::Healthy)
    );
}

#[test]
fn smart_availability_keys_cover_every_variant() {
    for availability in [
        SmartAvailability::Available,
        SmartAvailability::Unsupported,
        SmartAvailability::Unavailable,
        SmartAvailability::MissingTool,
        SmartAvailability::PermissionDenied,
    ] {
        assert!(!smart_availability_i18n_key(availability).is_empty());
    }
}

#[test]
fn smart_section_hidden_when_provider_cannot_supply_readings() {
    let mut disk = DiskMetrics::default();
    assert!(
        !smart_section_visible(&disk),
        "unavailable provider with no fields must hide the SMART section"
    );
    disk.smart_availability = SmartAvailability::PermissionDenied;
    assert!(!smart_section_visible(&disk));
    disk.smart_temperature_c = Some(40.0);
    assert!(smart_section_visible(&disk), "a real reading must show");
    disk.smart_temperature_c = None;
    disk.smart_availability = SmartAvailability::Available;
    assert!(
        smart_section_visible(&disk),
        "an available provider keeps the honest status section even before scalars arrive"
    );
}

#[test]
fn effective_smart_status_prefers_authoritative_state_then_availability() {
    // Authoritative state wins regardless of availability.
    let mut disk = DiskMetrics::default();
    disk.smart_state.status = DeviceStatus::PermissionDenied;
    disk.smart_availability = SmartAvailability::Available;
    assert_eq!(
        effective_smart_status(&disk),
        DeviceStatus::PermissionDenied
    );

    // An Unsupported state falls back to the availability projection.
    disk.smart_state.status = DeviceStatus::Unsupported;
    disk.smart_availability = SmartAvailability::Available;
    assert_eq!(effective_smart_status(&disk), DeviceStatus::Healthy);
    disk.smart_availability = SmartAvailability::PermissionDenied;
    assert_eq!(
        effective_smart_status(&disk),
        DeviceStatus::PermissionDenied
    );
    disk.smart_availability = SmartAvailability::Unavailable;
    assert_eq!(effective_smart_status(&disk), DeviceStatus::Stale);
}

#[test]
fn peak_of_floors_at_current_ignores_gaps_and_stays_absent_without_data() {
    // The live reading floors an empty window.
    assert_eq!(peak_of(&[], Some(2.5)), Some(2.5));
    // Finite samples win over a lower current; gaps are skipped.
    assert_eq!(peak_of(&[1.0, f32::NAN, 4.0], Some(2.0)), Some(4.0));
    // History alone still yields an honest peak.
    assert_eq!(peak_of(&[3.0], None), Some(3.0));
    // No current value AND no finite sample: absence, never a fabricated 0.
    assert_eq!(peak_of(&[], None), None);
    assert_eq!(peak_of(&[f32::NAN], None), None);
}

#[test]
fn value_with_peak_renders_bare_value_without_peak_and_dash_without_data() {
    // Both present: the localized "{value} (peak {peak})" join. The
    // English catalog is the test-time default, matching the TUI modal's
    // "(peak" regression anchor.
    i18n::set_language(i18n::Language::En);
    assert_eq!(
        value_with_peak(Some("3.1%".into()), Some("4.0%".into())),
        "3.1% (peak 4.0%)"
    );
    // No peak history: the bare value, no fabricated suffix.
    assert_eq!(value_with_peak(Some("3.1%".into()), None), "3.1%");
    // Missing current with known peak keeps the dash for the value.
    assert_eq!(value_with_peak(None, Some("4.0%".into())), "— (peak 4.0%)");
    // Nothing collected: the shared dash placeholder.
    assert_eq!(value_with_peak(None, None), MISSING_VALUE);
}

#[test]
fn cpu_time_seconds_keeps_a_measured_zero_a_number() {
    // The terminal/Bevy cell is a whole second count, never a dash for zero.
    assert_eq!(cpu_time_seconds(0), "0s");
    assert_eq!(cpu_time_seconds(12), "12s");
    assert_eq!(cpu_time_seconds(90), "90s");
    assert_eq!(optional_cpu_time_seconds(Some(7)), "7s");
    assert_eq!(optional_cpu_time_seconds(None), MISSING_VALUE);
}

#[test]
fn cpu_time_compact_ladder_matches_every_segment_boundary() {
    // Nothing accumulated yet is the honest dash, not a believable "0s".
    assert_eq!(cpu_time_compact(0), MISSING_VALUE);
    assert_eq!(cpu_time_compact(1), "1s");
    assert_eq!(cpu_time_compact(59), "59s");
    assert_eq!(cpu_time_compact(60), "1m 0s");
    assert_eq!(cpu_time_compact(3_599), "59m 59s");
    assert_eq!(cpu_time_compact(3_600), "1h 0m");
    assert_eq!(cpu_time_compact(86_400), "1d 0h");
    // Multi-day accumulations drop the minutes segment to stay width-bounded.
    assert_eq!(cpu_time_compact(2 * 86_400 + 3_600 + 61), "2d 1h");
    // None renders the dash through the optional wrapper too.
    assert_eq!(optional_cpu_time_compact(Some(3_600)), "1h 0m");
    assert_eq!(optional_cpu_time_compact(None), MISSING_VALUE);
}

#[test]
fn wifi_signal_quality_percent_clamps_the_ninety_to_thirty_window() {
    // The shared -90..-30 dBm → 0..100 mapping, clamped on both ends.
    assert_eq!(wifi_signal_quality_percent(-90), 0.0);
    assert_eq!(wifi_signal_quality_percent(-60), 50.0);
    assert_eq!(wifi_signal_quality_percent(-30), 100.0);
    assert_eq!(wifi_signal_quality_percent(-120), 0.0);
    assert_eq!(wifi_signal_quality_percent(0), 100.0);
    // A missing dBm observation stays missing instead of becoming a fake 0%.
    assert_eq!(optional_wifi_signal_quality_percent(Some(-60)), Some(50.0));
    assert_eq!(optional_wifi_signal_quality_percent(None), None);
}

#[test]
fn virtual_interface_name_classification_matches_known_patterns() {
    let virtual_names = [
        "veth12345",
        "veth_abc",
        "docker0",
        "docker_gwbridge",
        "virbr0",
        "virbr0-nic",
        "virbr1",
        "br-6a31c50409a6",
        "br-lan",
        "vnet0",
        "vnet1",
        "tun0",
        "tun1",
        "tap0",
        "tap1",
        "lo",
        "lo0",
        "vmnet1",
        "vmnet8",
        "vboxnet0",
        "dummy0",
        "flannel.1",
        "cni0",
        "cali1234",
        "cilium_net",
        "tailscale0",
        "wg0",
        "wg-home",
        "br0",
        "br1",
        "utun0",
        "utun3",
        "vEthernet (WSL)",
        "vEthernet (Default Switch)",
        "VirtualBox Host-Only Ethernet Adapter",
        "Hyper-V Virtual Ethernet Adapter",
        "TAP-Windows Adapter V9",
    ];

    for name in virtual_names {
        assert!(
            is_virtual_network_interface(name),
            "expected {name} to be classified as virtual"
        );
        assert!(
            !is_physical_network_interface(name),
            "expected {name} not to be physical"
        );
        assert_eq!(
            classify_network_interface(name),
            NetworkInterfaceGroup::Virtual,
            "expected {name} to have group Virtual"
        );
    }
}

#[test]
fn physical_interface_name_classification_excludes_physical_nics() {
    let physical_names = [
        "eth0",
        "eth1",
        "enp3s0",
        "eno1",
        "ens33",
        "enx00e04c680123",
        "wlan0",
        "wlp2s0",
        "wlo1",
        "wwan0",
        "en0",
        "en1",
        "Intel(R) Wi-Fi 6 AX200",
        "Realtek Gaming 2.5GbE Family Controller",
        "broadcom",
    ];

    for name in physical_names {
        assert!(
            is_physical_network_interface(name),
            "expected {name} to be classified as physical"
        );
        assert!(
            !is_virtual_network_interface(name),
            "expected {name} not to be virtual"
        );
        assert_eq!(
            classify_network_interface(name),
            NetworkInterfaceGroup::Physical,
            "expected {name} to have group Physical"
        );
    }
}

#[test]
fn network_metrics_device_classification() {
    // Explicit Virtual / Loopback / Vpn adapter types are always virtual
    let mut virt_adapter = NetworkMetrics::new("any_name");
    virt_adapter.apply_observations(
        NetworkAdapterType::Virtual,
        Default::default(),
        Default::default(),
    );
    assert!(is_virtual_network_device(&virt_adapter));
    assert!(is_virtual_nic(&virt_adapter));

    let mut lo_adapter = NetworkMetrics::new("custom_lo");
    lo_adapter.apply_observations(
        NetworkAdapterType::Loopback,
        Default::default(),
        Default::default(),
    );
    assert!(is_virtual_network_device(&lo_adapter));

    let mut vpn_adapter = NetworkMetrics::new("corporate");
    vpn_adapter.apply_observations(
        NetworkAdapterType::Vpn,
        Default::default(),
        Default::default(),
    );
    assert!(is_virtual_network_device(&vpn_adapter));

    // WiFi is physical
    let mut wifi_adapter = NetworkMetrics::new("wlan0");
    wifi_adapter.apply_observations(
        NetworkAdapterType::WiFi,
        Default::default(),
        Default::default(),
    );
    assert!(is_physical_network_device(&wifi_adapter));
    assert!(is_physical_nic(&wifi_adapter));

    // Ethernet with physical name -> Physical
    let mut eth_adapter = NetworkMetrics::new("eth0");
    eth_adapter.apply_observations(
        NetworkAdapterType::Ethernet,
        Default::default(),
        Default::default(),
    );
    assert_eq!(
        classify_network_device(&eth_adapter),
        NetworkInterfaceGroup::Physical
    );

    // Ethernet with veth / docker name -> Virtual
    let mut veth_adapter = NetworkMetrics::new("veth42a8b9");
    veth_adapter.apply_observations(
        NetworkAdapterType::Ethernet,
        Default::default(),
        Default::default(),
    );
    assert_eq!(
        classify_network_device(&veth_adapter),
        NetworkInterfaceGroup::Virtual
    );

    let mut docker_adapter = NetworkMetrics::new("docker0");
    docker_adapter.apply_observations(
        NetworkAdapterType::Ethernet,
        Default::default(),
        Default::default(),
    );
    assert_eq!(
        classify_network_device(&docker_adapter),
        NetworkInterfaceGroup::Virtual
    );

    // Other with virbr / br- -> Virtual
    let mut virbr_adapter = NetworkMetrics::new("virbr0");
    virbr_adapter.apply_observations(
        NetworkAdapterType::Other,
        Default::default(),
        Default::default(),
    );
    assert_eq!(
        classify_network_device(&virbr_adapter),
        NetworkInterfaceGroup::Virtual
    );

    let mut br_adapter = NetworkMetrics::new("br-6a31c50409a6");
    br_adapter.apply_observations(
        NetworkAdapterType::Other,
        Default::default(),
        Default::default(),
    );
    assert_eq!(
        classify_network_device(&br_adapter),
        NetworkInterfaceGroup::Virtual
    );
}

#[test]
fn group_network_devices_partitions_physical_and_virtual() {
    let mut eth0 = NetworkMetrics::new("eth0");
    eth0.apply_observations(
        NetworkAdapterType::Ethernet,
        Default::default(),
        Default::default(),
    );

    let mut wlan0 = NetworkMetrics::new("wlan0");
    wlan0.apply_observations(
        NetworkAdapterType::WiFi,
        Default::default(),
        Default::default(),
    );

    let mut docker0 = NetworkMetrics::new("docker0");
    docker0.apply_observations(
        NetworkAdapterType::Ethernet,
        Default::default(),
        Default::default(),
    );

    let mut veth1 = NetworkMetrics::new("veth1234");
    veth1.apply_observations(
        NetworkAdapterType::Ethernet,
        Default::default(),
        Default::default(),
    );

    let devices = [&eth0, &docker0, &wlan0, &veth1];
    let (physical, virtual_devs) = group_network_devices(devices);

    assert_eq!(physical.len(), 2);
    assert_eq!(physical[0].interface_name.as_ref(), "eth0");
    assert_eq!(physical[1].interface_name.as_ref(), "wlan0");

    assert_eq!(virtual_devs.len(), 2);
    assert_eq!(virtual_devs[0].interface_name.as_ref(), "docker0");
    assert_eq!(virtual_devs[1].interface_name.as_ref(), "veth1234");
}

#[test]
fn network_interface_group_properties() {
    assert!(NetworkInterfaceGroup::Physical.is_physical());
    assert!(!NetworkInterfaceGroup::Physical.is_virtual());
    assert_eq!(NetworkInterfaceGroup::Physical.key(), "physical");

    assert!(NetworkInterfaceGroup::Virtual.is_virtual());
    assert!(!NetworkInterfaceGroup::Virtual.is_physical());
    assert_eq!(NetworkInterfaceGroup::Virtual.key(), "virtual");

    // Labels resolve through i18n
    i18n::set_language(i18n::Language::En);
    assert_eq!(NetworkInterfaceGroup::Physical.label(), "physical");
    assert_eq!(NetworkInterfaceGroup::Virtual.label(), "Virtual");
}

#[test]
fn service_cycle_members_are_folded_from_typed_inventory_graphs() {
    use taskmanager_core::core::services::{
        ServiceItem, ServiceRelationEdge, ServiceRelationGraph, ServiceRelationKind, ServiceStatus,
    };

    let first = ServiceItem::from_inventory(
        "linux.service.systemd:first.service",
        "first",
        ServiceStatus::Active,
        "",
        "loaded",
        "active",
        "running",
    )
    .with_relations(ServiceRelationGraph::from_edges([
        ServiceRelationEdge::new(
            ServiceRelationKind::Before,
            "linux.service.systemd:second.service",
        ),
    ]));
    let second = ServiceItem::from_inventory(
        "linux.service.systemd:second.service",
        "second",
        ServiceStatus::Active,
        "",
        "loaded",
        "active",
        "running",
    )
    .with_relations(ServiceRelationGraph::from_edges([
        ServiceRelationEdge::new(
            ServiceRelationKind::Before,
            "linux.service.systemd:first.service",
        ),
    ]));

    let members = service_cycle_members([&first, &second]);
    assert!(members.contains(&first.id));
    assert!(members.contains(&second.id));
}
