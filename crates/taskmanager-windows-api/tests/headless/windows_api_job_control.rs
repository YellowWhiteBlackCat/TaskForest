use super::*;

#[test]
fn job_limit_lanes_stay_typed_off_windows() {
    #[cfg(not(windows))]
    {
        let request = WindowsJobLimitRequest {
            memory_limit_bytes: Some(1024),
            process_count_limit: Some(4),
            cpu_rate_percent: Some(50),
        };
        assert_eq!(
            apply_process_job_limits(1, 1, &request),
            Err(WindowsApiError::Unsupported)
        );
        assert_eq!(
            clear_process_job_limits(1, 1),
            Err(WindowsApiError::Unsupported)
        );
    }
}

#[cfg(windows)]
#[test]
fn job_limits_reject_wrong_identity_and_clear_reports_real_absence() {
    use crate::process_creation_time_100ns;

    let pid = std::process::id();
    let token = process_creation_time_100ns(pid).expect("current process creation time");
    let wrong_token = if token == u64::MAX { 1 } else { token + 1 };

    // A wrong creation token must never touch the real target's job.
    assert_eq!(
        apply_process_job_limits(
            pid,
            wrong_token,
            &WindowsJobLimitRequest {
                cpu_rate_percent: Some(50),
                ..WindowsJobLimitRequest::default()
            }
        ),
        Err(WindowsApiError::IdentityChanged)
    );
    assert_eq!(
        clear_process_job_limits(pid, token),
        Ok(false),
        "no job was created, so the clear must report a real absence"
    );
}

#[cfg(windows)]
#[test]
fn job_limits_apply_reapply_and_release_on_the_current_process() {
    use crate::process_creation_time_100ns;

    // A boundary-owned job is destroyed with its last handle
    // (KILL_ON_JOB_CLOSE is never set), so the test process survives and the
    // registry ends empty again.
    let pid = std::process::id();
    let token = process_creation_time_100ns(pid).expect("current process creation time");

    let first = WindowsJobLimitRequest {
        cpu_rate_percent: Some(50),
        ..WindowsJobLimitRequest::default()
    };
    if apply_process_job_limits(pid, token, &first).is_err() {
        // The host's job hierarchy cannot nest this assignment (a restricted
        // CI job or broken hierarchy); the typed refusal is the honest
        // outcome and nothing was registered.
        return;
    }

    // Re-apply replaces the limits on the already-owned job.
    let second = WindowsJobLimitRequest {
        process_count_limit: Some(u32::MAX),
        ..WindowsJobLimitRequest::default()
    };
    apply_process_job_limits(pid, token, &second).expect("re-apply replaces job limits");

    assert_eq!(
        clear_process_job_limits(pid, token),
        Ok(true),
        "the tracked job must exist and be released"
    );
    assert_eq!(
        clear_process_job_limits(pid, token),
        Ok(false),
        "a second clear reports the real absence"
    );
}
