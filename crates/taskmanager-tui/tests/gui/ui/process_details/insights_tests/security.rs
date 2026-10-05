//! Identity-bound affinity and security facts.
use super::*;

#[test]
fn insights_lines_renders_cpu_affinity_when_observed() {
    use taskmanager_application::ProcessAffinityReady;

    let _guard = en();
    let target = FrozenProcessIdentity::from_authoritative_parts(300, "aff-proc", 1000, 1000)
        .expect("valid target");
    let revision = ProcessInsightsRevision::new(1);
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target.clone(), revision);
    let projection = tracker.snapshot().expect("snapshot exists");

    let mut app = crate::demo_app();
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
    );

    // Seed affinity ready state
    let ready = ProcessAffinityReady {
        target,
        cpus: vec![0, 1, 2, 3],
        request_id: RequestId::new(1).expect("valid request id"),
    };
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessAffinity(Some(ready)),
    );

    let text = render_text(insights_lines(
        &app,
        TuiTheme::default(),
        fixture_identity(&app, 300),
    ));
    assert!(
        text.contains("Affinity"),
        "must render Affinity label: {text}"
    );
    assert!(
        text.contains("CPUs 0, 1, 2, 3") || text.contains("4 /") || text.contains("All"),
        "must render CPUs list: {text}"
    );
}

#[test]
fn insights_lines_renders_sandbox_detection() {
    use taskmanager_core::core::process_telemetry::{IsolationKind, ProcessIsolation};

    let _guard = en();
    let target = FrozenProcessIdentity::from_authoritative_parts(301, "box-proc", 1000, 1000)
        .expect("valid target");
    let revision = ProcessInsightsRevision::new(1);
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target, revision);
    let mut projection = tracker.snapshot().expect("snapshot exists");

    projection.isolation = ProcessInsightFacetState::Current(ProcessIsolation {
        state: DeviceState::healthy(1),
        kind: Some(IsolationKind::Docker),
        container_id: Some("c-docker999".into()),
        sandboxed: Some(true),
        ..ProcessIsolation::default()
    });

    let mut app = crate::demo_app();
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
    );

    let text = render_text(insights_lines(
        &app,
        TuiTheme::default(),
        fixture_identity(&app, 301),
    ));
    assert!(
        text.contains("Docker"),
        "must render Docker isolation: {text}"
    );
    assert!(
        text.contains("c-docker999"),
        "must render container id: {text}"
    );
    assert!(
        text.contains("Sandboxed"),
        "must render sandboxed label: {text}"
    );
}

#[test]
fn insights_lines_renders_namespace_audit() {
    use taskmanager_core::core::process_telemetry::{
        LinuxNamespaceAudit, LinuxNamespaceKind, NamespaceAuditEntry, NamespaceAuditStatus,
        ProcessIsolation,
    };

    let _guard = en();
    let target = FrozenProcessIdentity::from_authoritative_parts(302, "ns-proc", 1000, 1000)
        .expect("valid target");
    let revision = ProcessInsightsRevision::new(1);
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target, revision);
    let mut projection = tracker.snapshot().expect("snapshot exists");

    let audit = LinuxNamespaceAudit::from_entries(
        DeviceState::healthy(1),
        vec![NamespaceAuditEntry {
            kind: LinuxNamespaceKind::Pid,
            status: NamespaceAuditStatus::Isolated {
                inode: 4026533000,
                host_inode: 4026531836,
            },
        }],
    );
    projection.isolation = ProcessInsightFacetState::Current(ProcessIsolation {
        state: DeviceState::healthy(1),
        namespaces: Some(audit),
        ..ProcessIsolation::default()
    });

    let mut app = crate::demo_app();
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
    );

    let text = render_text(insights_lines(
        &app,
        TuiTheme::default(),
        fixture_identity(&app, 302),
    ));
    assert!(
        text.contains("Namespaces"),
        "must render namespaces label: {text}"
    );
}

#[test]
fn insights_lines_renders_seccomp_filter() {
    use taskmanager_core::core::process_telemetry::ProcessIsolation;

    let _guard = en();
    let target = FrozenProcessIdentity::from_authoritative_parts(303, "sec-proc", 1000, 1000)
        .expect("valid target");
    let revision = ProcessInsightsRevision::new(1);
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target, revision);
    let mut projection = tracker.snapshot().expect("snapshot exists");

    projection.isolation = ProcessInsightFacetState::Current(ProcessIsolation {
        state: DeviceState::healthy(1),
        seccomp_mode: Some(2),
        ..ProcessIsolation::default()
    });

    let mut app = crate::demo_app();
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
    );

    let text = render_text(insights_lines(
        &app,
        TuiTheme::default(),
        fixture_identity(&app, 303),
    ));
    assert!(
        text.contains("Seccomp"),
        "must render Seccomp label: {text}"
    );
}
