// test-intent: behavior
//! Headless behaviour tests for the Containers page's DECLARED quota
//! (CORE-46): the page compares a container against the ceilings the
//! container DECLARED, never against a cap the page invented.
//!
//! The claims under test:
//! - the over-limit state is raised ONLY for a declared ceiling whose
//!   observed share the observation actually crossed;
//! - a container that declares nothing produces no over-limit state and no
//!   percentage;
//! - a declaration the platform could not read reads "unknown" — never a
//!   "0%" under-the-limit state;
//! - an overrun stays visible: the share is never clamped to 100%;
//! - the declared CPU budget is honoured, and a `cpu.max` that names no
//!   budget declares no ceiling at all.

use super::*;

use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process_telemetry::{
    ContainerCpuQuota, ContainerQuota, ContainerSummary, IsolationKind,
};

const GIB: u64 = 1024 * 1024 * 1024;

/// One container summary with an explicit declared quota.
fn summary_with_quota(
    name: &str,
    cpu_percent: Option<f32>,
    memory_bytes: Option<u64>,
    quota: ContainerQuota,
) -> ContainerSummary {
    ContainerSummary {
        id: format!("/docker/{name}"),
        name: name.to_owned(),
        runtime: Some(IsolationKind::Docker),
        cgroup_path: format!("/docker/{name}"),
        cpu_percentage: cpu_percent.map_or_else(
            || ScalarObservation::unavailable(FailureKind::TimedOut),
            |value| ScalarObservation::available(value, 1_000),
        ),
        memory_bytes: memory_bytes.map_or_else(
            || ScalarObservation::unavailable(FailureKind::TimedOut),
            |value| ScalarObservation::available(value, 1_000),
        ),
        member_pids: vec![42],
        quota,
    }
}

/// A quota that declares a soft memory ceiling.
fn soft_ceiling(bytes: u64) -> ContainerQuota {
    ContainerQuota {
        memory_hard_limit_bytes: None,
        memory_soft_limit_bytes: Some(bytes),
        cpu: None,
        failure: None,
    }
}

/// A quota that declares a CPU budget of whole CPUs.
fn cpu_budget(cpus: u64, period_us: u64) -> ContainerQuota {
    ContainerQuota {
        memory_hard_limit_bytes: None,
        memory_soft_limit_bytes: None,
        cpu: Some(ContainerCpuQuota {
            quota_us: Some(cpus * period_us / 1_000),
            period_us,
        }),
        failure: None,
    }
}

#[test]
fn a_declared_ceiling_the_observation_crossed_raises_the_over_limit_state() {
    let summary = summary_with_quota("abc", Some(10.0), Some(2 * GIB), soft_ceiling(GIB));
    let view = container_quota_view(&summary);
    assert_eq!(view.state, ContainerQuotaState::Declared);
    let breach = view
        .breach()
        .expect("the observed share crossed the ceiling");
    assert!(
        breach
            .share_percent()
            .is_some_and(|share| (share - 200.0).abs() < 0.01),
        "the breach carries the observed share: {breach:?}"
    );
}

#[test]
fn a_container_that_declares_nothing_raises_no_over_limit_state() {
    let summary = summary_with_quota(
        "abc",
        Some(10.0),
        Some(2 * GIB),
        ContainerQuota::not_declared(),
    );
    let view = container_quota_view(&summary);
    assert_eq!(view.state, ContainerQuotaState::NotDeclared);
    assert!(
        view.breach().is_none(),
        "an undeclared ceiling produces no over-limit state"
    );
    assert_eq!(
        view.memory_share_percent, None,
        "an undeclared ceiling produces no percentage"
    );
}

#[test]
fn a_declaration_the_platform_could_not_read_reads_unknown() {
    let summary = summary_with_quota(
        "abc",
        Some(10.0),
        Some(2 * GIB),
        ContainerQuota::unavailable(FailureKind::TimedOut),
    );
    let view = container_quota_view(&summary);
    assert_eq!(view.state, ContainerQuotaState::Unknown);
    assert!(
        view.breach().is_none(),
        "an unreadable declaration raises no over-limit state"
    );
    let readout = quota_readout(&view);
    assert!(
        !readout.contains('%'),
        "an unknown declaration states no percentage: {readout}"
    );
}

#[test]
fn an_overrun_is_not_clamped_to_the_ceiling() {
    let summary = summary_with_quota("abc", Some(10.0), Some(3 * GIB), soft_ceiling(GIB));
    let view = container_quota_view(&summary);
    let share = view.memory_share_percent.expect("the share is observed");
    assert!(
        (share - 300.0).abs() < 0.01,
        "the share is the observed one, not a clamped 100: {share}"
    );
    let breach = view.breach().expect("the overrun is a breach");
    assert!(
        breach.is_deep_overrun(),
        "a run twice past the ceiling takes the deeper mark"
    );
}

#[test]
fn the_declared_cpu_budget_is_honoured_and_a_max_budget_declares_no_ceiling() {
    let summary = summary_with_quota("abc", Some(250.0), Some(GIB), cpu_budget(1, 100_000));
    let view = container_quota_view(&summary);
    let breach = view
        .breach()
        .expect("the observed load is above the budget");
    assert!(
        matches!(breach, ContainerQuotaBreach::Cpu { .. }),
        "the breach names the CPU dimension: {breach:?}"
    );

    // A `cpu.max` of "max" names NO budget: there is no CPU ceiling, so the
    // load can never be over it.
    let summary = summary_with_quota(
        "abc",
        Some(250.0),
        Some(GIB),
        ContainerQuota {
            memory_hard_limit_bytes: None,
            memory_soft_limit_bytes: None,
            cpu: Some(ContainerCpuQuota {
                quota_us: None,
                period_us: 100_000,
            }),
            failure: None,
        },
    );
    let view = container_quota_view(&summary);
    assert_eq!(view.cpu_budget_cpus, None);
    assert!(
        view.breach().is_none(),
        "a max budget declares no ceiling, so there is no over-limit state"
    );
}
