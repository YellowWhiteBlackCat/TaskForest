//! Page-level behavior tests: the pure formatting/column projections and the
//! Services / Startup / System / export / modal / overlay render paths. These
//! drive the renderer through the shared shell state without pixel read-back.

use super::super::startup_table::{
    TimelineRowKind, startup_control_text, startup_heading, startup_impact_text,
    startup_list_state, startup_rows, startup_source_text, startup_status_text, startup_timeline,
};
use super::super::system_table::{
    hardware_info_rows, hardware_list_state, npu_device_view_models, telemetry_rows,
};
use super::super::tables::{
    ListState, service_action_label, service_description, service_heading, service_list_state,
    service_rows,
};
use super::super::*;
use super::filtered_services;
use crate::theme;
use crate::ui::applications::{
    apps_columns, apps_table_width, localized_sort_column_label, sort_arrow,
    trend_header_index_for, visible_apps_columns,
};
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::services::{ServiceAction, ServiceStatus};
use taskmanager_shell::presentation::{bytes, duration};

use taskmanager_shell::page_help;
use taskmanager_shell::{SortCol, SortDir};

#[test]
fn byte_and_duration_formatting_matches_the_other_frontends() {
    assert_eq!(bytes(0), "0 B");
    assert_eq!(bytes(1536), "1.5 KiB");
    assert_eq!(bytes(2 * 1024 * 1024), "2.0 MiB");
    assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
    assert_eq!(duration(90), "00h 01m");
    assert_eq!(duration(86_400 + 3_600), "1d 01h 00m");
}

#[test]
fn npu_view_model_keeps_all_current_facts_and_marks_missing_values() {
    use taskmanager_core::core::failure::FailureKind;
    use taskmanager_core::core::identity::DeviceId;
    use taskmanager_core::core::metrics::ScalarObservation;
    use taskmanager_core::core::npu::{
        NpuDevice, NpuEngineKind, NpuInventorySnapshot, NpuMemoryReport,
    };

    let mut device = NpuDevice {
        device_id: DeviceId::new("accel0"),
        brand: Some("Intel AI Boost".into()),
        driver: Some("intel_vpu".into()),
        utilization_pct: ScalarObservation::available(42.4, 1),
        memory: NpuMemoryReport {
            dedicated_total_bytes: ScalarObservation::available(0, 1),
            shared_total_bytes: ScalarObservation::unavailable(FailureKind::Unsupported),
        },
        ..NpuDevice::default()
    };
    device.engines.push(Default::default());
    device.engines[0].kind = NpuEngineKind::Matrix;
    device.engines[0].utilization_pct = ScalarObservation::available(17.4, 1);
    let inventory = NpuInventorySnapshot::discovered(vec![device], 1);

    let models = npu_device_view_models(Some(&inventory));
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].title, "NPU accel0");
    assert_eq!(
        models[0]
            .rows
            .iter()
            .map(|row| row.value.as_str())
            .collect::<Vec<_>>(),
        ["Intel AI Boost", "intel_vpu", "42%", "17%", "0 B", "—"]
    );

    let failed = NpuInventorySnapshot::failed(FailureKind::ProviderFault, "fixture", 2);
    assert!(npu_device_view_models(Some(&failed)).is_empty());
}

#[test]
fn navigation_pages_use_real_embedded_semantic_svg_assets() {
    for help in page_help() {
        let icon = page_icon(help.page);
        assert!(
            taskmanager_icons::asset_bytes(icon).is_some(),
            "page {:?} must resolve to an embedded SVG icon",
            help.page
        );
    }
}

#[test]
fn process_affinity_modal_renders_a_bounded_focusable_cpu_grid() {
    let mut app = crate::IcedApp::demo();
    let target = app
        .shell
        .selected_process_identity()
        .expect("demo process must have an authoritative identity");
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::ProcessAffinity(Some(
            taskmanager_application::ProcessAffinityReady {
                request_id: taskmanager_platform_contract::RequestId::MIN,
                target,
                cpus: vec![0, 3],
            },
        )),
    );
    let _ = app.update(crate::app::Message::OpenProcessAffinity);

    let _view = view(&app);
    assert!(app.affinity_open());
    assert_eq!(app.logical_cpu_count(), 22);
    assert_eq!(
        crate::focus::focus_id(crate::app::FocusTarget::ProcessAffinityCpu(3)),
        "iced-process-affinity-cpu-3"
    );
    assert_eq!(
        crate::focus::focus_id(crate::app::FocusTarget::ProcessAffinityApply),
        "iced-process-affinity-apply"
    );
}

