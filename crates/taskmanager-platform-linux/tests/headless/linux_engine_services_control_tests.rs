use std::collections::VecDeque;

use super::*;
use crate::engine::services::target::{openrc_service_id, systemd_service_id};

struct FakeRunner {
    results: VecDeque<ControlCommandResult>,
    calls: Vec<(String, Vec<String>)>,
}

impl FakeRunner {
    fn new(results: impl IntoIterator<Item = ControlCommandResult>) -> Self {
        Self {
            results: results.into_iter().collect(),
            calls: Vec::new(),
        }
    }
}

impl ControlCommandRunner for FakeRunner {
    fn run(&mut self, program: &str, args: &[&str]) -> ControlCommandResult {
        self.calls.push((
            program.to_owned(),
            args.iter().map(|argument| (*argument).to_owned()).collect(),
        ));
        self.results
            .pop_front()
            .unwrap_or(ControlCommandResult::Failure(
                ProviderFailure::ProviderFault,
            ))
    }
}

fn resolved(init: InitSystem, native: &str) -> ResolvedServiceTarget {
    let id = match init {
        InitSystem::Systemd => systemd_service_id(native),
        InitSystem::Openrc => openrc_service_id(native),
        InitSystem::Unsupported => panic!("unsupported init has no target"),
    };
    resolve_service_target(&id).expect("canonical fixture target")
}

#[test]
fn openrc_control_revalidates_identity_and_uses_documented_arguments() {
    for (action, program, args) in [
        (ServiceAction::Start, "rc-service", vec!["demo", "start"]),
        (ServiceAction::Stop, "rc-service", vec!["demo", "stop"]),
        (
            ServiceAction::Restart,
            "rc-service",
            vec!["demo", "restart"],
        ),
        (
            ServiceAction::Enable,
            "rc-update",
            vec!["add", "demo", "default"],
        ),
        (
            ServiceAction::Disable,
            "rc-update",
            vec!["del", "demo", "default"],
        ),
    ] {
        let mut runner = FakeRunner::new([
            ControlCommandResult::Success(String::new()),
            ControlCommandResult::Success(String::new()),
        ]);
        let mut detector = || Ok(InitSystem::Openrc);
        assert_eq!(
            control_service_with(
                &resolved(InitSystem::Openrc, "demo"),
                action,
                &mut detector,
                &mut runner,
            ),
            Ok(())
        );
        assert_eq!(
            runner.calls,
            [
                (
                    "rc-service".to_owned(),
                    vec!["--exists".to_owned(), "demo".to_owned()],
                ),
                (
                    program.to_owned(),
                    args.into_iter().map(str::to_owned).collect(),
                ),
            ]
        );
    }
}

#[test]
fn systemd_control_preserves_exact_unit_identity() {
    let mut runner = FakeRunner::new([
        ControlCommandResult::Success("loaded\n".to_owned()),
        ControlCommandResult::Success(String::new()),
    ]);
    let mut detector = || Ok(InitSystem::Systemd);
    assert_eq!(
        control_service_with(
            &resolved(InitSystem::Systemd, "demo.service"),
            ServiceAction::Restart,
            &mut detector,
            &mut runner,
        ),
        Ok(())
    );
    assert_eq!(
        runner.calls,
        [
            (
                "systemctl".to_owned(),
                vec![
                    "show".to_owned(),
                    "--property=LoadState".to_owned(),
                    "--value".to_owned(),
                    "--".to_owned(),
                    "demo.service".to_owned(),
                ],
            ),
            (
                "systemctl".to_owned(),
                vec![
                    "restart".to_owned(),
                    "--".to_owned(),
                    "demo.service".to_owned(),
                ],
            ),
        ]
    );
}

#[test]
fn missing_or_replaced_targets_never_reach_mutation() {
    for init in [InitSystem::Systemd, InitSystem::Openrc] {
        let mut runner =
            FakeRunner::new([ControlCommandResult::Failure(ProviderFailure::Rejected)]);
        let mut detector = || Ok(init);
        let native = if init == InitSystem::Systemd {
            "demo.service"
        } else {
            "demo"
        };
        assert_eq!(
            control_service_with(
                &resolved(init, native),
                ServiceAction::Start,
                &mut detector,
                &mut runner,
            ),
            Err(ProviderFailure::IdentityChanged)
        );
        assert_eq!(runner.calls.len(), 1);
    }
}

#[test]
fn stale_or_midflight_backend_switch_never_reinterprets_or_mutates_target() {
    let target = resolved(InitSystem::Openrc, "demo");
    let mut runner = FakeRunner::new([]);
    let mut detector = || Ok(InitSystem::Systemd);
    assert_eq!(
        control_service_with(&target, ServiceAction::Start, &mut detector, &mut runner,),
        Err(ProviderFailure::IdentityChanged)
    );
    assert!(runner.calls.is_empty());

    let mut detections = VecDeque::from([Ok(InitSystem::Openrc), Ok(InitSystem::Systemd)]);
    let mut detector = || {
        detections
            .pop_front()
            .unwrap_or(Err(FailureKind::ProviderFault))
    };
    let mut runner = FakeRunner::new([ControlCommandResult::Success(String::new())]);
    assert_eq!(
        control_service_with(&target, ServiceAction::Start, &mut detector, &mut runner,),
        Err(ProviderFailure::IdentityChanged)
    );
    assert_eq!(runner.calls.len(), 1);
}

#[test]
fn command_failures_are_typed_and_later_requests_recover() {
    assert_eq!(
        classify_spawn_error(io::ErrorKind::NotFound),
        ProviderFailure::MissingDependency
    );
    assert_eq!(
        classify_spawn_error(io::ErrorKind::PermissionDenied),
        ProviderFailure::PermissionDenied
    );
    assert_eq!(
        classify_nonzero_exit(b"Access denied"),
        ProviderFailure::PermissionDenied
    );

    let mut runner = FakeRunner::new([
        ControlCommandResult::Failure(ProviderFailure::TimedOut),
        ControlCommandResult::Success(String::new()),
        ControlCommandResult::Success(String::new()),
    ]);
    let mut detector = || Ok(InitSystem::Openrc);
    let target = resolved(InitSystem::Openrc, "demo");
    assert_eq!(
        control_service_with(&target, ServiceAction::Start, &mut detector, &mut runner,),
        Err(ProviderFailure::TimedOut)
    );
    assert_eq!(
        control_service_with(&target, ServiceAction::Start, &mut detector, &mut runner,),
        Ok(())
    );
}

#[cfg(unix)]
#[test]
fn native_command_runner_classifies_missing_permission_nonzero_and_timeout() {
    assert_eq!(
        run_control_command(
            "/definitely-missing-taskmanager-service-tool",
            &[],
            Duration::from_millis(20),
        ),
        ControlCommandResult::Failure(ProviderFailure::MissingDependency)
    );
    assert_eq!(
        run_control_command("sleep", &["1"], Duration::from_millis(20)),
        ControlCommandResult::Failure(ProviderFailure::TimedOut)
    );
    assert_eq!(
        run_control_command(
            "sh",
            &["-c", "printf 'Permission denied' >&2; exit 1"],
            Duration::from_secs(1),
        ),
        ControlCommandResult::Failure(ProviderFailure::PermissionDenied)
    );
    assert_eq!(
        run_control_command("false", &[], Duration::from_secs(1)),
        ControlCommandResult::Failure(ProviderFailure::Rejected)
    );
}
