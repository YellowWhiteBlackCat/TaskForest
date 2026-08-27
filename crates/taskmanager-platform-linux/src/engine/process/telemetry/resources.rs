//! `/proc/<pid>/limits`, cgroup membership and cgroup-v2 resource facts.

use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use taskmanager_core::ProcessIdentity;
use taskmanager_core::{
    LimitValue, ProviderId, ResourceGroupMembership, ResourceLimit, ResourceLimitKind,
};

#[cfg(target_os = "linux")]
use super::parse_start_time_ticks;
use super::safe_cgroup_path;

mod observation;

pub use observation::ProcessResourceTracker;

pub type CgroupMembership = ResourceGroupMembership;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CgroupLimitFile {
    #[serde(rename = "memory_max")]
    Memory,
    #[serde(rename = "cpu_max")]
    Cpu,
    #[serde(rename = "pids_max")]
    Pids,
}

#[cfg(target_os = "linux")]
impl CgroupLimitFile {
    fn file_name(self) -> &'static str {
        match self {
            Self::Memory => "memory.max",
            Self::Cpu => "cpu.max",
            Self::Pids => "pids.max",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CgroupCpuLimit {
    pub quota_us: LimitValue,
    pub period_us: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CgroupLimitRequest {
    pub memory_max: Option<LimitValue>,
    pub cpu_max: Option<CgroupCpuLimit>,
    pub pids_max: Option<LimitValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CgroupLimitOperation {
    pub target: CgroupLimitFile,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CgroupLimitPlan {
    pub identity: ProcessIdentity,
    pub group_path: String,
    pub operations: Vec<CgroupLimitOperation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CgroupLimitConfirmation {
    pub identity: ProcessIdentity,
    pub allow_write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedCgroupLimitPlan {
    plan: CgroupLimitPlan,
}

impl AuthorizedCgroupLimitPlan {
    pub fn plan(&self) -> &CgroupLimitPlan {
        &self.plan
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CgroupLimitPlanError {
    MissingIdentity,
    MissingUnifiedCgroup,
    InvalidCpuPeriod,
    EmptyRequest,
    NotConfirmed,
    ConfirmationIdentityMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CgroupIoError {
    NotFound,
    PermissionDenied,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CgroupLimitApplyError {
    Unsupported,
    IdentityReadFailed(CgroupIoError),
    MembershipReadFailed(CgroupIoError),
    IdentityChanged {
        expected: ProcessIdentity,
        observed_start_time_ticks: u64,
    },
    TargetChanged {
        expected_group_path: String,
        observed_group_path: Option<String>,
    },
    ReadFailed {
        target: CgroupLimitFile,
        failure: CgroupIoError,
    },
    WriteFailed {
        target: CgroupLimitFile,
        failure: CgroupIoError,
        rollback_failed: bool,
    },
}

/// Narrow executor seam for a cgroup-v2 transaction. Production uses procfs +
/// cgroupfs; tests inject an in-memory implementation and never touch the host's
/// real cgroup hierarchy.
pub trait CgroupPlanIo {
    fn read_start_time_ticks(&mut self, pid: u32) -> Result<u64, CgroupIoError>;
    fn read_unified_group(&mut self, pid: u32) -> Result<Option<String>, CgroupIoError>;
    fn read_value(
        &mut self,
        group_path: &str,
        target: CgroupLimitFile,
    ) -> Result<String, CgroupIoError>;
    fn write_value(
        &mut self,
        group_path: &str,
        target: CgroupLimitFile,
        value: &str,
    ) -> Result<(), CgroupIoError>;
}

pub fn plan_cgroup_limits(
    identity: ProcessIdentity,
    memberships: &[CgroupMembership],
    request: CgroupLimitRequest,
) -> Result<CgroupLimitPlan, CgroupLimitPlanError> {
    if identity.start_token == 0 {
        return Err(CgroupLimitPlanError::MissingIdentity);
    }
    let group_path = memberships
        .iter()
        .find(|membership| {
            membership.provider.as_str() == "linux.cgroup"
                && membership.native_hierarchy_id == Some(0)
                && membership.capabilities.is_empty()
        })
        .map(|membership| membership.native_locator.clone())
        .filter(|path| safe_cgroup_path(Path::new("/"), path).is_some())
        .ok_or(CgroupLimitPlanError::MissingUnifiedCgroup)?;
    let mut operations = Vec::new();
    if let Some(value) = request.memory_max {
        operations.push(CgroupLimitOperation {
            target: CgroupLimitFile::Memory,
            value: format_limit(value),
        });
    }
    if let Some(value) = request.cpu_max {
        if value.period_us == 0 {
            return Err(CgroupLimitPlanError::InvalidCpuPeriod);
        }
        operations.push(CgroupLimitOperation {
            target: CgroupLimitFile::Cpu,
            value: format!("{} {}", format_limit(value.quota_us), value.period_us),
        });
    }
    if let Some(value) = request.pids_max {
        operations.push(CgroupLimitOperation {
            target: CgroupLimitFile::Pids,
            value: format_limit(value),
        });
    }
    if operations.is_empty() {
        return Err(CgroupLimitPlanError::EmptyRequest);
    }
    Ok(CgroupLimitPlan {
        identity,
        group_path,
        operations,
    })
}

pub fn authorize_cgroup_limit_plan(
    plan: CgroupLimitPlan,
    confirmation: CgroupLimitConfirmation,
) -> Result<AuthorizedCgroupLimitPlan, CgroupLimitPlanError> {
    if !confirmation.allow_write {
        return Err(CgroupLimitPlanError::NotConfirmed);
    }
    if confirmation.identity != plan.identity {
        return Err(CgroupLimitPlanError::ConfirmationIdentityMismatch);
    }
    Ok(AuthorizedCgroupLimitPlan { plan })
}

pub fn apply_cgroup_limit_plan_with(
    authorized: &AuthorizedCgroupLimitPlan,
    io: &mut impl CgroupPlanIo,
) -> Result<(), CgroupLimitApplyError> {
    let plan = authorized.plan();
    validate_cgroup_plan_target(plan, io)?;
    let mut previous_values = Vec::with_capacity(plan.operations.len());
    for operation in &plan.operations {
        let value = io
            .read_value(&plan.group_path, operation.target)
            .map_err(|failure| CgroupLimitApplyError::ReadFailed {
                target: operation.target,
                failure,
            })?;
        previous_values.push((operation.target, value));
    }
    // A read may block while the PID is recycled or the process migrates to a
    // different cgroup. Revalidate both frozen identities after every pre-read
    // and before the transaction's first write.
    validate_cgroup_plan_target(plan, io)?;
    for (index, operation) in plan.operations.iter().enumerate() {
        if let Err(failure) = io.write_value(&plan.group_path, operation.target, &operation.value) {
            let mut rollback_failed = false;
            // Restore the failed target too: a filesystem write can report an
            // error after truncating or partially updating the pseudo-file.
            for (target, value) in previous_values[..=index].iter().rev() {
                rollback_failed |= io.write_value(&plan.group_path, *target, value).is_err();
            }
            return Err(CgroupLimitApplyError::WriteFailed {
                target: operation.target,
                failure,
                rollback_failed,
            });
        }
    }
    Ok(())
}

fn validate_cgroup_plan_target(
    plan: &CgroupLimitPlan,
    io: &mut impl CgroupPlanIo,
) -> Result<(), CgroupLimitApplyError> {
    validate_start_token(plan, io)?;
    let observed_group_path = io
        .read_unified_group(plan.identity.pid)
        .map_err(CgroupLimitApplyError::MembershipReadFailed)?;
    // Clamp the membership read between two token reads. A recycled PID whose
    // replacement happens to join the same group must not pass validation.
    validate_start_token(plan, io)?;
    if observed_group_path.as_deref() != Some(plan.group_path.as_str()) {
        return Err(CgroupLimitApplyError::TargetChanged {
            expected_group_path: plan.group_path.clone(),
            observed_group_path,
        });
    }
    Ok(())
}

fn validate_start_token(
    plan: &CgroupLimitPlan,
    io: &mut impl CgroupPlanIo,
) -> Result<(), CgroupLimitApplyError> {
    let observed_start_time_ticks = io
        .read_start_time_ticks(plan.identity.pid)
        .map_err(CgroupLimitApplyError::IdentityReadFailed)?;
    if observed_start_time_ticks == plan.identity.start_token {
        Ok(())
    } else {
        Err(CgroupLimitApplyError::IdentityChanged {
            expected: plan.identity,
            observed_start_time_ticks,
        })
    }
}

#[cfg(target_os = "linux")]
pub fn apply_cgroup_limit_plan(
    authorized: &AuthorizedCgroupLimitPlan,
) -> Result<(), CgroupLimitApplyError> {
    let mut io = LinuxCgroupPlanIo {
        proc_root: PathBuf::from("/proc"),
        cgroup_root: PathBuf::from("/sys/fs/cgroup"),
    };
    apply_cgroup_limit_plan_with(authorized, &mut io)
}

#[cfg(not(target_os = "linux"))]
pub fn apply_cgroup_limit_plan(
    _authorized: &AuthorizedCgroupLimitPlan,
) -> Result<(), CgroupLimitApplyError> {
    Err(CgroupLimitApplyError::Unsupported)
}

#[cfg(target_os = "linux")]
struct LinuxCgroupPlanIo {
    proc_root: PathBuf,
    cgroup_root: PathBuf,
}

#[cfg(target_os = "linux")]
impl CgroupPlanIo for LinuxCgroupPlanIo {
    fn read_start_time_ticks(&mut self, pid: u32) -> Result<u64, CgroupIoError> {
        let text = std::fs::read_to_string(self.proc_root.join(pid.to_string()).join("stat"))
            .map_err(cgroup_io_error)?;
        parse_start_time_ticks(&text).ok_or(CgroupIoError::Unavailable)
    }

    fn read_unified_group(&mut self, pid: u32) -> Result<Option<String>, CgroupIoError> {
        let text = std::fs::read_to_string(self.proc_root.join(pid.to_string()).join("cgroup"))
            .map_err(cgroup_io_error)?;
        Ok(parse_proc_cgroup(&text)
            .into_iter()
            .find(|membership| {
                membership.native_hierarchy_id == Some(0) && membership.capabilities.is_empty()
            })
            .map(|membership| membership.native_locator))
    }

    fn read_value(
        &mut self,
        group_path: &str,
        target: CgroupLimitFile,
    ) -> Result<String, CgroupIoError> {
        let path = cgroup_target_path(&self.cgroup_root, group_path, target)?;
        std::fs::read_to_string(path)
            .map(|value| value.trim().to_owned())
            .map_err(cgroup_io_error)
    }

    fn write_value(
        &mut self,
        group_path: &str,
        target: CgroupLimitFile,
        value: &str,
    ) -> Result<(), CgroupIoError> {
        let path = cgroup_target_path(&self.cgroup_root, group_path, target)?;
        std::fs::write(path, value).map_err(cgroup_io_error)
    }
}

#[cfg(target_os = "linux")]
fn cgroup_target_path(
    root: &Path,
    group_path: &str,
    target: CgroupLimitFile,
) -> Result<PathBuf, CgroupIoError> {
    safe_cgroup_path(root, group_path)
        .map(|path| path.join(target.file_name()))
        .ok_or(CgroupIoError::Unavailable)
}

#[cfg(target_os = "linux")]
fn cgroup_io_error(error: std::io::Error) -> CgroupIoError {
    match error.kind() {
        std::io::ErrorKind::NotFound => CgroupIoError::NotFound,
        std::io::ErrorKind::PermissionDenied => CgroupIoError::PermissionDenied,
        _ => CgroupIoError::Unavailable,
    }
}

fn format_limit(value: LimitValue) -> String {
    match value {
        LimitValue::Unlimited => "max".into(),
        LimitValue::Value(value) => value.to_string(),
    }
}

pub fn parse_proc_limits(text: &str) -> Vec<ResourceLimit> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let name = line.get(..25)?.trim();
            let soft = parse_limit_value(line.get(25..45)?.trim())?;
            let hard = parse_limit_value(line.get(45..65)?.trim())?;
            let unit = line
                .get(65..)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Some(ResourceLimit {
                kind: parse_limit_kind(name),
                soft,
                hard,
                unit,
            })
        })
        .collect()
}

pub fn parse_proc_cgroup(text: &str) -> Vec<CgroupMembership> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, ':');
            let hierarchy_id = parts.next()?.parse().ok()?;
            let controllers = parts
                .next()?
                .split(',')
                .filter(|controller| !controller.is_empty())
                .map(ToOwned::to_owned)
                .collect();
            let path = parts.next()?.to_owned();
            Some(CgroupMembership {
                provider: ProviderId::borrowed("linux.cgroup"),
                native_hierarchy_id: Some(hierarchy_id),
                capabilities: controllers,
                native_locator: path,
            })
        })
        .collect()
}

fn parse_limit_kind(name: &str) -> ResourceLimitKind {
    match name {
        "Max cpu time" => ResourceLimitKind::CpuTime,
        "Max file size" => ResourceLimitKind::FileSize,
        "Max data size" => ResourceLimitKind::DataSize,
        "Max stack size" => ResourceLimitKind::StackSize,
        "Max core file size" => ResourceLimitKind::CoreFileSize,
        "Max resident set" => ResourceLimitKind::ResidentSet,
        "Max processes" => ResourceLimitKind::Processes,
        "Max open files" => ResourceLimitKind::OpenFiles,
        "Max locked memory" => ResourceLimitKind::LockedMemory,
        "Max address space" => ResourceLimitKind::AddressSpace,
        "Max file locks" => ResourceLimitKind::FileLocks,
        "Max pending signals" => ResourceLimitKind::PendingSignals,
        "Max msgqueue size" => ResourceLimitKind::MessageQueue,
        "Max nice priority" => ResourceLimitKind::NicePriority,
        "Max realtime priority" => ResourceLimitKind::RealtimePriority,
        "Max realtime timeout" => ResourceLimitKind::RealtimeTimeout,
        other => ResourceLimitKind::Other(other.to_owned()),
    }
}

fn parse_limit_value(value: &str) -> Option<LimitValue> {
    if value == "max" || value == "unlimited" {
        Some(LimitValue::Unlimited)
    } else {
        value.parse().ok().map(LimitValue::Value)
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/engine/process/telemetry/resources.rs"]
mod tests;