#[test]
fn memory_unit_preference_switches_bytes_and_bits() {
    // The bytes/bits ladder rides the full preference entry with the base-2
    // ladder pinned (the historical two-argument shape).
    assert_eq!(memory_text_pref(1536, true, true), "1.5 KiB");
    assert_eq!(memory_text_pref(1536, false, true), "12.0 Kib");
    assert_eq!(memory_text_pref(2 * 1024 * 1024, true, true), "2.0 MiB");
    assert_eq!(memory_text_pref(2 * 1024 * 1024, false, true), "16.0 Mib");
    assert_eq!(
        memory_text_pref(3 * 1024 * 1024 * 1024, false, true),
        "24.0 Gib"
    );
    assert_eq!(memory_text_pref(0, false, true), "0 b");
}

#[test]
fn unit_matrix_preference_switches_bytes_bits_and_base() {
    // memory_text_pref: the full GPUI Settings Units matrix.
    assert_eq!(memory_text_pref(1536, true, true), "1.5 KiB");
    assert_eq!(memory_text_pref(1536, true, false), "1.5 KB");
    assert_eq!(memory_text_pref(1536, false, false), "12.3 Kb");
    assert_eq!(memory_text_pref(2 * 1024 * 1024, false, true), "16.0 Mib");
    // quantity_text_pref: the same matrix for drive/network quantities.
    assert_eq!(quantity_text_pref(1_500_000, true, false), "1.5 MB");
    assert_eq!(quantity_text_pref(1_500_000, false, false), "12.0 Mb");
    assert_eq!(quantity_text_pref(1_500_000, true, true), "1.4 MiB");
    // The decimal ladder keeps the correct case per unit through the same
    // preference entry (bytes, base-10 / bits, base-10).
    assert_eq!(quantity_text_pref(0, true, false), "0 B");
    assert_eq!(quantity_text_pref(1500, false, false), "12.0 Kb");
    assert_eq!(quantity_text_pref(2_000_000_000, true, false), "2.0 GB");
}

#[test]
fn unavailable_performance_metrics_do_not_become_zero_percent() {
    assert_eq!(percent_text("CPU", None), "CPU —");
    // GPUI gauge parity: integer percent, space before the unit.
    assert_eq!(percent_text("CPU", Some(0.0)), "CPU 0 %");
}

/// The composition-bar segment color is the iced-specific edge of the shared
/// breakdown: each semantic kind maps onto its neutral theme token (the same
/// tokens the other frontends use), resolved through [`theme::color`]. Pinning
/// the mapping keeps every frontend on the shared semantic palette.
#[test]
fn segment_color_maps_each_kind_to_its_theme_token() {
    use taskmanager_shell::memory::MemSegmentKind;
    let theme = taskmanager_theme::Theme::dark();
    assert_eq!(
        segment_color(MemSegmentKind::Active, &theme),
        crate::theme_binding::color(theme.memory)
    );
    assert_eq!(
        segment_color(MemSegmentKind::InUse, &theme),
        crate::theme_binding::color(theme.memory)
    );
    assert_eq!(
        segment_color(MemSegmentKind::Inactive, &theme),
        crate::theme_binding::color(theme.accent)
    );
    assert_eq!(
        segment_color(MemSegmentKind::Cache, &theme),
        crate::theme_binding::color(theme.disk)
    );
    assert_eq!(
        segment_color(MemSegmentKind::Free, &theme),
        crate::theme_binding::color(theme.fg_dim)
    );
    assert_eq!(
        segment_color(MemSegmentKind::Available, &theme),
        crate::theme_binding::color(theme.fg_dim)
    );
    assert_eq!(
        segment_color(MemSegmentKind::Other, &theme),
        crate::theme_binding::color(theme.shade)
    );
}

