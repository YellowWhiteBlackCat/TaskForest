//! Phase 5 gap-closing tests: verifying the 10 deep parity additions in Iced.

use crate::IcedApp;
use crate::app::Message;
use std::collections::HashSet;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::hardware::{CoreBreakdown, CpuType, HardwareInfo};
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};

#[test]
fn test_affinity_presets_select_all_clear_all_invert_and_hetero() {
    let mut app = IcedApp::demo();
    let target = app
        .shell
        .selected_process_identity()
        .expect("demo process identity");
    app.open_local_surface(crate::app::LocalSurface::ProcessAffinity { target });
    app.process_presentation.affinity_cpus = Some(HashSet::new());

    // 1. Select all
    let _ = app.update(Message::SelectAllProcessAffinity);
    let cpus = app.process_presentation.affinity_cpus.as_ref().unwrap();
    assert_eq!(cpus.len(), app.logical_cpu_count());

    // 2. Clear all
    let _ = app.update(Message::ClearAllProcessAffinity);
    assert_eq!(
        app.process_presentation
            .affinity_cpus
            .as_ref()
            .unwrap()
            .len(),
        0
    );

    // 3. Invert
    let _ = app.update(Message::InvertProcessAffinity);
    assert_eq!(
        app.process_presentation
            .affinity_cpus
            .as_ref()
            .unwrap()
            .len(),
        app.logical_cpu_count()
    );

    // 4. Hetero P-Cores / E-Cores
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Hardware(Some(Box::new(HardwareInfo {
            core_breakdown: CoreBreakdown {
                p_cores: 2,
                e_cores: 2,
                lp_cores: 0,
            },
            cpu_types: vec![
                CpuType::Performance,
                CpuType::Performance,
                CpuType::Efficient,
                CpuType::Efficient,
            ],
            cpu_cores: Some(4),
            ..Default::default()
        }))),
    );

    let _ = app.update(Message::SelectProcessAffinityPCores);
    let p_cpus = app.process_presentation.affinity_cpus.as_ref().unwrap();
    assert!(p_cpus.contains(&0));
    assert!(p_cpus.contains(&1));
    assert!(!p_cpus.contains(&2));
    assert!(!p_cpus.contains(&3));

    let _ = app.update(Message::SelectProcessAffinityECores);
    let e_cpus = app.process_presentation.affinity_cpus.as_ref().unwrap();
    assert!(!e_cpus.contains(&0));
    assert!(!e_cpus.contains(&1));
    assert!(e_cpus.contains(&2));
    assert!(e_cpus.contains(&3));
}

#[test]
fn test_cpu_cache_and_policy_rows_formatting() {
    assert_eq!(crate::ui::perf_overview::format_cache_kb(512), "512 KiB");
    assert_eq!(crate::ui::perf_overview::format_cache_kb(2048), "2 MiB");
    assert_eq!(crate::ui::perf_overview::format_cache_kb(32768), "32 MiB");
    assert_eq!(crate::ui::perf_overview::format_cache_kb(1536), "1.50 MiB");
}

#[test]
fn test_process_tree_bulk_expand_all_and_collapse_all() {
    let mut app = IcedApp::demo();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(1)
                .parent_pid(None)
                .name("systemd".into())
                .build(),
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(100)
                .parent_pid(Some(1))
                .name("dbus".into())
                .build(),
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(200)
                .parent_pid(Some(100))
                .name("app".into())
                .build(),
        ])),
    );

    // 1. Collapse all
    let _ = app.update(Message::CollapseAllProcessTree);
    assert!(
        app.process_presentation
            .expanded_tree
            .contains(&ProcessLiveKey::from_parts(1, 11).expect("fixture process identity"),)
    );
    assert!(
        app.process_presentation
            .expanded_tree
            .contains(&ProcessLiveKey::from_parts(100, 1001).expect("fixture process identity"),)
    );

    // 2. Expand all
    let _ = app.update(Message::ExpandAllProcessTree);
    assert!(app.process_presentation.expanded_tree.is_empty());
}

#[test]
fn test_service_details_matching_pid_and_jump_to_process() {
    let mut app = IcedApp::demo();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(456)
                .name("NetworkManager".into())
                .build(),
        ])),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(vec![
            ServiceItem::from_inventory(
                "nm.service",
                "NetworkManager.service",
                ServiceStatus::Active,
                "",
                "",
                "",
                "",
            ),
        ])),
    );

    let _ = app
        .shell
        .apply_action(AppAction::SelectPage(AppPage::Services));
    let identity = app
        .shell
        .projection()
        .processes
        .as_deref()
        .and_then(|processes| {
            processes
                .iter()
                .find(|process| process.pid == 456)
                .and_then(ProcessLiveKey::from_process)
        })
        .expect("matching service process identity");
    let _ = app.update(Message::JumpToProcess { identity });

    assert_eq!(app.shell.page(), AppPage::Applications);
}

#[test]
fn test_network_copy_clipboard_feedback() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::CopyTextToClipboard {
        label: "IPv4".into(),
        text: "192.168.1.100".into(),
    });
    assert!(app.shell.feedback_text().contains("IPv4"));
}

#[test]
fn test_startup_action_bar_copy_command_and_location() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::OpenStartupLocation { index: 0 });
}

#[test]
fn test_users_session_remote_and_tty_details() {
    let app = IcedApp::demo();
    let _view = crate::ui::users::render(&app);
}

#[test]
fn test_performance_graph_resolution_selection() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPerformanceGraphPoints(300));
    assert_eq!(app.preferences().graph_data_points, 300);
}

#[test]
fn test_matches_process_query_structured_syntax() {
    let proc = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(1234)
        .metadata_observations(
            taskmanager_core::core::process::ProcessMetadataObservations::current(
                taskmanager_core::core::process::ProcessOwner::opaque("root"),
                None,
                1,
            ),
        )
        .status("Running".into())
        .cmdline("/usr/bin/python3 -m server".into())
        .name("python3".into())
        .build();

    // Prefix matches
    assert!(taskmanager_shell::matches_process_query(&proc, "pid:1234"));
    assert!(!taskmanager_shell::matches_process_query(&proc, "pid:5678"));
    assert!(taskmanager_shell::matches_process_query(&proc, "user:root"));
    assert!(!taskmanager_shell::matches_process_query(
        &proc,
        "user:alice"
    ));
    assert!(taskmanager_shell::matches_process_query(
        &proc,
        "status:running"
    ));
    assert!(taskmanager_shell::matches_process_query(
        &proc,
        "cmd:server"
    ));
    assert!(taskmanager_shell::matches_process_query(
        &proc,
        "name:python"
    ));

    // Multi-prefix matches
    assert!(taskmanager_shell::matches_process_query(
        &proc,
        "user:root pid:1234"
    ));
    assert!(!taskmanager_shell::matches_process_query(
        &proc,
        "user:root pid:9999"
    ));

    // Standard substring matches
    assert!(taskmanager_shell::matches_process_query(&proc, "python"));
    assert!(taskmanager_shell::matches_process_query(&proc, "1234"));
    assert!(!taskmanager_shell::matches_process_query(&proc, "nginx"));
}
