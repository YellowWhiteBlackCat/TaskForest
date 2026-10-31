//! Honesty and rendered-text tests for the Insights facet helpers (thread CPU
//! gaps, unreadable open-file targets, cold-start engine rates, typed
//! unavailable reasons, and mixed Current/Pending/Unavailable facets),
//! extracted from [`super`] so the insights module stays under the source-size
//! budget.

const PREVIEW_CAP: usize = 8;

use super::*;
use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::{Id, Operation, Tree};
use iced::{Element, Rectangle, Size};
use taskforest_contract::SubmissionErrorKind;
use taskforest_core::core::failure::FailureKind;
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process::FrozenProcessIdentity;
use taskforest_core::core::process_telemetry::ConnectionState;
use taskforest_core::core::process_telemetry::OpenFileKind;
use taskforest_core::core::process_telemetry::ThreadState;
use taskmanager_application::i18n::{Language, with_language};
use taskmanager_application::{
    ProcessInsightFacet, ProcessInsightFacetState, ProcessInsightUnavailable,
};
use taskmanager_assets::embedded_fonts;
use taskmanager_shell::demo_app;
use taskmanager_shell::fixture::process_insights::process_insights_projection;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
use taskmanager_test_support::pin_english;
use taskmanager_theme::Theme;

#[derive(Default)]
struct RenderedTextFragments(Vec<(String, Rectangle)>);

impl Operation for RenderedTextFragments {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }

    fn text(&mut self, _id: Option<&Id>, bounds: Rectangle, fragment: &str) {
        self.0.push((fragment.to_owned(), bounds));
    }
}

fn software_renderer() -> iced::Renderer {
    {
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .expect("font system");
        for font in embedded_fonts() {
            fonts.load_font(font);
        }
    }
    iced::futures::executor::block_on(iced::Renderer::new(
        crate::theme_binding::BUNDLED_UI_FONT,
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("software renderer")
}

fn rendered_text_fragments(
    element: &mut Element<'_, Message, iced::Theme, iced::Renderer>,
    renderer: &iced::Renderer,
) -> Vec<(String, Rectangle)> {
    let mut tree = Tree::new(&*element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        renderer,
        &Limits::new(Size::ZERO, Size::new(720.0, 480.0)),
    );
    let mut fragments = RenderedTextFragments::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), renderer, &mut fragments);
    fragments.0
}

fn has_visible_text(fragments: &[(String, Rectangle)], expected: &str) -> bool {
    fragments
        .iter()
        .any(|(fragment, bounds)| fragment == expected && bounds.width > 0.0 && bounds.height > 0.0)
}

/// Honesty: a thread whose `stat` lacked parseable CPU counters must render
/// the explicit dash, never a fabricated "0.0s"/"0.0%". These are the same
/// accessors the thread-row widget draws from.
#[test]
fn thread_cpu_helpers_keep_a_missing_value_honest() {
    let gap = ProcessThreadInfo {
        tid: 4243,
        comm: "reaper".into(),
        state: ThreadState::Running,
        cpu_time_secs: None,
        cpu_percent: None,
        wchan: None,
        run_queue_wait_ns: None,
        wait_kind: None,
    };
    let warm = ProcessThreadInfo {
        tid: 4242,
        comm: "telemetry-main".into(),
        state: ThreadState::Sleep,
        cpu_time_secs: Some(12.5),
        cpu_percent: Some(18.5),
        wchan: None,
        run_queue_wait_ns: None,
        wait_kind: None,
    };
    assert_eq!(cpu_time_text(gap.cpu_time_secs), "—");
    assert_eq!(cpu_percent_text(gap.cpu_percent), "—");
    assert_eq!(warm.state.as_short_label(), "S");
    assert_eq!(cpu_time_text(warm.cpu_time_secs), "12.5s");
    assert_eq!(cpu_percent_text(warm.cpu_percent), "18.5%");

    // The facet's row fold enumerates one row per projected thread (same
    // order), and the gap thread's own row keeps both dashes. The cap bounds
    // the materialized rows instead of dropping the trailing ones silently.
    let rows = thread_rows_vm(&[warm.clone(), gap.clone()], PREVIEW_CAP);
    assert_eq!(rows.len(), 2, "one row per projected thread");
    assert_eq!(
        (
            rows[0].tid.as_str(),
            rows[0].comm.as_str(),
            rows[0].state.as_str(),
            rows[0].cpu_time.as_str(),
            rows[0].cpu_percent.as_str(),
        ),
        ("4242", "telemetry-main", "S", "12.5s", "18.5%"),
        "the warm thread row must carry its own per-thread statistics"
    );
    assert_eq!(
        (
            rows[1].tid.as_str(),
            rows[1].comm.as_str(),
            rows[1].state.as_str(),
            rows[1].cpu_time.as_str(),
            rows[1].cpu_percent.as_str(),
        ),
        ("4243", "reaper", "R", "—", "—"),
        "the gap thread row must keep the typed dashes, never 0.0s/0.0%"
    );
    let many: Vec<ProcessThreadInfo> = (0..PREVIEW_CAP + 3)
        .map(|index| ProcessThreadInfo {
            tid: 5_000 + index as u32,
            comm: format!("worker-{index}"),
            ..warm.clone()
        })
        .collect();
    assert_eq!(
        thread_rows_vm(&many, PREVIEW_CAP).len(),
        PREVIEW_CAP,
        "the facet cap must bound the materialized thread rows"
    );
}