#[test]
fn sort_arrow_marks_only_the_active_column_with_direction() {
    // Active column carries the direction marker.
    assert_eq!(
        sort_arrow((SortCol::Cpu, SortDir::Desc), SortCol::Cpu),
        Some("▼")
    );
    assert_eq!(
        sort_arrow((SortCol::Memory, SortDir::Asc), SortCol::Memory),
        Some("▲")
    );
    // Every other column stays unmarked, whether or not it has a header.
    assert_eq!(
        sort_arrow((SortCol::Cpu, SortDir::Desc), SortCol::Pid),
        None
    );
    assert_eq!(
        sort_arrow((SortCol::Cpu, SortDir::Asc), SortCol::State),
        None,
        "State has no header cell but must still project no arrow"
    );
}

#[test]
fn apps_columns_keep_swap_conditional_and_project_the_gpui_parity_set() {
    let with_swap = apps_columns(true);
    assert_eq!(
        with_swap.iter().map(|(col, _)| *col).collect::<Vec<_>>(),
        vec![
            SortCol::Pid,
            SortCol::Name,
            SortCol::Cpu,
            SortCol::Memory,
            SortCol::Pss,
            SortCol::Swap,
            SortCol::DiskRead,
            SortCol::DiskWrite,
            SortCol::CpuTime,
            SortCol::Threads,
            SortCol::User,
            // GPUI-parity advanced columns (each individually sortable).
            SortCol::State,
            SortCol::Fds,
            SortCol::Nice,
            SortCol::StartTime,
        ]
    );
    // Labels come from the shell's SortCol::label, not a duplicated literal.
    assert_eq!(with_swap[0].0.label(), "PID");

    let without_swap = apps_columns(false);
    assert!(
        !without_swap.iter().any(|(col, _)| *col == SortCol::Swap),
        "Swap column is hidden on a zero-swap host"
    );
    // The advanced parity columns appear on both host shapes.
    for col in [
        SortCol::State,
        SortCol::Fds,
        SortCol::Nice,
        SortCol::StartTime,
    ] {
        assert!(
            with_swap.iter().any(|(c, _)| *c == col),
            "{col:?} must have a header cell"
        );
        assert!(
            without_swap.iter().any(|(c, _)| *c == col),
            "{col:?} must stay visible on a zero-swap host"
        );
    }
}

#[test]
fn applications_column_menu_hides_scalars_but_keeps_name_and_trend_anchored() {
    let hidden = std::collections::HashSet::from([SortCol::Cpu, SortCol::Memory]);
    let visible = visible_apps_columns(true, &hidden);
    assert!(visible.iter().any(|(column, _)| *column == SortCol::Name));
    assert!(!visible.iter().any(|(column, _)| *column == SortCol::Cpu));
    assert!(!visible.iter().any(|(column, _)| *column == SortCol::Memory));
    assert_eq!(trend_header_index_for(&visible), 2);
}

#[test]
fn applications_table_keeps_intrinsic_width_and_localized_headers() {
    use taskmanager_application::i18n::{Language, set_language};

    let hidden = std::collections::HashSet::new();
    let wide = apps_table_width(true, &hidden);
    assert!(
        wide > 1_200.0,
        "the full process table must remain wider than the desktop viewport so the horizontal scrollbar has real content: {wide}"
    );

    let advanced = std::collections::HashSet::from([
        SortCol::DiskRead,
        SortCol::DiskWrite,
        SortCol::CpuTime,
        SortCol::Threads,
        SortCol::User,
        SortCol::State,
        SortCol::Fds,
        SortCol::Nice,
        SortCol::StartTime,
    ]);
    let compact = apps_table_width(true, &advanced);
    assert!(
        compact < wide,
        "hiding advanced columns must reduce content width"
    );

    taskmanager_test_support::pin_english();
    assert_eq!(localized_sort_column_label(SortCol::DiskRead), "Disk read");
    set_language(Language::Zh);
    assert_eq!(localized_sort_column_label(SortCol::DiskRead), "磁盘读取");
    taskmanager_test_support::pin_english();
}

