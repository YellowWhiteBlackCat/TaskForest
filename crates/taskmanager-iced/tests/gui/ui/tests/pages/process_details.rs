//! Process-properties modal projection tests (the frozen/live identity split
//! and the typed `current_*` observation reads), extracted from `pages.rs` to
//! keep both files under the source-line ceiling.

use super::*;
use taskmanager_core::core::process::ProcessLiveKey;

#[test]
fn process_details_overlay_projects_frozen_and_live_facts() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut app = crate::IcedApp::demo();
    app.shell.application.active_page = AppPage::Applications;
    assert!(app.shell.select_row(0));
    let _ = app
        .shell
        .apply_action(taskmanager_application::AppAction::OpenProperties);
    let _view = view(&app);

    let shell = taskmanager_shell::demo_app();
    let identity = shell
        .projection()
        .processes
        .as_deref()
        .and_then(|processes| {
            processes
                .iter()
                .find(|process| process.pid == 4201)
                .and_then(ProcessLiveKey::from_process)
        })
        .expect("demo process identity");
    let rows = overlays::property_rows(identity, &shell);
    assert_eq!(
        rows.iter()
            .find(|(label, _)| label == "Name")
            .map(|(_, value)| value.as_str()),
        Some("zed")
    );
    assert_eq!(
        rows.iter()
            .find(|(label, _)| label == "PID")
            .map(|(_, value)| value.as_str()),
        Some("4201")
    );
    assert_eq!(
        rows.iter()
            .find(|(label, _)| label == "User")
            .map(|(_, value)| value.as_str()),
        Some("devuser")
    );

    // A stale identity yields frozen facts only — never a fabricated row.
    let stale = overlays::property_rows(
        ProcessLiveKey::from_parts(999_999, 1).expect("stale identity"),
        &shell,
    );
    assert_eq!(
        stale
            .iter()
            .find(|(label, _)| label == "Status")
            .map(|(_, value)| value.as_str()),
        Some("The process is no longer running")
    );
}

/// The properties overlay reads canonical `current_*` observations: an
/// unavailable measurement renders an honest "—", while measured zero stays
/// a real value.
#[test]
fn process_details_rows_read_canonical_observations() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let shell = taskmanager_shell::demo_app();
    let zed = &shell
        .projection()
        .processes
        .as_ref()
        .expect("demo processes")[0];

    // The live rows keep only their current typed measurements. Fields not
    // observed by the demo fixture render "—".
    let zed_identity = ProcessLiveKey::from_process(zed).expect("zed identity");
    let rows = overlays::property_rows(zed_identity, &shell);
    let value = |label: &str| {
        rows.iter()
            .find(|(row_label, _)| row_label == label)
            .map(|(_, value)| value.as_str())
            .unwrap_or("\u{ab}row absent\u{bb}")
    };
    // `{cpu:>5.1}%` keeps the original alignment padding.
    assert_eq!(value("CPU"), " 24.8%");
    assert!(
        value("Memory").ends_with('B'),
        "memory renders a quantity: {}",
        value("Memory")
    );
    assert_eq!(value("FDs"), "—");

    // Typed observations remain authoritative even when the provider-native
    // start token is unavailable.
    let bare = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .name("token-unavailable".into())
        .pid(4_242)
        .metadata_observations(
            taskmanager_core::core::process::ProcessMetadataObservations::current(
                taskmanager_core::core::process::ProcessOwner::opaque("root"),
                None,
                1,
            ),
        )
        .status("Sleeping".into())
        .current_cpu_percentage(11.0)
        .current_memory_bytes(512 * 1024 * 1024)
        .current_fds(0)
        .current_nice(-5)
        .current_threads(9)
        .current_cpu_time_secs(120)
        .build();
    let mut shell_bare = taskmanager_shell::demo_app();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell_bare,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![bare.clone()])),
    );
    // The visible-projection memo keys on process_revision + source length;
    // replacing the list changes the length, and the explicit bump documents
    // the intent regardless of future memo-key changes.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell_bare,
        taskmanager_shell::fixture::ProjectionSeedFact::AdvanceRevision(
            taskmanager_shell::fixture::ProjectionSeedDomain::Processes,
        ),
    );
    let bare_identity = ProcessLiveKey::from_process(&bare).expect("bare identity");
    let bare_rows = overlays::property_rows(bare_identity, &shell_bare);
    let bare_value = |label: &str| {
        bare_rows
            .iter()
            .find(|(row_label, _)| row_label == label)
            .map(|(_, value)| value.as_str())
            .unwrap_or("\u{ab}row absent\u{bb}")
    };
    assert_eq!(bare_value("FDs"), "0");
    assert_eq!(bare_value("Nice"), "-5");
    assert_eq!(bare_value("CPU time"), "00h 02m");
    assert_eq!(bare_value("Threads"), "9");
    assert_eq!(bare_value("CPU"), " 11.0%");
    assert_eq!(bare_value("Name"), "token-unavailable");
    assert_eq!(bare_value("User"), "root");
}