#[test]
fn thread_rows_vm_renders_runqueue_and_wait_diagnostics() {
    use taskforest_core::core::process_telemetry;

    let thread = ProcessThreadInfo {
        tid: 4244,
        comm: "io-worker".into(),
        state: ThreadState::UninterruptibleSleep,
        cpu_time_secs: Some(1.0),
        cpu_percent: Some(2.0),
        wchan: Some("futex_wait_queue_me".into()),
        run_queue_wait_ns: Some(2_500_000),
        wait_kind: Some(process_telemetry::ThreadWaitKind::KernelLock),
    };
    let rows = thread_rows_vm(&[thread], PREVIEW_CAP);
    assert_eq!(rows[0].wait, "lock 2.5ms");
    assert_eq!(rows[0].state, "D");
}

/// Honesty: an unreadable descriptor (None target) must surface the typed
/// "unreadable" marker, never a blank target or a fabricated path.
#[test]
fn open_file_row_marks_an_unreadable_target_not_blank() {
    let readable = OpenFileEntry {
        fd: 0,
        kind: OpenFileKind::File,
        target: Some("/dev/null".into()),
        deleted: false,
    };
    let unreadable = OpenFileEntry {
        fd: 9,
        kind: OpenFileKind::Other,
        target: None,
        deleted: false,
    };
    assert!(format_open_file_row(&readable).contains("/dev/null"));
    let denied_line = format_open_file_row(&unreadable);
    assert!(
        denied_line.contains(t("proc_insights.unreadable")),
        "an unreadable fd must surface the typed marker, got: {denied_line}"
    );

    // The facet's row fold enumerates one row per projected descriptor (same
    // order): the readable descriptor keeps its target and the unresolved
    // readlink keeps its typed marker. The cap bounds the materialized rows.
    let rows = open_file_rows(&[readable.clone(), unreadable.clone()], PREVIEW_CAP);
    assert_eq!(rows.len(), 2, "one row per projected descriptor");
    assert!(rows[0].contains("/dev/null") && rows[0].starts_with("fd 0"));
    assert!(
        rows[0].contains("[file]"),
        "descriptor classifies handle type"
    );
    assert!(
        rows[1].contains(t("proc_insights.unreadable")) && rows[1].starts_with("fd 9"),
        "the unreadable descriptor's own row carries its marker: {rows:?}"
    );
    let deleted_entry = OpenFileEntry {
        fd: 3,
        kind: OpenFileKind::File,
        target: Some("/tmp/deleted.log".to_string()),
        deleted: true,
    };
    let deleted_rows = open_file_rows(&[deleted_entry], PREVIEW_CAP);
    assert!(
        deleted_rows[0].contains("[deleted]"),
        "deleted descriptor is marked"
    );
    assert!(
        deleted_rows[0].contains("[file]"),
        "deleted descriptor classifies handle type"
    );
    let many: Vec<OpenFileEntry> = (0..PREVIEW_CAP + 3)
        .map(|index| OpenFileEntry {
            fd: index as u32,
            kind: OpenFileKind::File,
            target: Some(format!("/tmp/session-{index}.lock")),
            deleted: false,
        })
        .collect();
    assert_eq!(
        open_file_rows(&many, PREVIEW_CAP).len(),
        PREVIEW_CAP,
        "the facet cap must bound the materialized descriptor rows"
    );
}

