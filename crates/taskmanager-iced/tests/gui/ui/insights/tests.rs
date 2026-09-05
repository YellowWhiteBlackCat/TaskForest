//! The honesty unit tests for the insights facet helpers (thread CPU gaps,
//! unreadable open-file targets, cold-start engine rates, typed unavailable
//! reasons), extracted from [`super`] so the insights module stays under the
//! source-size budget. Moved verbatim; the assertions are unchanged.

use super::*;

/// Honesty: a thread whose `stat` lacked parseable CPU counters must render
/// the explicit dash, never a fabricated "0.0s"/"0.0%". These are the same
/// accessors the thread-row widget draws from.
#[test]
fn thread_cpu_helpers_keep_a_missing_value_honest() {
    let gap = ProcessThreadInfo {
        tid: 4243,
        comm: "reaper".into(),
        state: taskmanager_core::core::process_telemetry::ThreadState::Running,
        cpu_time_secs: None,
        cpu_percent: None,
    };
    let warm = ProcessThreadInfo {
        tid: 4242,
        comm: "telemetry-main".into(),
        state: taskmanager_core::core::process_telemetry::ThreadState::Sleep,
        cpu_time_secs: Some(12.5),
        cpu_percent: Some(18.5),
    };
    assert_eq!(cpu_time_text(gap.cpu_time_secs), "—");
    assert_eq!(cpu_percent_text(gap.cpu_percent), "—");
    assert_eq!(warm.state.as_short_label(), "S");
    assert_eq!(cpu_time_text(warm.cpu_time_secs), "12.5s");
    assert_eq!(cpu_percent_text(warm.cpu_percent), "18.5%");
}

/// Honesty: an unreadable descriptor (None target) must surface the typed
/// "unreadable" marker, never a blank target or a fabricated path.
#[test]
fn open_file_row_marks_an_unreadable_target_not_blank() {
    let readable = OpenFileEntry {
        fd: 0,
        kind: taskmanager_core::core::process_telemetry::OpenFileKind::File,
        target: Some("/dev/null".into()),
    };
    let unreadable = OpenFileEntry {
        fd: 9,
        kind: taskmanager_core::core::process_telemetry::OpenFileKind::Other,
        target: None,
    };
    assert!(format_open_file_row(&readable).contains("/dev/null"));
    let denied_line = format_open_file_row(&unreadable);
    assert!(
        denied_line.contains(t("proc_insights.unreadable")),
        "an unreadable fd must surface the typed marker, got: {denied_line}"
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
    // back to the cycle counter (xe), never showing both.
    let busy = ScalarObservation::available(3_600_000_000_000_u64, 1);
    let cycles = ScalarObservation::available(123_456_u64, 1);
    let i915 = format_engine_usage("render", &warm, &busy, &no_time);
    assert!(
        i915.contains("01h 00m"),
        "busy ns renders as duration: {i915}"
    );
    let xe = format_engine_usage("render", &warm, &no_time, &cycles);
    assert!(xe.contains("cycles"), "xe cycle counter is shown: {xe}");
    assert!(!i915.contains("cycles"));
    assert!(!xe.contains("1h") && !xe.contains("h "));
}

/// The typed unavailable message maps reason variants the way the overlay
/// promises: permission-denied (and escalation-requiring) → "permission
/// denied", unsupported → "unsupported", anything else → "unavailable".
#[test]
fn facet_unavailable_text_maps_typed_reasons() {
    assert_eq!(
        facet_unavailable_text(&ProcessInsightUnavailable::Provider(
            FailureKind::PermissionDenied
        )),
        "permission denied"
    );
    assert_eq!(
        facet_unavailable_text(&ProcessInsightUnavailable::Provider(
            FailureKind::RequiresEscalation
        )),
        "permission denied"
    );
    assert_eq!(
        facet_unavailable_text(&ProcessInsightUnavailable::Provider(
            FailureKind::Unsupported
        )),
        "unsupported"
    );
    assert_eq!(
        facet_unavailable_text(&ProcessInsightUnavailable::Submission(
            SubmissionErrorKind::UnsupportedCapability
        )),
        "unsupported"
    );
    assert_eq!(
        facet_unavailable_text(&ProcessInsightUnavailable::Provider(
            FailureKind::TemporarilyUnavailable
        )),
        "unavailable"
    );
}

#[cfg(test)]
mod connection_tests {
    use super::*;
    use taskmanager_core::core::process_telemetry::{
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
            state: taskmanager_core::core::process_telemetry::ConnectionState::Established,
            provider_key: None,
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
        assert_eq!(format_connection(&v4), "TCP  127.0.0.1:80 → 10.0.0.2:443");
        let v6 = connection(
            ConnectionTransport::Udp,
            ConnectionAddressFamily::Ipv6,
            ConnectionEndpoint::Ip("[::1]:53".parse().unwrap()),
            ConnectionEndpoint::Ip("[fe80::1]:53".parse().unwrap()),
        );
        assert_eq!(format_connection(&v6), "UDP6  [::1]:53 → [fe80::1]:53");
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
fn insights_sections_render_all_states_without_panic() {
    use taskmanager_application::{ProcessInsightsProjection, ProcessInsightsRevision};
    use taskmanager_core::core::process::FrozenProcessIdentity;

    let theme = taskmanager_theme::Theme::default();

    // None (initial)
    let _ = environment_section(&theme, None);
    let _ = resources_section(&theme, None);
    let _ = isolation_section(&theme, None);
    let _ = gpu_devices_section(&theme, None);

    // Pending via tracker
    let target =
        FrozenProcessIdentity::from_authoritative_parts(1, String::from("init"), 10, 100).unwrap();
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target.clone(), ProcessInsightsRevision::new(1));
    let pending = tracker.snapshot().unwrap();

    let _ = environment_section(&theme, Some(&pending));
    let _ = resources_section(&theme, Some(&pending));
    let _ = isolation_section(&theme, Some(&pending));
    let _ = gpu_devices_section(&theme, Some(&pending));

    // Unavailable
    let mut unavailable = pending.clone();
    unavailable.threads = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
    );
    unavailable.open_files = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
    );
    unavailable.network = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::RequiresEscalation),
    );
    unavailable.gpu = ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
        FailureKind::Unsupported,
    ));
    unavailable.resources = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Submission(SubmissionErrorKind::UnsupportedCapability),
    );
    unavailable.isolation = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::TemporarilyUnavailable),
    );
    unavailable.environment = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
    );

    let _ = environment_section(&theme, Some(&unavailable));
    let _ = resources_section(&theme, Some(&unavailable));
    let _ = isolation_section(&theme, Some(&unavailable));
    let _ = gpu_devices_section(&theme, Some(&unavailable));
}