#[test]
fn applications_header_renders_a_clickable_sort_target_for_every_column() {
    // The header must construct for each column being the active one, with and
    // without a swap device, proving the click surface + arrow projection do
    // not panic and stay bound to the shared sort state the table renders.
    let mut app = crate::IcedApp::demo();
    let _ = app.update(crate::app::Message::SelectPage(AppPage::Applications));
    for column in apps_columns(true).into_iter().map(|(col, _)| col) {
        app.shell.process_sort = (column, SortDir::Desc);
        let _view = view(&app);
        assert_eq!(
            sort_arrow(app.shell.process_sort, column),
            Some("▼"),
            "active column {column:?} must project the descending arrow"
        );
    }
    // A no-snapshot host keeps Swap hidden yet still renders every header.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(None)),
    );
    let _view = view(&app);
}

#[test]
fn apps_resource_projection_preserves_typed_pss_swap_and_measured_zero() {
    let mut process = taskmanager_core::core::process::ProcessItem::default();
    let mut observations = *process.scalar_observations();
    observations.memory_pss_bytes =
        taskmanager_core::core::metrics::ScalarObservation::available(512 * 1024 * 1024, 1);
    observations.swap_bytes = taskmanager_core::core::metrics::ScalarObservation::available(0, 1);
    process.apply_scalar_observations(observations);

    let cells = crate::ui::process_projection::build_row_cells_with_rules(
        &process,
        &taskmanager_core::core::time::LocalTimeRulesObservation::unsupported(0),
    );
    assert_eq!(cells.pss, "512.0 MiB");
    assert_eq!(cells.swap, "0 B");

    let mut observations = *process.scalar_observations();
    observations.memory_pss_bytes = taskmanager_core::core::metrics::ScalarObservation::default();
    observations.swap_bytes = taskmanager_core::core::metrics::ScalarObservation::default();
    process.apply_scalar_observations(observations);
    let cells = crate::ui::process_projection::build_row_cells_with_rules(
        &process,
        &taskmanager_core::core::time::LocalTimeRulesObservation::unsupported(0),
    );
    assert_eq!(
        (cells.pss.as_str(), cells.swap.as_str()),
        ("—", "—"),
        "unknown observations must not fall back to RSS or zero"
    );
}

#[test]
fn pages_cover_the_six_shared_pages() {
    let pages = page_help();
    assert_eq!(pages.len(), AppPage::ALL.len());
    for PageHelp { page, .. } in pages {
        assert!(
            AppPage::ALL.contains(&page),
            "every page enum variant must be tabbed"
        );
    }
}

#[test]
fn help_state_projects_shared_command_rows_into_the_iced_view() {
    let mut app = crate::IcedApp::demo();
    assert!(!app.shell.help_open());

    let _ = app.update(crate::app::Message::Key(crate::keys::IcedKey::Character(
        '?',
        taskmanager_application::Modifiers::NONE,
    )));
    assert!(app.shell.help_open());
    assert_eq!(
        taskmanager_shell::command_help().len(),
        taskmanager_application::CommandId::ALL.len()
    );

    // Construct the actual modal branch as a compile/runtime projection; the
    // behavior assertions above prove the shell state that selects it.
    let _view = view(&app);
}

#[test]
fn suggestions_overlay_preserves_typed_insufficient_state_in_iced() {
    let mut app = crate::IcedApp::demo();
    app.shell.dismiss_informational_overlay();
    app.shell.toggle_suggestions();

    let text = overlays::suggestion_text(
        taskmanager_core::core::alerts::AlertMetric::CpuUsagePercent,
        &app.shell,
    );
    assert!(text.contains("Insufficient"));
    assert!(text.contains("0/20") || text.contains("1/20"));
    let _view = view(&app);
}

