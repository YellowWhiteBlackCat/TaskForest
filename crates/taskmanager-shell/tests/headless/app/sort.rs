//! Process-table and inventory sort coverage, split out of the main tests
//! module so the file stays under the source-line ceiling.
use super::super::*;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::ProcessScalarObservations;
use taskmanager_core::core::services::ServiceStatus;
use taskmanager_core::core::startup::{
    StartupControlPolicy, StartupEntry, StartupEntryId, StartupEntryLocator, StartupImpact,
    StartupImpactEvidence, StartupImpactUnknownReason, StartupScope, StartupSource,
};

#[test]
fn default_sort_matches_historic_cpu_descending_primary_key() {
    let app = crate::demo_app();
    assert_eq!(app.process_sort, (SortCol::Cpu, SortDir::Desc));
    let rows = app.visible_processes();
    assert_eq!(rows.first().map(|row| row.pid), Some(4201));
}

#[test]
fn cycle_sort_column_reorders_rows_and_resets_the_cursor() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    app.selected = 3;
    app.process_sort = (SortCol::Pid, SortDir::Asc);
    assert_eq!(app.visible_processes().first().map(|row| row.pid), Some(1));

    app.process_sort = (SortCol::State, SortDir::Asc);
    app.cycle_sort_column();
    assert_eq!(app.process_sort, (SortCol::Pid, SortDir::Asc));
    assert_eq!(app.selected, 0);
    assert!(app.feedback_text().contains("PID"));
}

#[test]
fn toggle_sort_direction_flips_without_changing_column() {
    let mut app = crate::demo_app();
    app.process_sort = (SortCol::Memory, SortDir::Desc);
    let desc_first = app.visible_processes().first().map(|row| row.pid);

    app.toggle_sort_direction();
    assert_eq!(app.process_sort, (SortCol::Memory, SortDir::Asc));
    let asc_first = app.visible_processes().first().map(|row| row.pid);
    assert!(desc_first.is_some());
    assert!(asc_first.is_some());
    assert!(app.feedback_text().contains("ascending"));
}

#[test]
fn pss_and_swap_sort_use_typed_current_values_without_rss_fallbacks() {
    let mut app = ShellApp::new();
    app.application.active_page = AppPage::Applications;

    let mut low = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(10)
        .name("low".into())
        .current_memory_bytes(9 * 1024)
        .build();
    let mut low_observations = *low.scalar_observations();
    low_observations.memory_pss_bytes = ScalarObservation::available(100, 1);
    low_observations.swap_bytes = ScalarObservation::available(4, 1);
    low.apply_scalar_observations(low_observations);

    let mut high = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(20)
        .name("high".into())
        .current_memory_bytes(1)
        .build();
    let mut high_observations = *high.scalar_observations();
    high_observations.memory_pss_bytes = ScalarObservation::available(200, 1);
    high_observations.swap_bytes = ScalarObservation::available(8, 1);
    high.apply_scalar_observations(high_observations);

    app.data.processes = Some(vec![low, high].into());
    app.process_sort = (SortCol::Pss, SortDir::Desc);
    assert_eq!(app.visible_processes()[0].pid, 20);

    app.process_sort = (SortCol::Swap, SortDir::Desc);
    assert_eq!(app.visible_processes()[0].pid, 20);
}

#[test]
fn advanced_sort_columns_use_typed_current_values_and_skip_the_display_cycle() {
    let mut app = ShellApp::new();
    app.application.active_page = AppPage::Applications;

    let mut low = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(10)
        .name("low".into())
        .build();
    low.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(1, 1),
        threads: ScalarObservation::available(2, 1),
        cpu_time_secs: ScalarObservation::available(100, 1),
        disk_read_bytes_per_sec: ScalarObservation::available(1024, 1),
        disk_write_bytes_per_sec: ScalarObservation::available(2048, 1),
        ..Default::default()
    });

    let mut high = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(20)
        .name("high".into())
        .build();
    high.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(2, 1),
        threads: ScalarObservation::available(8, 1),
        cpu_time_secs: ScalarObservation::available(500, 1),
        disk_read_bytes_per_sec: ScalarObservation::available(8192, 1),
        disk_write_bytes_per_sec: ScalarObservation::available(4096, 1),
        ..Default::default()
    });

    app.data.processes = Some(vec![low, high.clone()].into());

    for (column, label) in [
        (SortCol::Threads, "Threads"),
        (SortCol::CpuTime, "CPU time"),
        (SortCol::DiskRead, "Disk R/s"),
        (SortCol::DiskWrite, "Disk W/s"),
    ] {
        app.process_sort = (column, SortDir::Desc);
        assert_eq!(
            app.visible_processes()[0].pid,
            20,
            "{label} descending puts the higher-valued row first"
        );
        app.process_sort = (column, SortDir::Asc);
        assert_eq!(
            app.visible_processes()[0].pid,
            10,
            "{label} ascending puts the lower-valued row first"
        );
    }

    // The advanced columns are excluded from the core display cycle: cycling
    // from one restarts at Pid rather than visiting another advanced column
    // (a terminal cannot display them).
    app.process_sort = (SortCol::DiskRead, SortDir::Desc);
    app.cycle_sort_column();
    assert_eq!(app.process_sort.0, SortCol::Pid);

    // An unobserved value (None) sorts below a measured one so a provider
    // failure never wins the top of a descending list.
    let mut missing = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(30)
        .name("missing".into())
        .build();
    missing.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(3, 1),
        ..Default::default()
    });
    app.data.processes = Some(vec![missing, high].into());
    app.process_sort = (SortCol::Threads, SortDir::Desc);
    assert_eq!(
        app.visible_processes()[0].pid,
        20,
        "an unobserved thread count never outranks a measured one"
    );
}

