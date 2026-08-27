use std::collections::VecDeque;

use taskmanager_core::SourceOutcome;

use super::super::BLAME_PROVIDER_ID;
use super::*;

const SYSTEMD_UNITS: &str = "alpha.service enabled enabled\nbeta.service enabled enabled\n";
const OPENRC_UPDATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/openrc_update_valid.txt"
));

fn openrc_entry(enabled: bool) -> StartupEntry {
    StartupEntry {
        id: native_startup_id(StartupSource::RunLevel, "demo"),
        name: "demo".to_owned(),
        exec: "rc-service demo start".to_owned(),
        enabled,
        source: StartupSource::RunLevel,
        scope: StartupScope::System,
        control_policy: StartupControlPolicy::Direct,
        locator: "demo".into(),
        impact: StartupImpact::None,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented,
        },
    }
}

#[test]
fn init_source_selection_is_mutually_exclusive_at_runtime() {
    let manager = StartupManager::new();

    let mut systemd_calls = Vec::new();
    let mut systemd_runner = |program: &str, _args: &[&str], _timeout: Duration| {
        systemd_calls.push(program.to_string());
        let stdout = if program == "systemctl" {
            SYSTEMD_UNITS
        } else {
            "100ms alpha.service\n"
        };
        InventoryCommandResult::Success(stdout.to_string())
    };
    let systemd = manager.scan_selected_init_sources_with(InitSystem::Systemd, &mut systemd_runner);
    assert_eq!(systemd_calls, ["systemctl", "systemd-analyze"]);
    assert_eq!(systemd.entries.len(), 2);
    assert_eq!(systemd.sources[0].provider, SYSTEMD_PROVIDER_ID);
    assert_eq!(systemd.sources[1].provider, BLAME_PROVIDER_ID);

    let mut openrc_calls = Vec::new();
    let mut openrc_runner = |program: &str, _args: &[&str], _timeout: Duration| {
        openrc_calls.push(program.to_string());
        InventoryCommandResult::Success(OPENRC_UPDATE.to_string())
    };
    let openrc = manager.scan_selected_init_sources_with(InitSystem::Openrc, &mut openrc_runner);
    assert_eq!(openrc_calls, ["rc-update"]);
    assert!(
        openrc
            .entries
            .iter()
            .all(|entry| entry.source == StartupSource::RunLevel)
    );
    assert_eq!(openrc.sources[0].provider, OPENRC_PROVIDER_ID);

    let mut unsupported_calls = Vec::new();
    let mut unsupported_runner = |program: &str, _args: &[&str], _timeout: Duration| {
        unsupported_calls.push(program.to_string());
        InventoryCommandResult::Failure(FailureKind::ProviderFault)
    };
    let unsupported =
        manager.scan_selected_init_sources_with(InitSystem::Unsupported, &mut unsupported_runner);
    assert!(unsupported_calls.is_empty());
    assert_eq!(
        unsupported.sources[0].outcome,
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    );
}

#[test]
fn startup_parsers_distinguish_empty_malformed_and_partial() {
    let (empty, empty_malformed) = parse_systemd_startup("");
    assert!(empty.is_empty());
    assert!(!empty_malformed);

    let (malformed, malformed_flag) = parse_systemd_startup("not a unit row");
    assert!(malformed.is_empty());
    assert!(malformed_flag);

    let (partial, partial_flag) =
        parse_systemd_startup("alpha.service enabled enabled\nbroken row\n");
    assert_eq!(partial.len(), 1);
    assert!(partial_flag);

    let (forged, forged_flag) =
        parse_systemd_startup("safe.service enabled enabled\nwild*.service enabled enabled\n");
    assert_eq!(forged.len(), 1);
    assert_eq!(forged[0].name, "safe.service");
    assert!(forged_flag);
}