/// Honesty: a cold-start rate gap must never fabricate `0.0%`, while a
/// warmed engine reports its percentage and (when the driver reports it)
/// its cumulative busy time or cycle count.
#[test]
fn engine_usage_keeps_a_cold_start_gap_honest() {
    let gap = ScalarObservation::<f32>::unavailable(FailureKind::TemporarilyUnavailable);
    let warm = ScalarObservation::available(12.5_f32, 1);
    let no_time = ScalarObservation::<u64>::unavailable(FailureKind::TemporarilyUnavailable);
    let gap_line = format_engine_usage("render", &gap, &no_time, &no_time);
    let warm_line = format_engine_usage("video", &warm, &no_time, &no_time);
    assert!(gap_line.contains("—"));
    assert!(
        !gap_line.contains("0.0%"),
        "cold-start gap must not fabricate 0%: {gap_line}"
    );
    assert!(warm_line.contains("12.5%"));
    // The cumulative readout prefers busy-nanoseconds (i915) and falls
    // back to the cycle counter (xe), never showing both. The duration
    // template and the cycle wording are locale-owned, so the test pins the
    // language it renders and asserts against instead of inheriting the
    // host's locale.
    let busy = ScalarObservation::available(3_600_000_000_000_u64, 1);
    let cycles = ScalarObservation::available(123_456_u64, 1);
    with_language(Language::En, || {
        let i915 = format_engine_usage("render", &warm, &busy, &no_time);
        assert!(
            i915.contains("01h 00m"),
            "busy ns renders as duration: {i915}"
        );
        let xe = format_engine_usage("render", &warm, &no_time, &cycles);
        assert!(xe.contains("cycles"), "xe cycle counter is shown: {xe}");
        assert!(!i915.contains("cycles"));
        assert!(!xe.contains("1h") && !xe.contains("h "));
    });
}

/// The typed unavailable message maps reasons to the shared localized copy:
/// permission-denied/escalation, unsupported-provider, and general unavailable.
#[test]
fn facet_unavailable_text_maps_typed_reasons() {
    pin_english();
    assert_eq!(
        facet_unavailable_text(&ProcessInsightUnavailable::Provider(
            FailureKind::PermissionDenied
        )),
        "Permission denied"
    );
    assert_eq!(
        facet_unavailable_text(&ProcessInsightUnavailable::Provider(
            FailureKind::RequiresEscalation
        )),
        "Permission denied"
    );
    assert_eq!(
        facet_unavailable_text(&ProcessInsightUnavailable::Provider(
            FailureKind::Unsupported
        )),
        "Unsupported by this provider"
    );
    assert_eq!(
        facet_unavailable_text(&ProcessInsightUnavailable::Submission(
            SubmissionErrorKind::UnsupportedCapability
        )),
        "Unsupported by this provider"
    );
    assert_eq!(
        facet_unavailable_text(&ProcessInsightUnavailable::Provider(
            FailureKind::TemporarilyUnavailable
        )),
        "Unavailable"
    );

    with_language(Language::Zh, || {
        assert_eq!(
            facet_unavailable_text(&ProcessInsightUnavailable::Provider(
                FailureKind::PermissionDenied
            )),
            "权限不足"
        );
        assert_eq!(
            facet_unavailable_text(&ProcessInsightUnavailable::Provider(
                FailureKind::Unsupported
            )),
            "此提供器不支持该指标"
        );
        assert_eq!(
            facet_unavailable_text(&ProcessInsightUnavailable::Provider(
                FailureKind::TemporarilyUnavailable
            )),
            "不可用"
        );
    });
    pin_english();
}

#[cfg(test)]
mod connection_tests {
    use super::*;
    use taskforest_core::core::process_telemetry::{
        ConnectionAddressFamily, ConnectionEndpoint, ConnectionTransport, ProcessConnection,
    };

    fn connection(
        transport: ConnectionTransport,
        family: ConnectionAddressFamily,
        local: ConnectionEndpoint,
        remote: ConnectionEndpoint,
    ) -> ProcessConnection {
        ProcessConnection {
            transport,
            family,
            local,
            remote,
            state: ConnectionState::Established,
            provider_key: None,
            rtt_ms: None,
        }
    }