#[test]
fn system_projection_keeps_fixture_facts_and_telemetry_separate() {
    // Pin English like the other t()-consuming tests; the labels resolve
    // through the shared catalog, so a concurrent language-flip test must not
    // change what this projection asserts.
    taskmanager_test_support::pin_english();
    let shell = taskmanager_shell::demo_app();
    let hardware = shell
        .projection()
        .hardware
        .as_ref()
        .expect("demo hardware fixture");
    let hardware_rows = hardware_info_rows(hardware, None);

    assert_eq!(
        hardware_list_state(Some(hardware), &hardware_rows),
        ListState::Ready
    );
    assert_eq!(
        hardware_rows
            .iter()
            .find(|row| row.label == t("system.field.os_name"))
            .map(|row| row.value.as_str()),
        Some("Linux")
    );
    assert_eq!(
        hardware_rows
            .iter()
            .find(|row| row.label == t("common.logical_cores"))
            .map(|row| row.value.as_str()),
        Some("22")
    );

    let snapshot = shell
        .projection()
        .snapshot
        .as_ref()
        .expect("demo snapshot fixture");
    let telemetry = telemetry_rows(snapshot);
    assert_eq!(telemetry[0].label, t("common.uptime"));
    assert_eq!(telemetry[0].value, "06h 42m");
    assert_eq!(telemetry[1].value, "347");
    assert_eq!(telemetry[2].value, "2816");
}

#[test]
fn system_projection_distinguishes_unloaded_and_empty_hardware() {
    taskmanager_test_support::pin_english();
    let unloaded = ShellApp::new();
    assert_eq!(hardware_list_state(None, &[]), ListState::Loading);

    let empty = HardwareInfo::default();
    let rows = hardware_info_rows(&empty, None);
    assert!(rows.is_empty());
    assert_eq!(hardware_list_state(Some(&empty), &rows), ListState::Empty);

    let facts = HardwareInfo {
        cpu_cores: Some(0),
        ..HardwareInfo::default()
    };
    let rows = hardware_info_rows(&facts, None);
    assert_eq!(
        rows.iter()
            .find(|row| row.label == t("common.logical_cores"))
            .map(|row| row.value.as_str()),
        Some("0")
    );
    assert_eq!(hardware_list_state(Some(&facts), &rows), ListState::Ready);
    assert!(unloaded.projection().hardware.is_none());
}

#[test]
fn system_projection_keeps_display_identity_and_mode_in_one_row() {
    use taskmanager_core::core::hardware::DisplayInfo;

    let hardware = HardwareInfo {
        displays: vec![DisplayInfo {
            connector: "HDMI-A-1".into(),
            manufacturer: Some("DEL".into()),
            model: Some("TaskPanel".into()),
            width_px: Some(2560),
            height_px: Some(1440),
            refresh_hz: Some(144.0),
            hdr_supported: Some(true),
            ..Default::default()
        }],
        ..HardwareInfo::default()
    };
    let rows = hardware_info_rows(&hardware, None);
    let value = rows
        .iter()
        .find(|row| row.label == t("system.display"))
        .map(|row| row.value.as_str())
        .expect("display row");
    assert!(value.starts_with("HDMI-A-1 · DEL TaskPanel · 2560×1440 · 144.0 Hz"));
    assert!(value.contains("HDR"));
}

#[test]
fn startup_projection_preserves_identity_status_source_and_impact_evidence() {
    // `startup_status_text` now resolves through the shared catalog, which
    // auto-detects the host locale on first use; pin English so the status
    // assertion is deterministic and independent of the host language.
    taskmanager_test_support::pin_english();

    let shell = taskmanager_shell::demo_app();
    assert_eq!(startup_list_state(&shell), ListState::Ready);

    let rows = startup_rows(&shell);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "user-service:ssh-agent.service");
    assert_eq!(startup_status_text(&rows[0]), "Enabled");
    assert_eq!(startup_impact_text(&rows[0]), "Low · 42 ms");
    assert_eq!(startup_source_text(&rows[0]), "User Service · User");
    assert_eq!(startup_control_text(&rows[0]), "Direct");
    assert_eq!(startup_impact_text(&rows[1]), "None · unmeasured");
    assert_eq!(startup_source_text(&rows[1]), "Desktop Entry · User");
}

#[test]
fn startup_projection_distinguishes_loading_and_confirmed_empty() {
    // Localized copy: pin English so the assertion is identical on every
    // runner regardless of the host locale.
    taskmanager_test_support::pin_english();
    let shell = ShellApp::new();
    assert_eq!(startup_list_state(&shell), ListState::Loading);
    assert_eq!(
        startup_heading(ListState::Loading, 0),
        "Startup · waiting for inventory…"
    );

    let mut shell = ShellApp::new();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell,
        taskmanager_shell::fixture::ProjectionSeedFact::StartupEntries(Some(Vec::new())),
    );
    assert_eq!(startup_list_state(&shell), ListState::Empty);
    assert_eq!(startup_heading(ListState::Empty, 0), "Startup · 0 reported");
}

