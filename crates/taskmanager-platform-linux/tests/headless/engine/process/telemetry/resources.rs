use super::observation::collect_from_roots;
use super::*;

impl LinuxCgroupPlanIo {
    fn with_roots(proc_root: PathBuf, cgroup_root: PathBuf) -> Self {
        Self {
            proc_root,
            cgroup_root,
        }
    }
}

const LIMITS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/proc_limits.txt"
));
const CGROUP: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/proc_cgroup.txt"
));

#[test]
fn limits_fixture_preserves_unlimited_and_units() {
    let limits = parse_proc_limits(LIMITS);
    assert_eq!(limits.len(), 3);
    assert_eq!(limits[0].kind, ResourceLimitKind::CpuTime);
    assert_eq!(limits[0].soft, LimitValue::Unlimited);
    assert_eq!(limits[1].soft, LimitValue::Value(1_048_576));
    assert_eq!(limits[2].kind, ResourceLimitKind::OpenFiles);
}

#[test]
fn cgroup_fixture_parses_v2_and_v1_memberships() {
    let memberships = parse_proc_cgroup(CGROUP);
    assert_eq!(memberships.len(), 2);
    assert!(memberships[0].capabilities.is_empty());
    assert_eq!(memberships[0].native_locator, "/user.slice/app.scope");
    assert_eq!(memberships[1].capabilities, ["cpu", "cpuacct"]);
}

#[test]
fn v2_provider_reads_values_and_preserves_max_as_unlimited() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-process-cgroup-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let proc_dir = root.join("proc");
    let group = root.join("cgroup/app.scope");
    std::fs::create_dir_all(&proc_dir).unwrap();
    std::fs::create_dir_all(&group).unwrap();
    std::fs::write(proc_dir.join("limits"), LIMITS).unwrap();
    std::fs::write(proc_dir.join("cgroup"), "0::/app.scope\n").unwrap();
    std::fs::write(group.join("memory.current"), "4096\n").unwrap();
    std::fs::write(group.join("memory.max"), "max\n").unwrap();
    std::fs::write(group.join("cpu.max"), "50000 100000\n").unwrap();
    std::fs::write(group.join("pids.current"), "7\n").unwrap();
    std::fs::write(group.join("pids.max"), "32\n").unwrap();
    let snapshot = collect_from_roots(&proc_dir, &root.join("cgroup"), 10);
    assert_eq!(snapshot.current_memory_usage_bytes(), Some(4096));
    assert_eq!(snapshot.current_memory_limit(), Some(LimitValue::Unlimited));
    assert_eq!(
        snapshot.current_cpu_time_quota_micros(),
        Some(LimitValue::Value(50_000))
    );
    assert_eq!(snapshot.current_cpu_time_period_micros(), Some(100_000));
    assert_eq!(snapshot.current_process_count(), Some(7));
    assert_eq!(
        snapshot.current_process_limit(),
        Some(LimitValue::Value(32))
    );
    std::fs::remove_dir_all(root).ok();
}

struct MemoryCgroupIo {
    start_time_ticks: u64,
    group_path: Option<String>,
    values: std::collections::HashMap<CgroupLimitFile, String>,
    fail_once_on: Option<CgroupLimitFile>,
    writes: Vec<(CgroupLimitFile, String)>,
    value_reads: usize,
    membership_reads: usize,
    start_time_after_value_read: Option<u64>,
    start_time_after_membership_read: Option<u64>,
    group_after_value_read: Option<Option<String>>,
}

impl Default for MemoryCgroupIo {
    fn default() -> Self {
        Self {
            start_time_ticks: 0,
            group_path: Some("/user.slice/app.scope".into()),
            values: std::collections::HashMap::new(),
            fail_once_on: None,
            writes: Vec::new(),
            value_reads: 0,
            membership_reads: 0,
            start_time_after_value_read: None,
            start_time_after_membership_read: None,
            group_after_value_read: None,
        }
    }
}

