//! Service discovery, parsing, command-boundary, and log-worker regressions.

use super::*;
use std::thread;
use std::time::Instant;

#[test]
fn openrc_status_maps_started_stopped_crashed() {
    // Canonical rc-status layout: runlevel header, then `[ started ]` rows;
    // the last row uses the newer OpenRC duration suffix to verify only the
    // first bracketed token is treated as the state.
    let out = [
        "Runlevel: default",
        " acpid                                                            [  started  ]",
        " alsa                                                             [  stopped  ]",
        " bluetooth                                                        [  crashed  ]",
        " cronie                                                           [  started 00:00:02 (0) ]",
        "Dynamic Runlevel: hotplugged",
    ]
    .join("\n");

    let svcs = parse_openrc_status(&out);
    assert_eq!(svcs.len(), 4, "headers + blank must be skipped");

    assert_eq!(svcs[0].name, "acpid");
    assert_eq!(svcs[0].status, ServiceStatus::Active);
    assert_eq!(svcs[0].active_state, "started");
    assert_eq!(svcs[0].load_state, "loaded");

    assert_eq!(svcs[1].name, "alsa");
    assert_eq!(svcs[1].status, ServiceStatus::Inactive);
    assert_eq!(svcs[1].active_state, "stopped");

    assert_eq!(svcs[2].name, "bluetooth");
    assert_eq!(svcs[2].status, ServiceStatus::Failed);
    assert_eq!(svcs[2].active_state, "crashed");

    // Duration suffix must not corrupt the parsed state.
    assert_eq!(svcs[3].name, "cronie");
    assert_eq!(svcs[3].status, ServiceStatus::Active);
    assert_eq!(svcs[3].active_state, "started");
}

#[test]
fn openrc_status_skips_headers_blanks_and_braceless_lines() {
    let out = [
        "",
        "Runlevel: default",
        "Dynamic Runlevel: hotplugged",
        "  ",
    ]
    .join("\n");
    assert!(parse_openrc_status(&out).is_empty());
    assert!(parse_openrc_status("").is_empty());
}

#[test]
fn openrc_update_dedupes_and_merges_runlevels() {
    // alsasound appears twice (boot + default) and must collapse to one row
    // with both runlevels joined in the description.
    let out = [
        "            acpid |      default",
        "         alsasound |      boot",
        "         alsasound |      default",
        "         alsasound |      boot default",
        "      bluetooth |      default",
    ]
    .join("\n");

    let svcs = parse_openrc_update(&out);
    assert_eq!(svcs.len(), 3, "duplicate service names must be merged");
    assert_eq!(svcs[0].name, "acpid");
    assert_eq!(svcs[0].status, ServiceStatus::Unknown);
    assert_eq!(svcs[0].description, "default");

    let alsa = svcs.iter().find(|s| s.name == "alsasound").unwrap();
    assert!(alsa.description.contains("boot"));
    assert!(alsa.description.contains("default"));
    assert_eq!(alsa.description, "boot default");
    assert_eq!(alsa.status, ServiceStatus::Unknown);
}

#[test]
fn openrc_update_skips_lines_without_pipe() {
    let out = ["acpid default", "  ", "bluetooth | default"].join("\n");
    let svcs = parse_openrc_update(&out);
    assert_eq!(svcs.len(), 1);
    assert_eq!(svcs[0].name, "bluetooth");
}

#[test]
fn openrc_parsers_never_issue_authority_for_pattern_or_path_names() {
    let status = [
        "valid-name [ started ]",
        "wild*card [ started ]",
        "../escape [ started ]",
        "semi;colon [ started ]",
    ]
    .join("\n");
    assert_eq!(
        parse_openrc_status(&status)
            .into_iter()
            .map(|service| service.name)
            .collect::<Vec<_>>(),
        ["valid-name"]
    );

    let update = [
        "valid-name | default",
        "wild?card | default",
        "../escape | default",
        "semi;colon | default",
    ]
    .join("\n");
    assert_eq!(
        parse_openrc_update(&update)
            .into_iter()
            .map(|service| service.name)
            .collect::<Vec<_>>(),
        ["valid-name"]
    );
}

