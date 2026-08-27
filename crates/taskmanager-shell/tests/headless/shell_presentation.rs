use super::*;

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
        assert_eq!(
            page.command.action(),
            taskmanager_application::AppAction::SelectPage(page.page)
        );
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
fn nice_formats_purely_without_a_wall_clock() {
    // Nice signs a positive priority and leaves zero/negative bare.
    assert_eq!(optional_nice(Some(10)), "+10");
    assert_eq!(optional_nice(Some(0)), "0");
    assert_eq!(optional_nice(Some(-5)), "-5");
    assert_eq!(optional_nice(None), "—");
}

#[test]
fn local_clock_requires_injected_rules() {
    let unavailable = taskmanager_application::LocalTimeRulesObservation::unsupported(1);
    assert_eq!(start_clock_local(Some(10_921), &unavailable), "—");
    assert_eq!(local_timestamp(10_921_000, &unavailable), "—");

    let utc = taskmanager_application::LocalTimeRulesObservation::current(
        taskmanager_application::LocalTimeRules::utc(),
        1,
    );
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
