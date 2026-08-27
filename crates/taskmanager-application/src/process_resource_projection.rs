//! Canonical current-value fold for process resource-limit observations.

use taskmanager_core::{LimitValue, ProcessResourceSnapshot};

/// Borrowed renderer input for one process's resource limits.
///
/// Availability is folded exactly once here. Frontends therefore cannot
/// disagree about whether stale, unavailable, or partial provider facts are
/// eligible for a current-value readout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectedProcessResources<'a> {
    pub memory_usage_bytes: Option<u64>,
    pub memory_limit: Option<LimitValue>,
    pub cpu_time_quota_micros: Option<LimitValue>,
    pub cpu_time_period_micros: Option<u64>,
    pub process_count: Option<u64>,
    pub process_limit: Option<LimitValue>,
    pub resource_group: Option<&'a str>,
}

/// Fold typed resource observations into the immutable facts renderers need.
#[must_use]
pub fn project_process_resources(
    resources: &ProcessResourceSnapshot,
) -> ProjectedProcessResources<'_> {
    ProjectedProcessResources {
        memory_usage_bytes: resources.current_memory_usage_bytes(),
        memory_limit: resources.current_memory_limit(),
        cpu_time_quota_micros: resources.current_cpu_time_quota_micros(),
        cpu_time_period_micros: resources.current_cpu_time_period_micros(),
        process_count: resources.current_process_count(),
        process_limit: resources.current_process_limit(),
        resource_group: resources
            .current_resource_groups()
            .into_iter()
            .flatten()
            .map(|membership| membership.native_locator.as_str())
            .find(|locator| !locator.is_empty()),
    }
}
