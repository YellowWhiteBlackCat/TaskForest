//! Frozen process properties follow canonical identity across filtering and PID reuse.

use super::{ProcessPropertiesSection, view_model};
use taskmanager_application::ProcessInsightFacet;
use taskmanager_application::{AppAction, AppPage, i18n::t};
use taskmanager_core::core::ScalarObservation;
use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskmanager_core::core::process_telemetry::ConnectionTransport;
use taskmanager_core::core::process_telemetry::ProcessEnvironmentEntry;
use taskmanager_shell::{ShellApp, fixture};

fn process(name: &str, token: u64) -> ProcessItem {
    let mut process = ProcessItem::new(42, name);
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(token, 1),
        ..Default::default()
    });
    process
}

#[test]
fn properties_never_display_a_reused_pid_and_do_not_depend_on_the_table_filter() {
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |rows| {
        *rows = Some(vec![process("original", 100)])
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(shell.select_row(0));
    let _ = shell.apply_action(AppAction::OpenProperties);
    let target = shell
        .process_properties_target()
        .expect("frozen target")
        .clone();
    shell.query = "hidden by filter".to_owned();
    let current = view_model(
        &shell,
        ProcessPropertiesSection::Overview,
        ProcessInsightFacet::Network,
    )
    .expect("properties remain open");
    assert_eq!(current.target, target);
    assert!(current.rows.iter().any(|(_, value)| value == "original"));
    assert!(
        !current
            .rows
            .iter()
            .any(|(_, value)| value == t("feedback.process_gone"))
    );
    fixture::edit_processes(&mut shell, |rows| {
        *rows = Some(vec![process("replacement", 200)])
    });
    shell.query.clear();
    let gone = view_model(
        &shell,
        ProcessPropertiesSection::Overview,
        ProcessInsightFacet::Network,
    )
    .expect("old frozen review remains open");
    assert_eq!(gone.target, target);
    assert!(
        gone.rows
            .iter()
            .any(|(_, value)| value == t("feedback.process_gone"))
    );
    assert!(!gone.rows.iter().any(|(_, value)| value == "replacement"));
}

#[test]
fn properties_refresh_uses_the_frozen_target_after_selection_changes() {
    use taskmanager_application::PlatformEffect;
    let mut shell = ShellApp::new();
    let original = process("original", 100);
    let mut other = process("other", 200);
    other.pid = 43;
    fixture::edit_processes(&mut shell, |rows| *rows = Some(vec![original, other]));
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    shell.query = "original".to_owned();
    assert!(shell.select_row(0));
    let _ = shell.apply_action(AppAction::OpenProperties);
    let frozen = shell.process_properties_target().expect("frozen").clone();
    shell.query = "other".to_owned();
    assert!(shell.select_row(0));
    let effect = shell
        .request_properties_process_insights()
        .expect("frozen target still lives");
    let PlatformEffect::ProcessInsights(target) = effect else {
        panic!("process insights effect")
    };
    assert_eq!(target, frozen);
    fixture::edit_processes(&mut shell, |rows| {
        *rows = Some(vec![process("replacement", 300)])
    });
    assert!(shell.request_properties_process_insights().is_none());
}

#[test]
fn performance_and_memory_views_keep_real_zeros_gaps_peaks_and_unavailable_values() {
    use taskmanager_application::process_details_vm::ProcessDetailsField;
    let mut p = process("measured", 100);
    p.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(100, 1),
        cpu_percentage: ScalarObservation::available(24.0, 1),
        memory_bytes: ScalarObservation::available(1_048_576, 1),
        memory_pss_bytes: ScalarObservation::available(524_288, 1),
        swap_bytes: ScalarObservation::available(0, 1),
        disk_read_bytes_per_sec: ScalarObservation::available(0, 1),
        ..Default::default()
    });
    p.cpu_history = vec![99.0, f32::NAN, 24.0];
    p.mem_history = vec![0.0, 1_048_576.0];
    p.disk_read_history = vec![0.0, 0.0];
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |rows| *rows = Some(vec![p]));
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(shell.select_row(0));
    let _ = shell.apply_action(AppAction::OpenProperties);
    let overview = view_model(
        &shell,
        ProcessPropertiesSection::Overview,
        ProcessInsightFacet::Network,
    )
    .expect("overview");
    assert!(
        overview
            .rows
            .iter()
            .any(|(label, value)| label == t("proc.pss") && value == "512.0 KiB")
    );
    assert!(
        overview
            .rows
            .iter()
            .any(|(label, value)| label == t("proc.uss") && value == "—")
    );
    assert!(
        overview
            .rows
            .iter()
            .any(|(label, value)| label == t("proc.swap") && value == "0 B")
    );
    let performance = view_model(
        &shell,
        ProcessPropertiesSection::Performance,
        ProcessInsightFacet::Network,
    )
    .expect("performance");
    assert_eq!(performance.curves.len(), 4);
    let cpu = performance
        .curves
        .iter()
        .find(|curve| curve.metric == ProcessDetailsField::Cpu)
        .expect("cpu");
    assert_eq!(cpu.current, "24.0%");
    assert_eq!(cpu.peak, "99.0%");
    assert!(cpu.samples[1].is_nan());
    let missing = performance
        .curves
        .iter()
        .find(|curve| curve.metric == ProcessDetailsField::DiskWriteRate)
        .expect("disk write");
    assert_eq!(missing.current, "—");
    assert_eq!(missing.peak, "—");
    assert!(
        performance.same_rendered(&performance.clone()),
        "missing samples cannot force an idle redraw"
    );
}

