//! Behavior tests for the job-object limit translation in the Windows
//! process-control adapter.

use super::*;

use taskmanager_core::{LimitValue, ResourceGroupCpuLimit, ResourceGroupLimitRequest};
use taskmanager_windows_api::WindowsJobLimitRequest;

fn cpu_limit(quota: LimitValue, period_micros: u64) -> ResourceGroupLimitRequest {
    ResourceGroupLimitRequest {
        cpu: Some(ResourceGroupCpuLimit {
            quota,
            period_micros,
        }),
        ..ResourceGroupLimitRequest::default()
    }
}

#[test]
fn job_limit_translation_maps_values_and_selects_the_clear_path() {
    // Exact dimensions map through unchanged.
    let mixed = ResourceGroupLimitRequest {
        memory: Some(LimitValue::Value(2 * 1024 * 1024 * 1024)),
        cpu: Some(ResourceGroupCpuLimit {
            quota: LimitValue::Value(50_000),
            period_micros: 100_000,
        }),
        processes: Some(LimitValue::Value(8)),
    };
    assert_eq!(
        WinProcessResourceControlProvider::job_limit_request(&mixed),
        Ok(Some(WindowsJobLimitRequest {
            memory_limit_bytes: Some(2 * 1024 * 1024 * 1024),
            process_count_limit: Some(8),
            cpu_rate_percent: Some(50),
        }))
    );

    // An absent or unlimited-everything request selects the clear path.
    assert_eq!(
        WinProcessResourceControlProvider::job_limit_request(&ResourceGroupLimitRequest::default()),
        Ok(None)
    );
    let relaxed = ResourceGroupLimitRequest {
        memory: Some(LimitValue::Unlimited),
        processes: Some(LimitValue::Unlimited),
        cpu: Some(ResourceGroupCpuLimit {
            quota: LimitValue::Unlimited,
            period_micros: 100_000,
        }),
    };
    assert_eq!(
        WinProcessResourceControlProvider::job_limit_request(&relaxed),
        Ok(None)
    );
}

#[test]
fn job_limit_translation_refuses_what_a_job_cannot_represent() {
    // A 2.5-core quota is not a whole percent and must not be rounded.
    let fractional = cpu_limit(LimitValue::Value(250_000), 100_000);
    assert_eq!(
        WinProcessResourceControlProvider::job_limit_request(&fractional),
        Err(ProviderFailure::Unsupported)
    );

    // Zero period, zero quota, and multi-core rates are equally unrepresentable.
    for (quota, period) in [
        (LimitValue::Value(50_000), 0),
        (LimitValue::Value(0), 100_000),
        (LimitValue::Value(200_000), 100_000),
    ] {
        assert_eq!(
            WinProcessResourceControlProvider::job_limit_request(&cpu_limit(quota, period)),
            Err(ProviderFailure::Unsupported),
            "quota {quota:?}/period {period} must be refused, not coerced"
        );
    }
}