#[test]
fn inventory_sorts_start_from_provider_order_and_apply_per_table() {
    let mut app = ShellApp::new();
    app.data.services = Some(vec![
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
        ServiceItem::from_inventory("", "docker.service", ServiceStatus::Failed, "", "", "", ""),
    ]);
    app.data.sessions = Some(vec![
        SessionItem {
            id: "3".into(),
            uid: 1000,
            user: "root".into(),
            seat: None,
            tty: None,
            remote: false,
            timestamp: None,
        },
        SessionItem {
            id: "9".into(),
            uid: 1000,
            user: "alice".into(),
            seat: Some("seat1".into()),
            tty: None,
            remote: false,
            timestamp: None,
        },
    ]);

    // Provider order is preserved until the user picks a column.
    assert_eq!(
        app.sorted_services()
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        vec!["zed.service", "apparmor.service", "docker.service"]
    );
    assert_eq!(
        app.sorted_sessions()
            .iter()
            .map(|row| row.user.as_str())
            .collect::<Vec<_>>(),
        vec!["root", "alice"]
    );

    // Name ascending.
    app.set_info_sort(InfoTable::Services, InfoSortCol::Name);
    assert_eq!(app.services_sort, Some((InfoSortCol::Name, SortDir::Asc)));
    assert_eq!(
        app.sorted_services()
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        vec!["apparmor.service", "docker.service", "zed.service"]
    );

    // Status rank: active before inactive before failed.
    app.set_info_sort(InfoTable::Services, InfoSortCol::Status);
    assert_eq!(
        app.sorted_services()
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        vec!["apparmor.service", "zed.service", "docker.service"]
    );

    // Same-column click toggles the direction; other tables keep their own.
    app.set_info_sort(InfoTable::Services, InfoSortCol::Status);
    assert_eq!(
        app.services_sort,
        Some((InfoSortCol::Status, SortDir::Desc))
    );
    assert_eq!(
        app.sorted_services()
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        vec!["docker.service", "zed.service", "apparmor.service"]
    );
    assert_eq!(app.sessions_sort, None);

    // Users-table session sort.
    app.set_info_sort(InfoTable::Users, InfoSortCol::Session);
    assert_eq!(
        app.sorted_sessions()
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["3", "9"]
    );
    app.set_info_sort(InfoTable::Users, InfoSortCol::Session);
    assert_eq!(
        app.sorted_sessions()
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["9", "3"]
    );
}

#[test]
fn indexed_inventory_sorts_preserve_provider_identity_without_pointer_scans() {
    let mut app = ShellApp::new();
    app.data.services = Some(vec![
        ServiceItem::from_inventory("", "z.service", ServiceStatus::Unknown, "", "", "", ""),
        ServiceItem::from_inventory("", "a.service", ServiceStatus::Unknown, "", "", "", ""),
    ]);
    app.data.startup_entries = Some(vec![
        StartupEntry {
            id: StartupEntryId::new("z"),
            name: "Zed".into(),
            exec: "z".into(),
            enabled: true,
            source: StartupSource::UserService,
            scope: StartupScope::User,
            control_policy: StartupControlPolicy::Direct,
            locator: StartupEntryLocator::new("z"),
            impact: StartupImpact::None,
            impact_evidence: StartupImpactEvidence::Unknown {
                reason: StartupImpactUnknownReason::NotInstrumented,
            },
        },
        StartupEntry {
            id: StartupEntryId::new("a"),
            name: "Alpha".into(),
            exec: "a".into(),
            enabled: false,
            source: StartupSource::UserService,
            scope: StartupScope::User,
            control_policy: StartupControlPolicy::Direct,
            locator: StartupEntryLocator::new("a"),
            impact: StartupImpact::None,
            impact_evidence: StartupImpactEvidence::Unknown {
                reason: StartupImpactUnknownReason::NotInstrumented,
            },
        },
    ]);
    app.data.sessions = Some(vec![
        SessionItem {
            id: "9".into(),
            ..SessionItem::default()
        },
        SessionItem {
            id: "3".into(),
            ..SessionItem::default()
        },
    ]);

    app.set_info_sort(InfoTable::Services, InfoSortCol::Name);
    app.set_info_sort(InfoTable::Startup, InfoSortCol::Name);
    app.set_info_sort(InfoTable::Users, InfoSortCol::Session);

    assert_eq!(app.sorted_service_indices(), vec![1, 0]);
    assert_eq!(app.sorted_startup_indices(), vec![1, 0]);
    assert_eq!(app.sorted_session_indices(), vec![1, 0]);
    assert_eq!(
        app.sorted_services()
            .into_iter()
            .map(|service| service.name.as_str())
            .collect::<Vec<_>>(),
        vec!["a.service", "z.service"]
    );
}