#[path = "pages/startup_timeline.rs"]
mod startup_timeline;

#[test]
fn service_projection_preserves_fixture_rows_and_typed_status() {
    let shell = taskmanager_shell::demo_app();

    assert_eq!(service_list_state(&shell), ListState::Ready);
    let rows = service_rows(&shell);
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0].name, "NetworkManager.service");
    assert_eq!(rows[0].status, ServiceStatus::Active);
    assert_eq!(rows[4].status, ServiceStatus::Failed);
    assert_eq!(rows[4].description, "Recovery required");
}

#[test]
fn services_filter_matches_name_and_description_case_insensitively() {
    let shell = taskmanager_shell::demo_app();
    let rows = service_rows(&shell);
    assert_eq!(rows.len(), 5);

    // Empty / whitespace query keeps every row.
    assert_eq!(filtered_services(&rows, "").len(), 5);
    assert_eq!(filtered_services(&rows, "   ").len(), 5);

    // Name substring, case-insensitive (the description "Network time" also
    // matches — both columns are searched, mirroring GPUI).
    let network = filtered_services(&rows, "network");
    assert_eq!(network.len(), 2);
    assert!(
        network
            .iter()
            .any(|row| row.name == "NetworkManager.service")
    );
    assert!(
        network
            .iter()
            .any(|row| row.name == "systemd-timesyncd.service")
    );

    // Description substring (a service whose description matches but whose
    // name does not is still found — GPUI filters both columns).
    let recovery = filtered_services(&rows, "recovery");
    assert!(
        recovery
            .iter()
            .any(|row| row.status == ServiceStatus::Failed)
    );

    // No match → empty (the page renders its empty-filter state honestly).
    assert!(filtered_services(&rows, "no-such-service").is_empty());
}

#[test]
fn services_search_message_stays_frontend_local_and_renders_filtered() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(crate::app::Message::SelectPage(AppPage::Services));
    let _ = app.update(crate::app::Message::ServicesSearchChanged("network".into()));
    assert_eq!(app.services_query(), "network");
    // The shared process query is untouched by the page-local filter.
    assert_eq!(app.shell.query, "");
    {
        let view = crate::ui::view(&app);
        let _ = view;
    }
    let _ = app.update(crate::app::Message::ServicesSearchChanged("".into()));
    assert_eq!(app.services_query(), "");
}

#[test]
fn service_projection_distinguishes_loading_empty_and_missing_description() {
    // Localized copy: pin English so the assertion is identical on every
    // runner regardless of the host locale.
    taskmanager_test_support::pin_english();
    let shell = ShellApp::new();
    assert_eq!(service_list_state(&shell), ListState::Loading);
    assert!(service_rows(&shell).is_empty());
    assert_eq!(
        service_heading(ListState::Loading, 0),
        "Services · waiting for inventory…"
    );

    let mut shell = ShellApp::new();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(Vec::new())),
    );
    assert_eq!(service_list_state(&shell), ListState::Empty);
    assert_eq!(
        service_heading(ListState::Empty, 0),
        "Services · 0 reported"
    );

    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(vec![
            taskmanager_core::core::services::ServiceItem::from_inventory(
                "",
                "no-description.service",
                ServiceStatus::Unknown,
                "",
                "",
                "",
                "",
            ),
        ])),
    );
    assert_eq!(service_list_state(&shell), ListState::Ready);
    let rows = service_rows(&shell);
    assert_eq!(service_description(&rows[0].description), "—");
    assert_eq!(
        service_heading(ListState::Ready, rows.len()),
        "Services · 1 reported"
    );
}