    #[test]
    fn connection_readout_keeps_endpoints_and_family_aware_names() {
        let v4 = connection(
            ConnectionTransport::Tcp,
            ConnectionAddressFamily::Ipv4,
            ConnectionEndpoint::Ip("127.0.0.1:80".parse().unwrap()),
            ConnectionEndpoint::Ip("10.0.0.2:443".parse().unwrap()),
        );
        assert_eq!(
            format_connection(&v4),
            "TCP [ESTABLISHED · EXTERNAL]  127.0.0.1:80 → 10.0.0.2:443"
        );
        let v6 = connection(
            ConnectionTransport::Udp,
            ConnectionAddressFamily::Ipv6,
            ConnectionEndpoint::Ip("[::1]:53".parse().unwrap()),
            ConnectionEndpoint::Ip("[fe80::1]:53".parse().unwrap()),
        );
        assert_eq!(
            format_connection(&v6),
            "UDP6 [ESTABLISHED · EXTERNAL]  [::1]:53 → [fe80::1]:53"
        );
        let local = connection(
            ConnectionTransport::Tcp,
            ConnectionAddressFamily::Local,
            ConnectionEndpoint::Local {
                path: "/run/user/1000/x".into(),
            },
            ConnectionEndpoint::Unspecified,
        );
        assert!(
            format_connection(&local).contains("/run/user/1000/x"),
            "unix-socket path must not be dropped"
        );
    }

    #[test]
    fn connection_readout_surfaces_rtt_metrics() {
        let mut conn = connection(
            ConnectionTransport::Tcp,
            ConnectionAddressFamily::Ipv4,
            ConnectionEndpoint::Ip("127.0.0.1:80".parse().unwrap()),
            ConnectionEndpoint::Ip("10.0.0.2:443".parse().unwrap()),
        );
        conn.rtt_ms = Some(16.5);
        let text = format_connection(&conn);
        assert!(
            text.contains("16.5 ms"),
            "must surface RTT in connection readout: {text}"
        );
    }
}

#[test]
fn format_resource_pair_formats_honestly() {
    assert_eq!(
        format_resource_pair(
            Some("10 MiB".into()),
            Some(LimitValue::Unlimited),
            |v| format!("{v} B")
        ),
        Some("10 MiB / ∞".into())
    );
    assert_eq!(
        format_resource_pair(
            Some("10 MiB".into()),
            Some(LimitValue::Value(100)),
            |v| format!("{v} B")
        ),
        Some("10 MiB / 100 B".into())
    );
    assert_eq!(
        format_resource_pair(Some("10 MiB".into()), None, |v| format!("{v} B")),
        Some("10 MiB".into())
    );
    assert_eq!(
        format_resource_pair(None, Some(LimitValue::Unlimited), |v| format!("{v} B")),
        Some("— / ∞".into())
    );
    assert_eq!(
        format_resource_pair(None, Some(LimitValue::Value(100)), |v| format!("{v} B")),
        Some("— / 100 B".into())
    );
    assert_eq!(format_resource_pair(None, None, |v| format!("{v} B")), None);
}

#[test]
fn insights_current_resource_facet_renders_beside_pending_and_typed_unavailable_siblings() {
    pin_english();
    let theme = Theme::dark();
    let target =
        FrozenProcessIdentity::from_authoritative_parts(1, String::from("init"), 10, 100).unwrap();
    let mut mixed = process_insights_projection(target.clone()).expect("fixture insights");
    mixed.environment = ProcessInsightFacetState::Pending;
    let unavailable_reason = ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied);
    mixed.network = ProcessInsightFacetState::Unavailable(unavailable_reason);

    assert!(matches!(
        &mixed.resources,
        ProcessInsightFacetState::Current(_)
    ));
    assert!(matches!(
        &mixed.environment,
        ProcessInsightFacetState::Pending
    ));
    assert!(matches!(
        &mixed.network,
        ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
            FailureKind::PermissionDenied
        ))
    ));
    assert!(mixed.is_collecting(), "pending siblings remain pending");

    let mut shell = demo_app();
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::ProcessInsights(Box::new(Some(mixed))),
    );
    let renderer = software_renderer();

    let mut current_view = insights_block(&theme, &shell, &target, ProcessInsightFacet::Resources);
    let current_text = rendered_text_fragments(&mut current_view, &renderer);
    assert!(has_visible_text(
        &current_text,
        "/system.slice/telemetry-worker.scope"
    ));

    let mut pending_view =
        insights_block(&theme, &shell, &target, ProcessInsightFacet::Environment);
    let pending_text = rendered_text_fragments(&mut pending_view, &renderer);
    assert!(
        pending_text.iter().any(|(fragment, bounds)| {
            matches!(fragment.as_str(), "collecting…" | "采集中…")
                && bounds.width > 0.0
                && bounds.height > 0.0
        }),
        "the Pending sibling must produce visible collecting text: {pending_text:?}"
    );

    let mut unavailable_view =
        insights_block(&theme, &shell, &target, ProcessInsightFacet::Network);
    let unavailable_text = rendered_text_fragments(&mut unavailable_view, &renderer);
    assert!(
        unavailable_text.iter().any(|(fragment, bounds)| {
            fragment == "Permission denied" && bounds.width > 0.0 && bounds.height > 0.0
        }),
        "the typed PermissionDenied sibling must keep its visible reason: {unavailable_text:?}"
    );

    let chinese_unavailable_text = with_language(Language::Zh, || {
        let mut unavailable_view =
            insights_block(&theme, &shell, &target, ProcessInsightFacet::Network);
        rendered_text_fragments(&mut unavailable_view, &renderer)
    });
    assert!(
        has_visible_text(&chinese_unavailable_text, "权限不足"),
        "the typed PermissionDenied sibling must localize its visible reason: {chinese_unavailable_text:?}"
    );
}