#[test]
fn service_status_maps_openrc_and_systemd_keywords() {
    // OpenRC rc-status states.
    assert_eq!(ServiceStatus::from("started"), ServiceStatus::Active);
    assert_eq!(ServiceStatus::from("stopped"), ServiceStatus::Inactive);
    assert_eq!(ServiceStatus::from("crashed"), ServiceStatus::Failed);
    // systemd active-column keywords stay mapped (regression guard).
    assert_eq!(ServiceStatus::from("running"), ServiceStatus::Active);
    assert_eq!(ServiceStatus::from("active"), ServiceStatus::Active);
    assert_eq!(ServiceStatus::from("inactive"), ServiceStatus::Inactive);
    assert_eq!(ServiceStatus::from("dead"), ServiceStatus::Inactive);
    assert_eq!(ServiceStatus::from("failed"), ServiceStatus::Failed);
    // Unknown for anything unrecognised (case-insensitive).
    assert_eq!(ServiceStatus::from("flummoxed"), ServiceStatus::Unknown);
    assert_eq!(ServiceStatus::from("STARTED"), ServiceStatus::Active);
}

#[test]
fn detect_init_returns_known_variant() {
    // Live detection is allowed to be unsupported in containers and on hosts
    // running an init backend that this provider does not implement. A missing
    // or inaccessible `/proc` remains a typed probe failure.
    let init = ServiceManager::detect_init();
    assert!(matches!(
        init,
        Ok(InitSystem::Systemd | InitSystem::Openrc | InitSystem::Unsupported)
            | Err(taskmanager_core::FailureKind::MissingDependency)
            | Err(taskmanager_core::FailureKind::PermissionDenied)
            | Err(taskmanager_core::FailureKind::ProviderFault)
    ));
}

// ── parse_systemctl_show_deps ────────────────────────────────────────────
// The pure parser behind ServiceManager::fetch_deps. Mock `systemctl show`
// key=value output (no --value); verifies typed edges, compatibility
// projections, key matching, empty values, ordering, and suffix rejection.

#[test]
fn parse_deps_reads_every_typed_relation() {
    let out = [
        "Requires=sysinit.target basic.target",
        "Wants=display-manager.service",
        "Requisite=local-fs.target",
        "BindsTo=device.target",
        "PartOf=application.target",
        "Conflicts=shutdown.target",
        "Before=graphical.target",
        "After=network.target nss-lookup.target",
        "WantedBy=graphical.target",
        "RequiredBy=workload.target",
        "UpheldBy=guardian.service",
    ]
    .join("\n");
    let d = parse_systemctl_show_deps(&out);
    for (kind, target) in [
        (ServiceRelationKind::Requires, "sysinit.target"),
        (ServiceRelationKind::Requires, "basic.target"),
        (ServiceRelationKind::Wants, "display-manager.service"),
        (ServiceRelationKind::Requisite, "local-fs.target"),
        (ServiceRelationKind::BindsTo, "device.target"),
        (ServiceRelationKind::PartOf, "application.target"),
        (ServiceRelationKind::Conflicts, "shutdown.target"),
        (ServiceRelationKind::Before, "graphical.target"),
        (ServiceRelationKind::After, "network.target"),
        (ServiceRelationKind::After, "nss-lookup.target"),
        (ServiceRelationKind::WantedBy, "graphical.target"),
        (ServiceRelationKind::RequiredBy, "workload.target"),
        (ServiceRelationKind::UpheldBy, "guardian.service"),
    ] {
        assert!(
            d.relations().edges().contains(&ServiceRelationEdge::new(
                kind,
                super::target::systemd_unit_id(target),
            )),
            "missing typed relation target {target}"
        );
    }
    assert_eq!(d.relations().len(), 13);
}

#[test]
fn parse_deps_ignores_unrelated_keys() {
    // `systemctl show` without `-p` (or with extra `-p` props) emits many
    // other keys (Id=, Description=, ...); only relationship properties are read.
    let out = [
        "Id=foo.service",
        "Requires=basic.target",
        "Description=Foo service",
        "After=network.target",
        "Wants=",
        "WantedBy=multi-user.target",
        "LoadState=loaded",
    ]
    .join("\n");
    let d = parse_systemctl_show_deps(&out);
    assert_eq!(d.relations().len(), 3);
    assert!(d.relations().contains_kind(&ServiceRelationKind::Requires));
    assert!(d.relations().contains_kind(&ServiceRelationKind::After));
    assert!(d.relations().contains_kind(&ServiceRelationKind::WantedBy));
    assert!(!d.relations().contains_kind(&ServiceRelationKind::Wants));
}

#[test]
fn parse_deps_empty_value_yields_empty_string() {
    // When a dependency directive is absent systemd prints `Key=` (empty).
    // This must not manufacture an empty edge or placeholder target.
    let out = ["Requires=", "Wants=", "WantedBy=", "After="].join("\n");
    let d = parse_systemctl_show_deps(&out);
    assert_eq!(d, ServiceDeps::default());
}

#[test]
fn parse_deps_missing_keys_default_empty() {
    // systemctl was asked for 4 props but returned none of them (e.g. the
    // unit is not loaded → `systemctl show` prints nothing useful). All four
    // relationship graph stays at its empty default.
    let d = parse_systemctl_show_deps("");
    assert_eq!(d, ServiceDeps::default());
}

