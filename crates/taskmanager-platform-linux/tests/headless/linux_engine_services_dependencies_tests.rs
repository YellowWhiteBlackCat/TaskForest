use std::collections::VecDeque;

use super::*;

struct FakeRunner {
    outcomes: VecDeque<DependencyCommandResult>,
    calls: Vec<(String, Vec<String>)>,
}

impl FakeRunner {
    fn new(outcomes: impl IntoIterator<Item = DependencyCommandResult>) -> Self {
        Self {
            outcomes: outcomes.into_iter().collect(),
            calls: Vec::new(),
        }
    }
}

impl DependencyCommandRunner for FakeRunner {
    fn run(&mut self, program: &str, args: &[&str]) -> DependencyCommandResult {
        self.calls.push((
            program.to_string(),
            args.iter().map(|arg| (*arg).to_string()).collect(),
        ));
        self.outcomes
            .pop_front()
            .unwrap_or(DependencyCommandResult::Failure(
                ProviderFailure::ProviderFault,
            ))
    }
}

#[test]
fn successful_empty_systemd_response_is_authoritative() {
    let mut runner = FakeRunner::new([DependencyCommandResult::Success(
        [
            "Requires=",
            "Wants=",
            "Requisite=",
            "BindsTo=",
            "PartOf=",
            "Conflicts=",
            "Before=",
            "After=",
            "WantedBy=",
            "RequiredBy=",
            "UpheldBy=",
        ]
        .join("\n"),
    )]);

    assert_eq!(
        ServiceManager::fetch_deps_with(InitSystem::Systemd, "demo.service", &mut runner),
        Ok(ServiceDeps::default())
    );
    assert_eq!(
        runner.calls,
        [(
            "systemctl".to_string(),
            vec![
                "show".to_string(),
                "-p".to_string(),
                SYSTEMD_DEPENDENCY_PROPERTIES.to_string(),
                "--".to_string(),
                "demo.service".to_string(),
            ],
        )]
    );
}

#[test]
fn every_command_failure_remains_distinct_from_empty_dependencies() {
    for failure in [
        ProviderFailure::MissingDependency,
        ProviderFailure::PermissionDenied,
        ProviderFailure::TimedOut,
        ProviderFailure::Rejected,
        ProviderFailure::ProviderFault,
    ] {
        let mut runner = FakeRunner::new([DependencyCommandResult::Failure(failure)]);
        assert_eq!(
            ServiceManager::fetch_deps_with(InitSystem::Systemd, "demo.service", &mut runner,),
            Err(failure)
        );
    }
}

#[test]
fn unsupported_init_backends_never_run_systemctl() {
    for init in [InitSystem::Openrc, InitSystem::Unsupported] {
        let mut runner = FakeRunner::new([]);
        assert_eq!(
            ServiceManager::fetch_deps_with(init, "demo.service", &mut runner),
            Err(ProviderFailure::Unsupported)
        );
        assert!(runner.calls.is_empty());
    }
}

#[test]
fn a_later_request_recovers_after_a_transient_failure() {
    let mut runner = FakeRunner::new([
        DependencyCommandResult::Failure(ProviderFailure::TimedOut),
        DependencyCommandResult::Success("Requires=network.target\n".to_string()),
    ]);

    assert_eq!(
        ServiceManager::fetch_deps_with(InitSystem::Systemd, "demo.service", &mut runner),
        Err(ProviderFailure::TimedOut)
    );
    let recovered =
        ServiceManager::fetch_deps_with(InitSystem::Systemd, "demo.service", &mut runner)
            .expect("the later successful query must recover");
    assert_eq!(
        recovered
            .relation_targets(&taskmanager_core::ServiceRelationKind::Requires)
            .map(|target| target.as_str())
            .collect::<Vec<_>>(),
        [crate::engine::services::target::systemd_unit_id("network.target").as_str()]
    );
    assert_eq!(runner.calls.len(), 2);
}

#[test]
fn bounded_failures_and_provider_exit_are_typed() {
    assert_eq!(
        classify_spawn_error(io::ErrorKind::NotFound),
        ProviderFailure::MissingDependency
    );
    assert_eq!(
        classify_spawn_error(io::ErrorKind::PermissionDenied),
        ProviderFailure::PermissionDenied
    );
    assert_eq!(
        classify_spawn_error(io::ErrorKind::Other),
        ProviderFailure::ProviderFault
    );
    assert_eq!(
        classify_nonzero_exit(b"Failed to connect: Access denied"),
        ProviderFailure::PermissionDenied
    );
    assert_eq!(
        classify_nonzero_exit(b"Unit demo.service could not be found"),
        ProviderFailure::Rejected
    );
}

#[cfg(unix)]
#[test]
fn native_runner_classifies_missing_timeout_nonzero_and_invalid_utf8() {
    assert!(matches!(
        run_dependency_command(
            "taskmanager-definitely-missing-systemctl",
            &[],
            Duration::from_millis(50),
        ),
        DependencyCommandResult::Failure(ProviderFailure::MissingDependency)
    ));
    assert!(matches!(
        run_dependency_command("sleep", &["1"], Duration::from_millis(20)),
        DependencyCommandResult::Failure(ProviderFailure::TimedOut)
    ));
    assert!(matches!(
        run_dependency_command("false", &[], Duration::from_secs(1)),
        DependencyCommandResult::Failure(ProviderFailure::Rejected)
    ));
    assert!(matches!(
        run_dependency_command("printf", &["\\377"], Duration::from_secs(1)),
        DependencyCommandResult::Failure(ProviderFailure::ProviderFault)
    ));
}