#[test]
fn open_files_section_renders_fd_limit_saturation() {
    use taskforest_core::core::device_state::DeviceState;
    use taskforest_core::core::process::FrozenProcessIdentity;
    use taskforest_core::core::process_telemetry::{
        OpenFileEntry, OpenFileKind, ProcessOpenFiles, ProcessResourceObservations,
        ResourceObservation,
    };
    use taskforest_core::{LimitValue, ProcessResourceSnapshot, ResourceLimit, ResourceLimitKind};
    use taskmanager_application::{ProcessInsightsProjection, ProcessInsightsRevision};

    let theme = Theme::dark();
    let target =
        FrozenProcessIdentity::from_authoritative_parts(1, String::from("init"), 10, 100).unwrap();
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target, ProcessInsightsRevision::new(1));
    let mut pending = tracker.snapshot().unwrap();

    let open_files = ProcessOpenFiles {
        state: DeviceState::healthy(1000),
        entries: vec![OpenFileEntry {
            fd: 0,
            kind: OpenFileKind::File,
            target: Some("/dev/null".into()),
            deleted: false,
        }],
        unreadable_count: 0,
    };
    pending.open_files = ProcessInsightFacetState::Current(open_files);

    let obs = ProcessResourceObservations {
        limits: ResourceObservation::current(
            vec![ResourceLimit {
                kind: ResourceLimitKind::OpenFiles,
                soft: LimitValue::Value(1024),
                hard: LimitValue::Value(4096),
                unit: None,
            }],
            1000,
        ),
        ..ProcessResourceObservations::default()
    };
    let res = ProcessResourceSnapshot::from_observations(DeviceState::healthy(1000), obs, vec![]);
    pending.resources = ProcessInsightFacetState::Current(res);

    let _section = open_files_section(&theme, Some(&pending));
}

#[test]
fn isolation_section_renders_sandbox_detection() {
    use taskforest_core::core::device_state::DeviceState;
    use taskforest_core::core::process::FrozenProcessIdentity;
    use taskforest_core::core::process_telemetry::{IsolationKind, ProcessIsolation};
    use taskmanager_application::{ProcessInsightsProjection, ProcessInsightsRevision};

    let theme = Theme::default();
    let target =
        FrozenProcessIdentity::from_authoritative_parts(1, String::from("init"), 10, 100).unwrap();
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target, ProcessInsightsRevision::new(1));
    let mut projection = tracker.snapshot().unwrap();

    projection.isolation = ProcessInsightFacetState::Current(ProcessIsolation {
        state: DeviceState::healthy(1),
        kind: Some(IsolationKind::Docker),
        container_id: Some("c-docker123".into()),
        sandboxed: Some(true),
        ..ProcessIsolation::default()
    });

    let _section = isolation_section(&theme, Some(&projection));
}