impl CgroupPlanIo for MemoryCgroupIo {
    fn read_start_time_ticks(&mut self, _pid: u32) -> Result<u64, CgroupIoError> {
        if self.value_reads > 0
            && let Some(start_time_ticks) = self.start_time_after_value_read
        {
            return Ok(start_time_ticks);
        }
        if self.membership_reads > 0
            && let Some(start_time_ticks) = self.start_time_after_membership_read
        {
            return Ok(start_time_ticks);
        }
        Ok(self.start_time_ticks)
    }

    fn read_unified_group(&mut self, _pid: u32) -> Result<Option<String>, CgroupIoError> {
        self.membership_reads = self.membership_reads.saturating_add(1);
        if self.value_reads > 0
            && let Some(group_path) = &self.group_after_value_read
        {
            return Ok(group_path.clone());
        }
        Ok(self.group_path.clone())
    }

    fn read_value(
        &mut self,
        _group_path: &str,
        target: CgroupLimitFile,
    ) -> Result<String, CgroupIoError> {
        self.value_reads = self.value_reads.saturating_add(1);
        self.values
            .get(&target)
            .cloned()
            .ok_or(CgroupIoError::NotFound)
    }

    fn write_value(
        &mut self,
        _group_path: &str,
        target: CgroupLimitFile,
        value: &str,
    ) -> Result<(), CgroupIoError> {
        if self.fail_once_on == Some(target) {
            self.fail_once_on = None;
            return Err(CgroupIoError::PermissionDenied);
        }
        self.values.insert(target, value.to_owned());
        self.writes.push((target, value.to_owned()));
        Ok(())
    }
}

fn unified_membership() -> Vec<CgroupMembership> {
    vec![CgroupMembership {
        provider: ProviderId::borrowed("linux.cgroup"),
        native_hierarchy_id: Some(0),
        capabilities: Vec::new(),
        native_locator: "/user.slice/app.scope".into(),
    }]
}

#[test]
fn cgroup_plan_is_typed_and_requires_matching_explicit_confirmation() {
    let identity = ProcessIdentity {
        pid: 42,
        start_token: 900,
    };
    let plan = plan_cgroup_limits(
        identity,
        &unified_membership(),
        CgroupLimitRequest {
            memory_max: Some(LimitValue::Value(1_048_576)),
            cpu_max: Some(CgroupCpuLimit {
                quota_us: LimitValue::Unlimited,
                period_us: 100_000,
            }),
            pids_max: None,
        },
    )
    .unwrap();
    assert_eq!(plan.operations[0].target, CgroupLimitFile::Memory);
    assert_eq!(plan.operations[0].value, "1048576");
    assert_eq!(plan.operations[1].value, "max 100000");
    assert_eq!(
        authorize_cgroup_limit_plan(
            plan.clone(),
            CgroupLimitConfirmation {
                identity,
                allow_write: false,
            }
        ),
        Err(CgroupLimitPlanError::NotConfirmed)
    );
    assert_eq!(
        authorize_cgroup_limit_plan(
            plan,
            CgroupLimitConfirmation {
                identity: ProcessIdentity {
                    start_token: 901,
                    ..identity
                },
                allow_write: true,
            }
        ),
        Err(CgroupLimitPlanError::ConfirmationIdentityMismatch)
    );
}