fn opened_shell() -> ShellApp {
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |rows| {
        *rows = Some(vec![process("original", 100)])
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(shell.select_row(0));
    let _ = shell.apply_action(AppAction::OpenProperties);
    shell
}

#[test]
fn complete_inspection_preserves_tail_entries_and_separates_resource_and_security_facts() {
    use super::insights::insight_rows;
    use taskmanager_application::ProcessInsightFacetState;
    let shell = opened_shell();
    let target = shell.process_properties_target().expect("target").clone();
    let mut projection =
        fixture::process_insights::process_insights_projection(target.clone()).expect("projection");
    assert_eq!(
        projection.raw_identity().expect("raw identity").start_token,
        target.authoritative_start_token().expect("token")
    );
    let ProcessInsightFacetState::Current(network) = &mut projection.network else {
        panic!("network")
    };
    for index in 3..8 {
        let mut connection = network.connections[0].clone();
        connection.transport = ConnectionTransport::Other(format!("tail-{index}"));
        network.connections.push(connection);
    }
    let ProcessInsightFacetState::Current(gpu) = &mut projection.gpu else {
        panic!("gpu")
    };
    for index in 1..5 {
        let mut device = gpu.devices[0].clone();
        device.device_id = format!("tail-device-{index}");
        gpu.devices.push(device);
        let mut engine = gpu.engines.engines[0].clone();
        engine.name = format!("tail-engine-{index}");
        gpu.engines.engines.push(engine);
    }
    let ProcessInsightFacetState::Current(threads) = &mut projection.threads else {
        panic!("threads")
    };
    for index in 2..8 {
        let mut thread = threads.threads[0].clone();
        thread.comm = format!("tail-thread-{index}");
        threads.threads.push(thread);
    }
    let ProcessInsightFacetState::Current(environment) = &mut projection.environment else {
        panic!("environment")
    };
    environment.entries.push(ProcessEnvironmentEntry {
        key: "TAIL_ENV".into(),
        value: "last-value".into(),
    });
    environment.truncated_count = 2;
    for (facet, needles) in [
        (ProcessInsightFacet::Network, vec!["tail-7", "12.4 ms"]),
        (
            ProcessInsightFacet::Gpu,
            vec!["tail-device-4", "tail-engine-4", "0.0%"],
        ),
        (ProcessInsightFacet::Threads, vec!["tail-thread-7", "poll"]),
        (
            ProcessInsightFacet::OpenFiles,
            vec!["9 [other]", "unreadable"],
        ),
        (
            ProcessInsightFacet::Environment,
            vec!["TAIL_ENV=last-value", "/opt/app", "+2"],
        ),
    ] {
        let (rows, _) = insight_rows(Some(&projection), facet);
        let text = rows
            .iter()
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        for needle in needles {
            assert!(
                text.contains(needle),
                "{facet:?}: missing {needle} in {text}"
            );
        }
    }
    let (resources, _) = insight_rows(Some(&projection), ProcessInsightFacet::Resources);
    assert_eq!(resources.len(), 4);
    assert!(
        resources
            .iter()
            .any(|(label, value)| label == t("proc_insights.resource_group")
                && value == "/system.slice/telemetry-worker.scope")
    );
    let (isolation, _) = insight_rows(Some(&projection), ProcessInsightFacet::Isolation);
    assert!(
        isolation
            .iter()
            .any(|(label, value)| label == t("proc_insights.seccomp") && value == "2")
    );
    assert!(
        isolation
            .iter()
            .any(|(label, value)| label == t("proc_insights.namespaces")
                && value.contains("isolated"))
    );
}

#[test]
fn independent_availability_and_mismatched_targets_remain_honest() {
    use taskmanager_application::{ProcessInsightFacetState, ProcessInsightUnavailable};
    use taskmanager_core::core::FailureKind;
    let mut shell = opened_shell();
    let target = shell.process_properties_target().expect("target").clone();
    let mut projection =
        fixture::process_insights::process_insights_projection(target).expect("projection");
    projection.network = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::RequiresEscalation),
    );
    projection.gpu = ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
        FailureKind::PermissionDenied,
    ));
    projection.threads = ProcessInsightFacetState::Pending;
    fixture::seed_projection_fact(
        &mut shell,
        fixture::ProjectionSeedFact::ProcessInsights(Box::new(Some(projection.clone()))),
    );
    let view = |shell: &ShellApp, facet| {
        view_model(shell, ProcessPropertiesSection::Insights, facet).expect("view")
    };
    assert!(view(&shell, ProcessInsightFacet::Network).authorize_network);
    assert!(
        view(&shell, ProcessInsightFacet::Gpu).rows[0]
            .1
            .contains(t("proc_insights.permission_denied"))
    );
    assert_eq!(
        view(&shell, ProcessInsightFacet::Threads).rows[0].1,
        t("proc_insights.collecting")
    );
    assert!(
        view(&shell, ProcessInsightFacet::Resources)
            .rows
            .iter()
            .any(|(_, value)| value.contains("384.0 MiB")),
        "another denied domain cannot erase accepted resources"
    );
    projection.target = FrozenProcessIdentity::from_authoritative_parts(42, "replacement", 2, 200)
        .expect("replacement");
    fixture::seed_projection_fact(
        &mut shell,
        fixture::ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
    );
    let mismatch = view(&shell, ProcessInsightFacet::Network);
    assert!(!mismatch.authorize_network);
    assert_eq!(mismatch.rows[0].1, t("proc_insights.collecting"));
}

#[path = "process_properties/ports.rs"]
mod ports;