#[test]
fn isolation_section_renders_posix_capabilities() {
    use taskforest_core::core::device_state::DeviceState;
    use taskforest_core::core::process::FrozenProcessIdentity;
    use taskforest_core::core::process_telemetry::{ProcessCapabilities, ProcessIsolation};
    use taskmanager_application::{ProcessInsightsProjection, ProcessInsightsRevision};

    let theme = Theme::default();
    let target =
        FrozenProcessIdentity::from_authoritative_parts(1, String::from("init"), 10, 100).unwrap();
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target, ProcessInsightsRevision::new(1));
    let mut projection = tracker.snapshot().unwrap();

    projection.isolation = ProcessInsightFacetState::Current(ProcessIsolation {
        state: DeviceState::healthy(1),
        capabilities: Some(ProcessCapabilities::from_masks(
            DeviceState::healthy(1),
            Some(0),
            Some(1 << 21),
            Some(1 << 21),
            Some(0),
            Some(0),
        )),
        ..ProcessIsolation::default()
    });

    let _section = isolation_section(&theme, Some(&projection));
}

#[test]
fn isolation_section_renders_namespace_audit() {
    use taskforest_core::core::device_state::DeviceState;
    use taskforest_core::core::process::FrozenProcessIdentity;
    use taskforest_core::core::process_telemetry::{
        LinuxNamespaceAudit, LinuxNamespaceKind, NamespaceAuditEntry, NamespaceAuditStatus,
        ProcessIsolation,
    };
    use taskmanager_application::{ProcessInsightsProjection, ProcessInsightsRevision};

    let theme = Theme::default();
    let target =
        FrozenProcessIdentity::from_authoritative_parts(1, String::from("init"), 10, 100).unwrap();
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target, ProcessInsightsRevision::new(1));
    let mut projection = tracker.snapshot().unwrap();

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

    let _section = isolation_section(&theme, Some(&projection));
}

#[test]
fn isolation_section_renders_seccomp_filter() {
    use taskforest_core::core::device_state::DeviceState;
    use taskforest_core::core::process::FrozenProcessIdentity;
    use taskforest_core::core::process_telemetry::ProcessIsolation;
    use taskmanager_application::{ProcessInsightsProjection, ProcessInsightsRevision};

    let theme = Theme::default();
    let target =
        FrozenProcessIdentity::from_authoritative_parts(1, String::from("init"), 10, 100).unwrap();
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target, ProcessInsightsRevision::new(1));
    let mut projection = tracker.snapshot().unwrap();

    projection.isolation = ProcessInsightFacetState::Current(ProcessIsolation {
        state: DeviceState::healthy(1),
        seccomp_mode: Some(2),
        ..ProcessIsolation::default()
    });

    let _section = isolation_section(&theme, Some(&projection));
}

#[test]
fn network_section_renders_observed_throughput() {
    use taskforest_core::core::device_state::DeviceState;
    use taskforest_core::core::process::FrozenProcessIdentity;
    use taskforest_core::core::process_telemetry::ProcessNetworkSnapshot;
    use taskmanager_application::{ProcessInsightsProjection, ProcessInsightsRevision};

    let theme = Theme::default();
    let target =
        FrozenProcessIdentity::from_authoritative_parts(1, String::from("init"), 10, 100).unwrap();
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target, ProcessInsightsRevision::new(1));
    let mut projection = tracker.snapshot().unwrap();
    projection.network = ProcessInsightFacetState::Current(ProcessNetworkSnapshot {
        state: DeviceState::healthy(1),
        connections: Vec::new(),
        rx_bytes_per_sec: Some(1024),
        tx_bytes_per_sec: Some(2048),
        traffic_state: DeviceState::healthy(1),
        traffic_failure: None,
        traffic_provider: None,
        connection_counters: None,
    });

    let _section = network_section(&theme, Some(&projection));
}

#[test]
fn environment_section_renders_observed_entries() {
    use taskforest_core::core::device_state::DeviceState;
    use taskforest_core::core::process::FrozenProcessIdentity;
    use taskforest_core::core::process_telemetry::{ProcessEnvironment, ProcessEnvironmentEntry};
    use taskmanager_application::{ProcessInsightsProjection, ProcessInsightsRevision};

    let theme = Theme::default();
    let target =
        FrozenProcessIdentity::from_authoritative_parts(1, String::from("init"), 10, 100).unwrap();
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target, ProcessInsightsRevision::new(1));
    let mut projection = tracker.snapshot().unwrap();
    projection.environment = ProcessInsightFacetState::Current(ProcessEnvironment {
        state: DeviceState::healthy(1),
        working_directory: Some("/opt/app".into()),
        entries: vec![ProcessEnvironmentEntry {
            key: "PATH".into(),
            value: "/usr/bin".into(),
        }],
        ..ProcessEnvironment::default()
    });

    let _section = environment_section(&theme, Some(&projection));
}
