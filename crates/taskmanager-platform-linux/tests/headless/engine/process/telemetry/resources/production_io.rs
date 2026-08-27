use super::*;

/// The production `LinuxCgroupPlanIo` must apply a plan end-to-end against a
/// fake hierarchy. No root is required: the executor accepts explicit roots.
#[test]
#[cfg(target_os = "linux")]
fn production_cgroup_io_applies_limits_against_fake_filesystem() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-cgroup-write-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let proc_dir = root.join("proc/42");
    let group_dir = root.join("cgroup/user.slice/app.scope");
    std::fs::create_dir_all(&proc_dir).unwrap();
    std::fs::create_dir_all(&group_dir).unwrap();
    std::fs::write(
        proc_dir.join("stat"),
        "42 (worker) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 900 20",
    )
    .unwrap();
    std::fs::write(proc_dir.join("cgroup"), "0::/user.slice/app.scope\n").unwrap();
    std::fs::write(group_dir.join("memory.max"), "max\n").unwrap();
    std::fs::write(group_dir.join("pids.max"), "32\n").unwrap();

    let identity = ProcessIdentity {
        pid: 42,
        start_token: 900,
    };
    let authorized = authorize_cgroup_limit_plan(
        plan_cgroup_limits(
            identity,
            &unified_membership(),
            CgroupLimitRequest {
                memory_max: Some(LimitValue::Value(1_048_576)),
                pids_max: Some(LimitValue::Value(16)),
                ..Default::default()
            },
        )
        .unwrap(),
        CgroupLimitConfirmation {
            identity,
            allow_write: true,
        },
    )
    .unwrap();

    let mut io = LinuxCgroupPlanIo::with_roots(root.join("proc"), root.join("cgroup"));
    apply_cgroup_limit_plan_with(&authorized, &mut io).unwrap();

    assert_eq!(
        std::fs::read_to_string(group_dir.join("memory.max")).unwrap(),
        "1048576"
    );
    assert_eq!(
        std::fs::read_to_string(group_dir.join("pids.max")).unwrap(),
        "16"
    );

    std::fs::remove_file(group_dir.join("pids.max")).unwrap();
    assert_eq!(
        io.read_value("/user.slice/app.scope", CgroupLimitFile::Pids),
        Err(CgroupIoError::NotFound)
    );

    std::fs::remove_dir_all(root).ok();
}
