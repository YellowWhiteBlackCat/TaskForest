use super::*;

fn authorized_memory_plan(identity: ProcessIdentity) -> AuthorizedCgroupLimitPlan {
    authorize_cgroup_limit_plan(
        plan_cgroup_limits(
            identity,
            &unified_membership(),
            CgroupLimitRequest {
                memory_max: Some(LimitValue::Unlimited),
                ..Default::default()
            },
        )
        .unwrap(),
        CgroupLimitConfirmation {
            identity,
            allow_write: true,
        },
    )
    .unwrap()
}

#[test]
fn cgroup_apply_rechecks_pid_identity_before_any_write() {
    let identity = ProcessIdentity {
        pid: 42,
        start_token: 900,
    };
    let mut io = MemoryCgroupIo {
        start_time_ticks: 901,
        values: std::collections::HashMap::from([(CgroupLimitFile::Memory, "100".into())]),
        ..Default::default()
    };

    assert!(matches!(
        apply_cgroup_limit_plan_with(&authorized_memory_plan(identity), &mut io),
        Err(CgroupLimitApplyError::IdentityChanged { .. })
    ));
    assert!(io.writes.is_empty());
}

#[test]
fn cgroup_apply_clamps_membership_read_between_identity_checks() {
    let identity = ProcessIdentity {
        pid: 42,
        start_token: 900,
    };
    let mut io = MemoryCgroupIo {
        start_time_ticks: 900,
        start_time_after_membership_read: Some(901),
        values: std::collections::HashMap::from([(CgroupLimitFile::Memory, "100".into())]),
        ..Default::default()
    };

    assert!(matches!(
        apply_cgroup_limit_plan_with(&authorized_memory_plan(identity), &mut io),
        Err(CgroupLimitApplyError::IdentityChanged {
            observed_start_time_ticks: 901,
            ..
        })
    ));
    assert_eq!(io.value_reads, 0);
    assert!(io.writes.is_empty());
}

#[test]
fn cgroup_apply_rechecks_pid_identity_after_pre_reads_before_any_write() {
    let identity = ProcessIdentity {
        pid: 42,
        start_token: 900,
    };
    let mut io = MemoryCgroupIo {
        start_time_ticks: 900,
        values: std::collections::HashMap::from([(CgroupLimitFile::Memory, "100".into())]),
        start_time_after_value_read: Some(901),
        ..Default::default()
    };

    assert!(matches!(
        apply_cgroup_limit_plan_with(&authorized_memory_plan(identity), &mut io),
        Err(CgroupLimitApplyError::IdentityChanged {
            observed_start_time_ticks: 901,
            ..
        })
    ));
    assert!(io.writes.is_empty());
}

#[test]
fn cgroup_apply_rechecks_membership_after_pre_reads_before_any_write() {
    let identity = ProcessIdentity {
        pid: 42,
        start_token: 900,
    };
    let mut io = MemoryCgroupIo {
        start_time_ticks: 900,
        values: std::collections::HashMap::from([(CgroupLimitFile::Memory, "100".into())]),
        group_after_value_read: Some(Some("/user.slice/moved.scope".into())),
        ..Default::default()
    };

    assert_eq!(
        apply_cgroup_limit_plan_with(&authorized_memory_plan(identity), &mut io),
        Err(CgroupLimitApplyError::TargetChanged {
            expected_group_path: "/user.slice/app.scope".into(),
            observed_group_path: Some("/user.slice/moved.scope".into()),
        })
    );
    assert!(io.writes.is_empty());
}