#[test]
fn parse_deps_tolerates_whitespace_and_blank_lines() {
    // Real `systemctl show` output has no leading whitespace, but the parser
    // trims each line defensively; stray blank lines between keys are skipped.
    let out = [
        "",
        "Requires=basic.target",
        "   ",
        "After=network.target",
        "",
    ]
    .join("\n");
    let d = parse_systemctl_show_deps(&out);
    assert!(d.relations().edges().contains(&ServiceRelationEdge::new(
        ServiceRelationKind::Requires,
        super::target::systemd_unit_id("basic.target")
    )));
    assert!(d.relations().edges().contains(&ServiceRelationEdge::new(
        ServiceRelationKind::After,
        super::target::systemd_unit_id("network.target")
    )));
}

#[test]
fn parse_deps_rejects_suffixed_keys() {
    // `Requires=` must not match `Requires_override=` etc. — strict prefix
    // matching, mirroring parse_unit_description's Description= discipline.
    let out = [
        "Requires_override=decoy.target",
        "Requires=real.target",
        "Afterwards=network.target",
        "After=basic.target",
    ]
    .join("\n");
    let d = parse_systemctl_show_deps(&out);
    assert_eq!(d.relations().len(), 2);
    assert!(d.relations().edges().contains(&ServiceRelationEdge::new(
        ServiceRelationKind::Requires,
        super::target::systemd_unit_id("real.target")
    )));
    assert!(d.relations().edges().contains(&ServiceRelationEdge::new(
        ServiceRelationKind::After,
        super::target::systemd_unit_id("basic.target")
    )));
}

#[test]
fn parse_deps_never_issues_relation_authority_for_patterns_or_paths() {
    let d = parse_systemctl_show_deps(
        "Requires=safe.service wild*.service ../escape.service semi;colon.service\n",
    );
    assert_eq!(
        d.relation_targets(&ServiceRelationKind::Requires)
            .map(|target| target.as_str())
            .collect::<Vec<_>>(),
        [super::target::systemd_service_id("safe.service").as_str()]
    );
}

#[test]
fn parse_deps_last_value_wins_for_repeated_key() {
    // If a key appears more than once (shouldn't happen with `systemctl show`
    // but is robust behaviour), the last occurrence wins — matches how the
    // systemd Description parser treats duplicate directives.
    let d = parse_systemctl_show_deps("Requires=a.target\nRequires=b.target\n");
    assert_eq!(
        d.relation_targets(&ServiceRelationKind::Requires)
            .map(|target| target.as_str())
            .collect::<Vec<_>>(),
        [super::target::systemd_unit_id("b.target").as_str()]
    );
}

#[test]
fn parse_deps_trims_trailing_whitespace_in_values() {
    // Defensive: a stray trailing space must not become a target fragment.
    let d = parse_systemctl_show_deps("Requires=basic.target   \n");
    assert_eq!(
        d.relation_targets(&ServiceRelationKind::Requires)
            .map(|target| target.as_str())
            .collect::<Vec<_>>(),
        [super::target::systemd_unit_id("basic.target").as_str()]
    );
}

#[test]
fn parse_deps_is_case_sensitive() {
    // systemctl property names are case-sensitive (capitalised). A lowercase
    // `requires=` must NOT match `Requires=` — this is the systemd parser.
    assert_eq!(
        parse_systemctl_show_deps("requires=basic.target\n"),
        ServiceDeps::default()
    );
}

#[test]
fn service_deps_default_is_all_empty() {
    let d = ServiceDeps::default();
    assert!(d.relations().is_empty());
}

#[test]
fn service_item_default_has_empty_typed_relations() {
    // Inventory scanning does not fetch details, so its canonical relation
    // graph is honestly empty instead of a writable string sentinel.
    let item = ServiceItem::default();
    assert!(item.name.is_empty());
    assert_eq!(item.status, ServiceStatus::Unknown);
    assert!(item.relations().is_empty());
}

// ── bounded service log provider ────────────────────────────────────────

#[test]
fn service_log_fixture_is_limited_to_the_latest_fifty_lines() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/service_journal.log"
    ));
    let state = classify_service_log_outcome(ServiceLogCommandOutcome::Exited {
        success: true,
        stdout: fixture.to_string(),
        stderr: String::new(),
    });
    let ServiceLogState::Ready(lines) = state else {
        panic!("fixture must classify as ready");
    };
    assert_eq!(lines.len(), SERVICE_LOG_LINE_LIMIT);
    assert!(lines.first().is_some_and(|line| line.ends_with("line 06")));
    assert!(lines.last().is_some_and(|line| line.ends_with("line 55")));
}