#[test]
fn search_highlight_segments_flow_from_the_shared_shell_filter() {
    let mut shell = taskmanager_shell::demo_app();
    shell.open_search();
    shell.query = "a".into();

    let rows = shell.visible_processes();
    let analyzer = rows
        .iter()
        .find(|process| process.name == "rust-analyzer")
        .expect("demo fixture must keep rust-analyzer under query \"a\"");
    assert_eq!(
        crate::ui::components::highlight::highlight_segments(&analyzer.name, &shell.query),
        vec![
            ("rust-".to_string(), false),
            ("a".to_string(), true),
            ("n".to_string(), false),
            ("a".to_string(), true),
            ("lyzer".to_string(), false),
        ]
    );

    // A row the filter keeps for another reason must still highlight by name:
    // NetworkManager matches the query in its name, not only via the user.
    let manager = rows
        .iter()
        .find(|process| process.name == "NetworkManager")
        .expect("demo fixture keeps NetworkManager under query \"a\"");
    assert_eq!(
        crate::ui::components::highlight::highlight_segments(&manager.name, &shell.query),
        vec![
            ("NetworkM".to_string(), false),
            ("a".to_string(), true),
            ("n".to_string(), false),
            ("a".to_string(), true),
            ("ger".to_string(), false),
        ]
    );

    // The full Applications page renders with search active and highlighted
    // Name cells (the default page is Performance, so select it explicitly).
    let mut app = crate::IcedApp::demo();
    app.shell.open_search();
    app.shell.query = "system".into();
    let _ = app.update(crate::app::Message::SelectPage(AppPage::Applications));
    let _view = view(&app);
}

#[test]
fn export_without_an_injected_host_worker_reports_unavailable() {
    taskmanager_test_support::pin_english();
    let mut app = crate::IcedApp::demo();
    let _task = app.update(crate::app::Message::ExportSnapshot);
    let feedback = app.shell.feedback_notice().expect("export feedback set");
    assert_eq!(
        feedback.severity(),
        taskmanager_shell::FeedbackSeverity::Error
    );
    assert_eq!(feedback.text(), "Snapshot export is unavailable");
}

#[test]
fn export_without_data_is_an_honest_message_not_a_failure() {
    taskmanager_test_support::pin_english();
    let mut app = crate::IcedApp::default();
    let _task = app.update(crate::app::Message::ExportSnapshot);
    let feedback = app.shell.feedback_notice().expect("feedback");
    assert_eq!(
        feedback.severity(),
        taskmanager_shell::FeedbackSeverity::Warning
    );
    assert_eq!(feedback.text(), "No snapshot data to export yet");
}

#[test]
fn startup_page_renders_the_enable_disable_toggle_and_routes_through_shell() {
    taskmanager_test_support::pin_english();

    let mut app = crate::IcedApp::demo();
    let _ = app.update(crate::app::Message::SelectPage(AppPage::Startup));

    // The demo fixture carries two startup entries; rendering the page for each
    // selection must build the contextual Enable/Disable action bar without
    // panic (both the enabled and the disabled entry branches).
    let shell = taskmanager_shell::demo_app();
    let rows = startup_rows(&shell);
    assert_eq!(rows.len(), 2, "demo fixture carries two startup entries");
    for index in 0..rows.len() {
        app.shell.selected = index;
        let _view = view(&app);
    }

    // The toggle gates behind a confirmation (mirrors GPUI): the request sets
    // the pending slot without firing; confirm emits the effect, which demo
    // mode honestly suppresses rather than executing.
    let _ = app.update(crate::app::Message::RequestStartupControl(true));
    assert!(
        app.shell.pending_startup().is_some(),
        "startup toggle must gate behind pending_startup"
    );
    let _ = app.update(crate::app::Message::ConfirmStartupControl);
    assert!(
        app.shell.feedback_text().contains("Demo mode"),
        "confirm must submit the gated startup control: {}",
        app.shell.feedback_text()
    );
    taskmanager_test_support::pin_english();
}

#[test]
fn services_page_projects_rows_and_action_labels_for_every_variant() {
    // `service_action_label` now resolves through the shared catalog, which
    // auto-detects the host locale on first use; pin English so the label
    // assertion is deterministic and independent of the host language.
    taskmanager_test_support::pin_english();

    for action in [
        taskmanager_core::core::services::ServiceAction::Start,
        taskmanager_core::core::services::ServiceAction::Stop,
        taskmanager_core::core::services::ServiceAction::Restart,
        taskmanager_core::core::services::ServiceAction::Enable,
        taskmanager_core::core::services::ServiceAction::Disable,
    ] {
        let label = service_action_label(action);
        assert!(!label.is_empty());
    }
    assert_eq!(service_action_label(ServiceAction::Restart), "Restart");

    let mut app = crate::IcedApp::demo();
    let _ = app.update(crate::app::Message::SelectPage(AppPage::Services));
    let _view = view(&app);

    let shell = taskmanager_shell::demo_app();
    let rows = service_rows(&shell);
    assert_eq!(rows.len(), 5);
    assert_eq!(service_list_state(&shell), ListState::Ready);
}