#[test]
fn cycle_info_sort_column_walks_the_tables_own_columns_and_wraps() {
    use taskmanager_core::core::services::ServiceStatus;
    let mut app = crate::demo_app();
    // Provider order: cycle starts at the first column of the table's cycle.
    app.cycle_info_sort_column(InfoTable::Services);
    assert_eq!(
        app.services_sort,
        Some((InfoSortCol::Name, SortDir::Asc)),
        "a never-sorted table starts at the first cycle column"
    );
    // Name -> Status -> wrap back to Name.
    app.cycle_info_sort_column(InfoTable::Services);
    assert_eq!(app.services_sort, Some((InfoSortCol::Status, SortDir::Asc)));
    app.cycle_info_sort_column(InfoTable::Services);
    assert_eq!(
        app.services_sort,
        Some((InfoSortCol::Name, SortDir::Asc)),
        "the cycle wraps around"
    );

    // The Users cycle has three columns and never offers Status (a user row
    // has no service status). None -> Name -> Session -> Seat on three presses;
    // a fourth wraps back to Name.
    app.cycle_info_sort_column(InfoTable::Users);
    app.cycle_info_sort_column(InfoTable::Users);
    app.cycle_info_sort_column(InfoTable::Users);
    assert_eq!(
        app.sessions_sort,
        Some((InfoSortCol::Seat, SortDir::Asc)),
        "Users cycles Name -> Session -> Seat"
    );
    app.cycle_info_sort_column(InfoTable::Users);
    assert_eq!(app.sessions_sort, Some((InfoSortCol::Name, SortDir::Asc)));
    // And the sorted rows actually reorder with the picked column.
    let by_name = app.sorted_sessions();
    assert_eq!(
        by_name.first().map(|session| session.user.as_str()),
        Some("devuser"),
        "Name sort orders the fixture sessions"
    );
    let _ = ServiceStatus::Active;
}

#[test]
fn toggle_info_sort_direction_flips_and_starts_from_the_cycle_head() {
    let mut app = crate::demo_app();
    // Provider order (no slot): S starts from the first column, descending.
    app.toggle_info_sort_direction(InfoTable::Startup);
    assert_eq!(
        app.startup_sort,
        Some((InfoSortCol::Name, SortDir::Desc)),
        "a never-sorted table starts descending from the first column"
    );
    // Flip again: same column, ascending.
    app.toggle_info_sort_direction(InfoTable::Startup);
    assert_eq!(app.startup_sort, Some((InfoSortCol::Name, SortDir::Asc)));
    // The startup rows reorder: Name ascending puts the fixture's lexicographic
    // first entry at the top.
    let rows = app.sorted_startup_entries();
    assert_eq!(
        rows.first().map(|entry| entry.name.as_str()),
        Some("Clipboard Sync"),
        "Name ascending orders the startup fixture"
    );
}

#[test]
fn inventory_sort_status_names_the_direction_like_the_process_table() {
    let mut app = crate::demo_app();
    // A fresh sort starts ascending and reports the direction.
    app.cycle_info_sort_column(InfoTable::Services);
    assert!(
        app.feedback_text().contains("ascending"),
        "fresh sort names the direction: {}",
        app.feedback_text()
    );
    // S flips the direction and the status follows.
    app.toggle_info_sort_direction(InfoTable::Services);
    assert!(
        app.feedback_text().contains("descending"),
        "direction flip updates the status: {}",
        app.feedback_text()
    );
    // set_info_sort on a picked column toggling also reports the new direction.
    app.set_info_sort(InfoTable::Services, InfoSortCol::Name);
    let first = app.feedback_text().to_owned();
    app.set_info_sort(InfoTable::Services, InfoSortCol::Name);
    assert_ne!(
        app.feedback_text(),
        first,
        "re-clicking the column flips direction"
    );
    assert!(
        app.feedback_text().contains("ascending") || app.feedback_text().contains("descending")
    );
}