#[test]
fn service_log_timeout_has_an_explicit_state() {
    let failure =
        ServiceLogFailure::with_detail(ServiceLogErrorKind::TimedOut, "journalctl timed out");
    assert_eq!(
        classify_service_log_outcome(ServiceLogCommandOutcome::Failure(failure.clone())),
        ServiceLogState::Unavailable(failure)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn bounded_log_command_kills_a_timed_out_child() {
    let mut command = Command::new("sh");
    command.args(["-c", "while :; do :; done"]);
    let started = Instant::now();
    let ServiceLogCommandOutcome::Failure(failure) =
        run_command_with_timeout(command, Duration::from_millis(20))
    else {
        panic!("non-terminating command must fail");
    };
    assert_eq!(failure.kind, ServiceLogErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[cfg(target_os = "linux")]
#[test]
fn missing_log_tool_is_not_a_generic_provider_failure() {
    let command = Command::new("/definitely-missing-taskmanager-journalctl");
    let ServiceLogCommandOutcome::Failure(failure) =
        run_command_with_timeout(command, Duration::from_millis(20))
    else {
        panic!("missing executable must fail to start");
    };

    assert_eq!(failure.kind, ServiceLogErrorKind::MissingTool);
    assert!(failure.detail.is_some());
}

#[test]
fn service_log_permission_failure_is_not_a_generic_error() {
    let state = classify_service_log_outcome(ServiceLogCommandOutcome::Exited {
        success: false,
        stdout: String::new(),
        stderr: "Failed to open journal: Permission denied".into(),
    });
    assert!(matches!(state, ServiceLogState::Unavailable(failure)
        if failure.kind == ServiceLogErrorKind::PermissionDenied
            && failure.detail.as_deref().is_some_and(|detail| detail.contains("Permission denied"))));
}

#[test]
fn nonzero_log_command_is_a_typed_provider_failure() {
    let state = classify_service_log_outcome(ServiceLogCommandOutcome::Exited {
        success: false,
        stdout: String::new(),
        stderr: "journal database is corrupt".into(),
    });
    assert!(matches!(state, ServiceLogState::Unavailable(failure)
        if failure.kind == ServiceLogErrorKind::ProviderFailed
            && failure.detail.as_deref() == Some("journal database is corrupt")));
}

#[test]
fn service_log_empty_and_copy_states_are_actionable() {
    let empty = classify_service_log_outcome(ServiceLogCommandOutcome::Exited {
        success: true,
        stdout: "  \n\n".into(),
        stderr: String::new(),
    });
    assert_eq!(empty, ServiceLogState::Empty);
    assert!(empty.copy_text().is_none());

    let ready = ServiceLogState::from_lines(vec!["one".into(), "two".into()]);
    assert_eq!(ready.copy_text().as_deref(), Some("one\ntwo"));
}

#[test]
fn service_log_worker_transitions_loading_to_result_without_blocking_request() {
    let timed_out =
        ServiceLogFailure::with_detail(ServiceLogErrorKind::TimedOut, "journalctl timed out");
    let loader_failure = timed_out.clone();
    let worker = ServiceLogWorker::with_loader(move |_| {
        ServiceLogState::Unavailable(loader_failure.clone())
    });
    let target = target::systemd_service_id("fixture-service.service");
    assert_eq!(
        worker.request(target.clone()),
        ServiceLogSnapshot {
            service_id: target.clone(),
            state: ServiceLogState::Loading,
        }
    );

    let deadline = Instant::now() + Duration::from_secs(1);
    let result = loop {
        if let Some(result) = worker
            .try_recv_latest()
            .expect("service log worker remains connected")
        {
            break result;
        }
        assert!(Instant::now() < deadline, "worker result timed out");
        thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(result.service_id, target);
    assert_eq!(result.state, ServiceLogState::Unavailable(timed_out));
}

#[test]
fn disconnected_log_worker_returns_typed_provider_failure() {
    let worker = ServiceLogWorker::disconnected();
    let target = target::systemd_service_id("fixture-service.service");
    let snapshot = worker.request(target.clone());
    assert_eq!(snapshot.service_id, target);
    assert!(
        matches!(snapshot.state, ServiceLogState::Unavailable(failure)
        if failure.kind == ServiceLogErrorKind::TemporarilyUnavailable
            && failure.detail.as_deref().is_some_and(|detail| detail.contains("unavailable")))
    );
    let receive_failure = worker
        .try_recv_latest()
        .expect_err("disconnected result channel must not look idle");
    assert_eq!(
        receive_failure.kind,
        ServiceLogErrorKind::TemporarilyUnavailable
    );
}
