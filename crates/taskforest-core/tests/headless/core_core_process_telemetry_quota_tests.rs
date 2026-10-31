//! test-intent: behavior
//!
//! The container quota contract has three honest states: nothing declared,
//! a measured ceiling, and an unreadable declaration. Every test below pins
//! one of them so a frontend can never mistake "unknown" for "unlimited" or
//! read a zero-byte ceiling out of an absent declaration.

use super::*;
use crate::core::failure::FailureKind;

#[test]
fn an_undeclared_quota_carries_no_ceiling_in_any_dimension() {
    let quota = ContainerQuota::not_declared();

    assert_eq!(quota.memory_hard_limit_bytes, None);
    assert_eq!(quota.memory_soft_limit_bytes, None);
    assert_eq!(quota.cpu, None);
    assert_eq!(quota.failure, None);
    assert!(!quota.declares_any_ceiling());
    assert_eq!(quota.soft_memory_usage_percent(Some(1_048_576)), None);
}

#[test]
fn a_measured_quota_publishes_only_the_declared_dimensions() {
    let quota = ContainerQuota {
        memory_hard_limit_bytes: Some(2 * 1024 * 1024 * 1024),
        memory_soft_limit_bytes: Some(1024 * 1024 * 1024),
        cpu: Some(ContainerCpuQuota {
            quota_us: Some(200_000),
            period_us: 100_000,
        }),
        failure: None,
    };

    assert!(quota.declares_any_ceiling());
    assert_eq!(quota.memory_hard_limit_bytes, Some(2 * 1024 * 1024 * 1024));
    assert_eq!(quota.memory_soft_limit_bytes, Some(1024 * 1024 * 1024));
    assert_eq!(quota.cpu.and_then(|cpu| cpu.cpus()), Some(2.0));
}

#[test]
fn an_unreadable_declaration_is_unknown_and_never_a_ceiling() {
    let quota = ContainerQuota::unavailable(FailureKind::PermissionDenied);

    assert_eq!(quota.failure, Some(FailureKind::PermissionDenied));
    assert_eq!(quota.memory_hard_limit_bytes, None);
    assert_eq!(quota.memory_soft_limit_bytes, None);
    assert_eq!(quota.cpu, None);
    assert!(!quota.declares_any_ceiling());
    // An unknown declaration has no usage percentage: a frontend must not
    // claim the container is under (or over) a ceiling it never read.
    assert_eq!(quota.soft_memory_usage_percent(Some(4_000_000_000)), None);
}

#[test]
fn the_soft_ceiling_drives_the_usage_percentage_and_keeps_an_overrun_visible() {
    let quota = ContainerQuota {
        memory_soft_limit_bytes: Some(1_000),
        ..ContainerQuota::not_declared()
    };

    assert_eq!(quota.soft_memory_usage_percent(Some(500)), Some(50.0));
    assert_eq!(quota.soft_memory_usage_percent(Some(2_000)), Some(200.0));
    // Unknown current usage stays unknown instead of becoming 0%.
    assert_eq!(quota.soft_memory_usage_percent(None), None);
}

#[test]
fn a_cpu_quota_without_a_budget_reports_no_ceiling_but_keeps_its_period() {
    let quota = ContainerCpuQuota {
        quota_us: None,
        period_us: 100_000,
    };

    assert_eq!(quota.cpus(), None);
    assert_eq!(quota.period_us, 100_000);
}

#[test]
fn a_cpu_quota_over_an_unusable_period_never_reports_a_zero_cpu_quota() {
    let quota = ContainerCpuQuota {
        quota_us: Some(100_000),
        period_us: 0,
    };

    assert_eq!(quota.cpus(), None);
}
