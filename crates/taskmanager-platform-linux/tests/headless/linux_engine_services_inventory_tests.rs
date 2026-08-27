use std::collections::VecDeque;

use super::*;

const SYSTEMD_VALID: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/systemctl_services_valid.txt"
));
const SYSTEMD_MALFORMED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/systemctl_services_malformed.txt"
));
const OPENRC_VALID: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/openrc_status_valid.txt"
));
const OPENRC_EMPTY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/openrc_status_empty.txt"
));
const OPENRC_UPDATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/openrc_update_valid.txt"
));

struct FakeRunner {
    results: VecDeque<InventoryCommandResult>,
    calls: Vec<String>,
}

impl FakeRunner {
    fn new(results: impl IntoIterator<Item = InventoryCommandResult>) -> Self {
        Self {
            results: results.into_iter().collect(),
            calls: Vec::new(),
        }
    }
}

impl InventoryCommandRunner for FakeRunner {
    fn run(&mut self, program: &str, _args: &[&str]) -> InventoryCommandResult {
        self.calls.push(program.to_string());
        self.results
            .pop_front()
            .unwrap_or(InventoryCommandResult::Failure(FailureKind::ProviderFault))
    }
}

fn snapshot(init: InitSystem, runner: &mut FakeRunner) -> PartialSourceSnapshot<ServiceItem> {
    ServiceManager::scan_snapshot_with(init, runner, Vec::new)
}

#[test]
fn runtime_selection_runs_only_the_selected_backend() {
    let mut systemd = FakeRunner::new([InventoryCommandResult::Success(SYSTEMD_VALID.to_string())]);
    let systemd_snapshot = snapshot(InitSystem::Systemd, &mut systemd);
    assert_eq!(systemd.calls, ["systemctl"]);
    assert_eq!(
        systemd_snapshot.sources[0].provider,
        SYSTEMD_INVENTORY_PROVIDER
    );
    assert_eq!(
        systemd_snapshot.sources[0].outcome,
        SourceOutcome::Available
    );

    let mut openrc = FakeRunner::new([InventoryCommandResult::Success(OPENRC_VALID.to_string())]);
    let openrc_snapshot = snapshot(InitSystem::Openrc, &mut openrc);
    assert_eq!(openrc.calls, ["rc-status"]);
    assert_eq!(
        openrc_snapshot.sources[0].provider,
        OPENRC_INVENTORY_PROVIDER
    );
    assert_eq!(openrc_snapshot.sources[0].outcome, SourceOutcome::Available);

    let mut unsupported = FakeRunner::new([]);
    let unsupported_snapshot = snapshot(InitSystem::Unsupported, &mut unsupported);
    assert!(unsupported.calls.is_empty());
    assert_eq!(
        unsupported_snapshot.sources[0].outcome,
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    );
}

#[test]
fn init_probe_failure_runs_no_backend_and_remains_typed() {
    for failure in [
        FailureKind::MissingDependency,
        FailureKind::PermissionDenied,
        FailureKind::TimedOut,
        FailureKind::ProviderFault,
    ] {
        let mut runner = FakeRunner::new([]);
        let snapshot =
            ServiceManager::scan_snapshot_from_detection(Err(failure), &mut runner, Vec::new);
        assert!(runner.calls.is_empty());
        assert!(snapshot.items.is_empty());
        assert_eq!(snapshot.sources[0].provider, INIT_DETECTION_PROVIDER);
        assert_eq!(
            snapshot.sources[0].outcome,
            SourceOutcome::Unavailable(failure)
        );
    }
}

#[test]
fn true_empty_inventory_is_distinct_from_every_command_failure() {
    let mut empty = FakeRunner::new([InventoryCommandResult::Success(String::new())]);
    assert_eq!(
        snapshot(InitSystem::Systemd, &mut empty).sources[0].outcome,
        SourceOutcome::Empty
    );

    for failure in [
        FailureKind::MissingDependency,
        FailureKind::PermissionDenied,
        FailureKind::TimedOut,
        FailureKind::Rejected,
    ] {
        let mut failed = FakeRunner::new([InventoryCommandResult::Failure(failure)]);
        assert_eq!(
            snapshot(InitSystem::Systemd, &mut failed).sources[0].outcome,
            SourceOutcome::Unavailable(failure)
        );
    }

    let mut malformed = FakeRunner::new([InventoryCommandResult::Success(
        SYSTEMD_MALFORMED.to_string(),
    )]);
    assert_eq!(
        snapshot(InitSystem::Systemd, &mut malformed).sources[0].outcome,
        SourceOutcome::Unavailable(FailureKind::ProviderFault)
    );
}