#[test]
fn zebra_rows_compose_across_the_four_inventory_tables() {
    // The zebra parity seam drives the row surface of every inventory table;
    // the demo fixture must exercise multiple striped rows per table, and each
    // page must still compose with stripes for its full row set.
    let shell = taskmanager_shell::demo_app();
    assert!(
        shell.visible_processes().len() >= 4
            && service_rows(&shell).len() >= 4
            && startup_rows(&shell).len() >= 2
            && shell
                .projection()
                .sessions
                .as_deref()
                .is_some_and(|sessions| sessions.len() >= 2),
        "the demo fixture must exercise multiple striped rows per table"
    );
    // Even 0-based rows stripe, odd rows stay plain — the parity the row
    // containers are styled through (rows 0, 2, ... wear the stripe).
    assert!(theme::zebra_index(0) && theme::zebra_index(2));
    assert!(!theme::zebra_index(1) && !theme::zebra_index(3));

    let mut app = crate::IcedApp::demo();
    for page in [
        AppPage::Applications,
        AppPage::Services,
        AppPage::Startup,
        AppPage::Users,
    ] {
        let _ = app.update(crate::app::Message::SelectPage(page));
        let _view = view(&app); // striped rows compose; render-and-drop.
    }
}

#[test]
fn shared_page_body_strings_follow_the_active_language() {
    // The Services-page action labels are now resolved through the shared
    // catalog (`taskmanager_application::i18n::t`), which reads a process-wide
    // language global; `service_action_label` is the pure-function seam both
    // the confirm bar and the per-row buttons render. Driving the global and
    // asserting the resolved text proves the shared-page chrome follows the
    // active language rather than a hard-coded English literal. The `Language`
    // here is the *shared* one (the catalog resolver), distinct from the
    // renderer-local `crate::i18n::Language`; `IcedApp` mirrors the picker into
    // it via `i18n::sync_shared_languages`.
    use taskmanager_application::i18n::{Language, set_language};

    taskmanager_test_support::pin_english();
    assert_eq!(service_action_label(ServiceAction::Start), "Start");
    assert_eq!(service_action_label(ServiceAction::Stop), "Stop");
    assert_eq!(service_action_label(ServiceAction::Restart), "Restart");

    set_language(Language::Zh);
    assert_eq!(service_action_label(ServiceAction::Start), "启动");
    assert_eq!(service_action_label(ServiceAction::Stop), "停止");
    assert_eq!(service_action_label(ServiceAction::Restart), "重启");

    // Restore En so the rest of the suite (which assumes the English default)
    // is unaffected by this global mutation.
    taskmanager_test_support::pin_english();
}

#[test]
fn service_control_bar_replaces_the_action_hint_while_pending() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(crate::app::Message::SelectPage(AppPage::Services));
    let _ = app.update(crate::app::Message::RequestServiceAction {
        index: 0,
        action: taskmanager_core::core::services::ServiceAction::Restart,
    });
    assert!(app.shell.pending_service_control().is_some());
    assert_eq!(
        app.shell
            .pending_service_control()
            .map(|target| target.action),
        Some(taskmanager_core::core::services::ServiceAction::Restart)
    );
    let _view = view(&app);
}

#[test]
fn local_modals_render_on_top_of_every_page() {
    let mut app = crate::IcedApp::demo();
    for open in [
        crate::app::Message::OpenSettings,
        crate::app::Message::OpenAbout,
        crate::app::Message::OpenHealth,
        crate::app::Message::OpenContainers,
    ] {
        let _ = app.update(crate::app::Message::DismissOverlay);
        let _ = app.update(open);
        let _view = view(&app);
    }
}

#[path = "pages/process_details.rs"]
mod process_details;