#[test]
fn systemd_inventory_preserves_disabled_and_read_only_unit_states() {
    let (entries, malformed) = parse_systemd_startup(
        "enabled.service enabled enabled\n\
             disabled.service disabled enabled\n\
             static.service static -\n",
    );
    assert!(!malformed);
    assert_eq!(entries.len(), 3);
    assert!(entries[0].enabled);
    assert_eq!(entries[0].control_policy, StartupControlPolicy::Direct);
    assert!(!entries[1].enabled);
    assert_eq!(entries[1].control_policy, StartupControlPolicy::Direct);
    assert!(!entries[2].enabled);
    assert_eq!(entries[2].control_policy, StartupControlPolicy::Unsupported);
}

#[test]
fn systemd_preflight_matches_exact_unit_and_current_state() {
    let output = "alpha.service enabled enabled\nalpha-helper.service disabled enabled\n";
    assert_eq!(systemd_unit_enabled(output, "alpha.service"), Some(true));
    assert_eq!(
        systemd_unit_enabled(output, "alpha-helper.service"),
        Some(false)
    );
    assert_eq!(systemd_unit_enabled(output, "missing.service"), None);
}

#[test]
fn verbose_openrc_inventory_keeps_disabled_services_controllable() {
    let output = "enabled | boot default\ndisabled |             \n";
    let mut runner = |program: &str, args: &[&str], _timeout: Duration| {
        assert_eq!(program, "rc-update");
        assert_eq!(args, ["-v", "show"]);
        InventoryCommandResult::Success(output.into())
    };
    let (entries, status) = scan_openrc_startup(&mut runner);

    assert_eq!(status.outcome, SourceOutcome::Available);
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .find(|entry| entry.name == "enabled")
            .is_some_and(|entry| entry.enabled)
    );
    assert!(
        entries
            .iter()
            .find(|entry| entry.name == "disabled")
            .is_some_and(
                |entry| !entry.enabled && entry.control_policy == StartupControlPolicy::Direct
            )
    );
}

#[test]
fn openrc_startup_distinguishes_empty_malformed_permission_and_recovery() {
    for (result, expected) in [
        (
            InventoryCommandResult::Success(String::new()),
            SourceOutcome::Empty,
        ),
        (
            InventoryCommandResult::Success("broken row\n".to_owned()),
            SourceOutcome::Unavailable(FailureKind::ProviderFault),
        ),
        (
            InventoryCommandResult::Failure(FailureKind::PermissionDenied),
            SourceOutcome::Unavailable(FailureKind::PermissionDenied),
        ),
        (
            InventoryCommandResult::Failure(FailureKind::MissingDependency),
            SourceOutcome::Unavailable(FailureKind::MissingDependency),
        ),
    ] {
        let mut result = Some(result);
        let mut runner = |_program: &str, _args: &[&str], _timeout: Duration| {
            result
                .take()
                .unwrap_or(InventoryCommandResult::Failure(FailureKind::ProviderFault))
        };
        let (entries, status) = scan_openrc_startup(&mut runner);
        assert!(entries.is_empty());
        assert_eq!(status.outcome, expected);
    }

    let mut outcomes = VecDeque::from([
        InventoryCommandResult::Failure(FailureKind::MissingDependency),
        InventoryCommandResult::Success("demo | default\n".to_owned()),
    ]);
    let mut runner = |_program: &str, _args: &[&str], _timeout: Duration| {
        outcomes
            .pop_front()
            .unwrap_or(InventoryCommandResult::Failure(FailureKind::ProviderFault))
    };
    assert_eq!(
        scan_openrc_startup(&mut runner).1.outcome,
        SourceOutcome::Unavailable(FailureKind::MissingDependency)
    );
    let (recovered, status) = scan_openrc_startup(&mut runner);
    assert_eq!(status.outcome, SourceOutcome::Available);
    assert_eq!(recovered.len(), 1);
    assert!(recovered[0].enabled);
}