#[test]
fn injected_cgroup_write_rolls_back_prior_values_on_partial_failure() {
    let identity = ProcessIdentity {
        pid: 42,
        start_token: 900,
    };
    let plan = plan_cgroup_limits(
        identity,
        &unified_membership(),
        CgroupLimitRequest {
            memory_max: Some(LimitValue::Value(200)),
            pids_max: Some(LimitValue::Value(20)),
            ..Default::default()
        },
    )
    .unwrap();
    let authorized = authorize_cgroup_limit_plan(
        plan,
        CgroupLimitConfirmation {
            identity,
            allow_write: true,
        },
    )
    .unwrap();
    let mut io = MemoryCgroupIo {
        start_time_ticks: 900,
        values: std::collections::HashMap::from([
            (CgroupLimitFile::Memory, "100".into()),
            (CgroupLimitFile::Pids, "10".into()),
        ]),
        fail_once_on: Some(CgroupLimitFile::Pids),
        writes: Vec::new(),
        ..Default::default()
    };
    let error = apply_cgroup_limit_plan_with(&authorized, &mut io).unwrap_err();
    assert_eq!(
        error,
        CgroupLimitApplyError::WriteFailed {
            target: CgroupLimitFile::Pids,
            failure: CgroupIoError::PermissionDenied,
            rollback_failed: false,
        }
    );
    assert_eq!(io.values[&CgroupLimitFile::Memory], "100");
    assert_eq!(io.values[&CgroupLimitFile::Pids], "10");
    assert_eq!(
        io.writes,
        vec![
            (CgroupLimitFile::Memory, "200".into()),
            (CgroupLimitFile::Pids, "10".into()),
            (CgroupLimitFile::Memory, "100".into()),
        ]
    );
}

#[test]
fn plan_and_authorize_cover_all_error_paths_and_round_trip() {
    let identity = ProcessIdentity {
        pid: 7,
        start_token: 42,
    };

    // A zero start token can never authorize a write.
    assert_eq!(
        plan_cgroup_limits(
            ProcessIdentity {
                pid: 7,
                start_token: 0,
            },
            &unified_membership(),
            CgroupLimitRequest {
                memory_max: Some(LimitValue::Value(10)),
                ..Default::default()
            },
        ),
        Err(CgroupLimitPlanError::MissingIdentity)
    );

    // A CPU quota needs a non-zero period.
    assert_eq!(
        plan_cgroup_limits(
            identity,
            &unified_membership(),
            CgroupLimitRequest {
                cpu_max: Some(CgroupCpuLimit {
                    quota_us: LimitValue::Value(1_000),
                    period_us: 0,
                }),
                ..Default::default()
            },
        ),
        Err(CgroupLimitPlanError::InvalidCpuPeriod)
    );

    // At least one limit must be selected.
    assert_eq!(
        plan_cgroup_limits(
            identity,
            &unified_membership(),
            CgroupLimitRequest::default()
        ),
        Err(CgroupLimitPlanError::EmptyRequest)
    );

    // A cgroup-v2 unified membership is required.
    assert_eq!(
        plan_cgroup_limits(
            identity,
            &[],
            CgroupLimitRequest {
                pids_max: Some(LimitValue::Value(5)),
                ..Default::default()
            },
        ),
        Err(CgroupLimitPlanError::MissingUnifiedCgroup)
    );

    // Success: authorize round-trips the plan and preserves formatting.
    let plan = plan_cgroup_limits(
        identity,
        &unified_membership(),
        CgroupLimitRequest {
            memory_max: Some(LimitValue::Unlimited),
            pids_max: Some(LimitValue::Value(8)),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(plan.identity, identity);
    assert_eq!(plan.group_path, "/user.slice/app.scope");
    let authorized = authorize_cgroup_limit_plan(
        plan,
        CgroupLimitConfirmation {
            identity,
            allow_write: true,
        },
    )
    .unwrap();
    let round_tripped = authorized.plan();
    assert_eq!(round_tripped.operations.len(), 2);
    assert_eq!(round_tripped.operations[0].target, CgroupLimitFile::Memory);
    assert_eq!(round_tripped.operations[0].value, "max");
    assert_eq!(round_tripped.operations[1].target, CgroupLimitFile::Pids);
    assert_eq!(round_tripped.operations[1].value, "8");
}

#[path = "resources/production_io.rs"]
mod production_io;
#[path = "resources/write_safety_tests.rs"]
mod write_safety_tests;