#[test]
fn systemd_command_failure_with_unit_file_rows_is_partial() {
    let mut runner = FakeRunner::new([InventoryCommandResult::Failure(FailureKind::TimedOut)]);
    let snapshot = ServiceManager::scan_snapshot_with(InitSystem::Systemd, &mut runner, || {
        vec![ServiceItem::from_inventory(
            "",
            "fallback",
            ServiceStatus::Unknown,
            "",
            "",
            "",
            "",
        )]
    });
    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(
        snapshot.sources[0].outcome,
        SourceOutcome::Partial(FailureKind::TimedOut)
    );
}

#[test]
fn openrc_headers_only_are_empty_without_invoking_fallback() {
    let mut runner = FakeRunner::new([InventoryCommandResult::Success(OPENRC_EMPTY.to_string())]);
    let snapshot = snapshot(InitSystem::Openrc, &mut runner);
    assert_eq!(runner.calls, ["rc-status"]);
    assert_eq!(snapshot.sources[0].outcome, SourceOutcome::Empty);
}

#[test]
fn openrc_fallback_retains_primary_failure_and_never_runs_systemd() {
    let mut runner = FakeRunner::new([
        InventoryCommandResult::Failure(FailureKind::MissingDependency),
        InventoryCommandResult::Success(OPENRC_UPDATE.to_string()),
    ]);
    let snapshot = snapshot(InitSystem::Openrc, &mut runner);
    assert_eq!(runner.calls, ["rc-status", "rc-update"]);
    assert_eq!(snapshot.items.len(), 2);
    assert_eq!(
        snapshot.sources[0].outcome,
        SourceOutcome::Partial(FailureKind::MissingDependency)
    );
}

#[test]
fn openrc_inventory_recovers_after_tool_disappearance_and_service_hot_change() {
    let first = "alpha [ started ]\n";
    let second = "alpha [ stopped ]\nbeta [ started ]\n";
    let mut runner = FakeRunner::new([
        InventoryCommandResult::Failure(FailureKind::MissingDependency),
        InventoryCommandResult::Failure(FailureKind::MissingDependency),
        InventoryCommandResult::Success(first.to_owned()),
        InventoryCommandResult::Success(second.to_owned()),
    ]);

    let unavailable = snapshot(InitSystem::Openrc, &mut runner);
    assert_eq!(
        unavailable.sources[0].outcome,
        SourceOutcome::Unavailable(FailureKind::MissingDependency)
    );
    let recovered = snapshot(InitSystem::Openrc, &mut runner);
    assert_eq!(recovered.sources[0].outcome, SourceOutcome::Available);
    assert_eq!(recovered.items.len(), 1);
    let changed = snapshot(InitSystem::Openrc, &mut runner);
    assert_eq!(changed.sources[0].outcome, SourceOutcome::Available);
    assert_eq!(changed.items.len(), 2);
    assert_eq!(changed.items[0].status, ServiceStatus::Inactive);
}

#[test]
fn mixed_valid_and_malformed_rows_are_partial_not_available() {
    let mixed = format!("{SYSTEMD_VALID}\nmalformed row");
    let mut runner = FakeRunner::new([InventoryCommandResult::Success(mixed)]);
    let snapshot = snapshot(InitSystem::Systemd, &mut runner);
    assert_eq!(snapshot.items.len(), 2);
    assert_eq!(
        snapshot.sources[0].outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
}

#[test]
fn forged_systemd_pattern_row_is_partial_and_never_receives_authority() {
    let output = "safe.service loaded active running Safe service\n\
                      wildcard*.service loaded active running Forged pattern\n";
    let mut runner = FakeRunner::new([InventoryCommandResult::Success(output.to_owned())]);
    let snapshot = snapshot(InitSystem::Systemd, &mut runner);

    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items[0].name, "safe");
    assert_eq!(
        snapshot.sources[0].outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
}

#[test]
fn forged_openrc_name_is_partial_and_never_receives_authority() {
    let output = "safe-name [ started ]\nwild*card [ started ]\n";
    let mut runner = FakeRunner::new([InventoryCommandResult::Success(output.to_owned())]);
    let snapshot = snapshot(InitSystem::Openrc, &mut runner);

    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items[0].name, "safe-name");
    assert_eq!(
        snapshot.sources[0].outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
}

#[test]
fn command_failures_are_classified_without_parsing_stderr_text() {
    assert_eq!(
        classify_spawn_error(io::ErrorKind::NotFound),
        FailureKind::MissingDependency
    );
    assert_eq!(
        classify_spawn_error(io::ErrorKind::PermissionDenied),
        FailureKind::PermissionDenied
    );
    assert_eq!(classify_nonzero_exit(), FailureKind::Rejected);
}