#[test]
fn toolbar_and_service_rows_register_operation_ids_for_keyboard_reachability() {
    let ids: Vec<String> = [
        crate::app::FocusTarget::SettingsTrigger,
        crate::app::FocusTarget::ContainersTrigger,
        crate::app::FocusTarget::HealthTrigger,
        crate::app::FocusTarget::AboutTrigger,
        crate::app::FocusTarget::Export,
        crate::app::FocusTarget::ConfirmServiceControl,
        crate::app::FocusTarget::CancelServiceControl,
        crate::app::FocusTarget::ServiceAction {
            index: 3,
            action: taskmanager_core::core::services::ServiceAction::Stop,
        },
        crate::app::FocusTarget::SettingsChoice {
            section: "mode",
            index: 2,
        },
    ]
    .into_iter()
    .map(crate::focus::focus_id)
    .collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len());
    assert_eq!(
        crate::focus::focus_id(crate::app::FocusTarget::SettingsTrigger),
        "iced-settings-trigger"
    );
    assert_eq!(
        crate::focus::focus_id(crate::app::FocusTarget::ServiceAction {
            index: 3,
            action: taskmanager_core::core::services::ServiceAction::Stop,
        }),
        "iced-service-action-3-Stop"
    );
}

#[test]
fn info_header_sort_message_routes_to_the_shared_shell_sort_slot() {
    use taskmanager_core::core::services::{ServiceItem, ServiceStatus};

    use taskmanager_shell::{InfoSortCol, InfoTable};

    let mut app = crate::IcedApp::demo();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(vec![
            ServiceItem::from_inventory("", "zed.service", ServiceStatus::Inactive, "", "", "", ""),
            ServiceItem::from_inventory(
                "",
                "apparmor.service",
                ServiceStatus::Active,
                "",
                "",
                "",
                "",
            ),
        ])),
    );
    assert_eq!(app.shell.services_sort, None);
    assert_eq!(
        app.shell
            .sorted_services()
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        vec!["zed.service", "apparmor.service"]
    );

    // The header click message lands in the shell's Services sort slot and the
    // projected row order follows (single source, not an iced-local sort).
    let _ = app.update(Message::SortInfoTable {
        table: InfoTable::Services,
        column: InfoSortCol::Name,
    });
    assert_eq!(
        app.shell.services_sort,
        Some((InfoSortCol::Name, SortDir::Asc))
    );
    assert_eq!(
        app.shell
            .sorted_services()
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        vec!["apparmor.service", "zed.service"]
    );

    // Clicking the active column again toggles direction.
    let _ = app.update(Message::SortInfoTable {
        table: InfoTable::Services,
        column: InfoSortCol::Name,
    });
    assert_eq!(
        app.shell.services_sort,
        Some((InfoSortCol::Name, SortDir::Desc))
    );
}