#[test]
fn openrc_control_uses_exact_runlevel_args_and_rechecks_runtime_before_mutation() {
    let manager = StartupManager::new();
    let entry = openrc_entry(false);
    let mut detections = VecDeque::from([Ok(InitSystem::Openrc), Ok(InitSystem::Openrc)]);
    let mut detector = || {
        detections
            .pop_front()
            .unwrap_or(Err(FailureKind::ProviderFault))
    };
    let mut calls = Vec::new();
    let mut results = VecDeque::from([
        InventoryCommandResult::Success("demo | \n".to_owned()),
        InventoryCommandResult::Success(String::new()),
    ]);
    let mut runner = |program: &str, args: &[&str], _timeout: Duration| {
        calls.push((
            program.to_owned(),
            args.iter().map(|argument| (*argument).to_owned()).collect(),
        ));
        results
            .pop_front()
            .unwrap_or(InventoryCommandResult::Failure(FailureKind::ProviderFault))
    };

    assert_eq!(
        manager.set_init_source_enabled_with(&entry, true, &mut detector, &mut runner,),
        Ok(())
    );
    assert_eq!(
        calls,
        [
            (
                "rc-update".to_owned(),
                vec!["-v".to_owned(), "show".to_owned()],
            ),
            (
                "rc-update".to_owned(),
                vec!["add".to_owned(), "demo".to_owned(), "default".to_owned(),],
            ),
        ]
    );
}

#[test]
fn init_switch_or_tool_loss_never_mutates_a_stale_startup_target() {
    let manager = StartupManager::new();
    let entry = openrc_entry(false);

    let mut detections = VecDeque::from([Ok(InitSystem::Openrc), Ok(InitSystem::Systemd)]);
    let mut detector = || {
        detections
            .pop_front()
            .unwrap_or(Err(FailureKind::ProviderFault))
    };
    let mut calls = Vec::new();
    let mut runner = |program: &str, args: &[&str], _timeout: Duration| {
        calls.push((program.to_owned(), args.len()));
        InventoryCommandResult::Success("demo | \n".to_owned())
    };
    assert_eq!(
        manager.set_init_source_enabled_with(&entry, true, &mut detector, &mut runner,),
        Err(ProviderFailure::IdentityChanged)
    );
    assert_eq!(calls, [("rc-update".to_owned(), 2)]);

    let mut detector = || Ok(InitSystem::Openrc);
    let mut calls = Vec::new();
    let mut runner = |program: &str, _args: &[&str], _timeout: Duration| {
        calls.push(program.to_owned());
        InventoryCommandResult::Failure(FailureKind::MissingDependency)
    };
    assert_eq!(
        manager.set_init_source_enabled_with(&entry, true, &mut detector, &mut runner,),
        Err(ProviderFailure::MissingDependency)
    );
    assert_eq!(calls, ["rc-update"]);
}

#[test]
fn native_startup_locators_cannot_be_reinterpreted_as_options_or_paths() {
    for valid in [
        "alpha.service",
        "worker@42.service",
        "dbus-org.demo.service",
    ] {
        assert!(valid_systemd_user_unit(valid), "{valid}");
    }
    for invalid in [
        "",
        "-force.service",
        "../escape.service",
        "nested/name.service",
        "not-a-service.timer",
        "white space.service",
        "wild*.service",
        "query?.service",
        "semi;colon.service",
    ] {
        assert!(!valid_systemd_user_unit(invalid), "{invalid}");
    }
    assert!(valid_openrc_service("networking"));
    assert!(!valid_openrc_service("--help"));
    assert!(!valid_openrc_service("../networking"));
    assert!(!valid_openrc_service("wild*card"));
    assert!(!valid_openrc_service("semi;colon"));
}

#[test]
fn command_failure_mapping_preserves_provider_failure_kind() {
    for (failure, expected) in [
        (FailureKind::Unsupported, ProviderFailure::Unsupported),
        (
            FailureKind::PermissionDenied,
            ProviderFailure::PermissionDenied,
        ),
        (
            FailureKind::MissingDependency,
            ProviderFailure::MissingDependency,
        ),
        (FailureKind::TimedOut, ProviderFailure::TimedOut),
        (
            FailureKind::IdentityChanged,
            ProviderFailure::IdentityChanged,
        ),
        (
            FailureKind::TemporarilyUnavailable,
            ProviderFailure::TemporarilyUnavailable,
        ),
        (FailureKind::Rejected, ProviderFailure::Rejected),
        (FailureKind::ProviderFault, ProviderFailure::ProviderFault),
    ] {
        assert_eq!(provider_failure(failure), expected);
    }
}
