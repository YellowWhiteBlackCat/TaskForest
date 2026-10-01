//! Canonical current-value fold for process resource-limit observations.

use taskmanager_core::{LimitValue, ProcessResourceSnapshot, ResourceLimitKind};

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
    pub open_files_soft_limit: Option<LimitValue>,
    pub open_files_hard_limit: Option<LimitValue>,
}

impl ProjectedProcessResources<'_> {
    /// Current memory consumption as a percentage of the finite memory limit.
    /// Unlimited limits and missing measurements remain `None`.
    #[must_use]
    pub fn memory_usage_percent(&self) -> Option<f32> {
        self.memory_limit
            .and_then(|limit| limit.usage_percent(self.memory_usage_bytes))
    }

    /// Current process-count consumption as a percentage of the finite PID
    /// limit. A measured zero remains a real `0.0%`.
    #[must_use]
    pub fn process_usage_percent(&self) -> Option<f32> {
        self.process_limit
            .and_then(|limit| limit.usage_percent(self.process_count))
    }

    /// Current open-files descriptor consumption as a percentage of the finite
    /// soft limit.
    #[must_use]
    pub fn open_files_soft_usage_percent(&self, count: u64) -> Option<f32> {
        self.open_files_soft_limit
            .and_then(|limit| limit.usage_percent(Some(count)))
    }

    /// Return whether the descriptor count has reached or exceeded a caller-supplied
    /// warning band (such as `90.0` for 90%) of the finite soft limit.
    #[must_use]
    pub fn is_near_soft_open_files_limit(
        &self,
        count: u64,
        threshold_percent: f32,
    ) -> Option<bool> {
        if !threshold_percent.is_finite() || threshold_percent < 0.0 {
            return None;
        }
        self.open_files_soft_usage_percent(count)
            .map(|percent| percent >= threshold_percent)
    }
}

/// Fold typed resource observations into the immutable facts renderers need.
#[must_use]
pub fn project_process_resources(
    resources: &ProcessResourceSnapshot,
) -> ProjectedProcessResources<'_> {
    let (open_files_soft_limit, open_files_hard_limit) = resources
        .current_limits()
        .into_iter()
        .flatten()
        .find(|limit| limit.kind == ResourceLimitKind::OpenFiles)
        .map_or((None, None), |limit| (Some(limit.soft), Some(limit.hard)));

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
        open_files_soft_limit,
        open_files_hard_limit,
    }
}
