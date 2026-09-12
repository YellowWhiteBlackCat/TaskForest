//! Typed data models for the diagnostic bundle.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use taskmanager_core::core::FailureKind;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::source::SourceOutcome;
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilitySnapshot, CapabilityStatus, SubmissionErrorKind,
};

use crate::platform::{
    ProjectedSystemTelemetry, SystemTelemetryDomainState, SystemTelemetryUnavailable,
};

#[allow(dead_code)]
pub const DIAGNOSTIC_BUNDLE_SCHEMA_VERSION: u32 = 1;

/// Structured system overview projection facts for the diagnostic bundle.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct DiagnosticSystemOverview {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_brand: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sockets: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_freq_mhz: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_usage_pct: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_total_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_used_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_available_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swap_total_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swap_used_bytes: Option<u64>,
    pub uptime_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_average: Option<DiagnosticLoadAverage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure: Option<DiagnosticPressure>,
    pub disks_count: usize,
    pub networks_count: usize,
    pub gpu_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub virt: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticLoadAverage {
    pub one_minute: f32,
    pub five_minutes: f32,
    pub fifteen_minutes: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct DiagnosticPressure {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_some_10s: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_some_10s: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io_some_10s: Option<f32>,
}

impl DiagnosticSystemOverview {
    #[must_use]
    pub fn from_facts(hardware: Option<&HardwareInfo>, snapshot: Option<&SystemSnapshot>) -> Self {
        let os_name = hardware.and_then(|h| h.os_name.clone());
        let os_version = hardware.and_then(|h| h.os_version.clone());
        let kernel_version = hardware.and_then(|h| h.kernel_version.clone());
        let hostname = hardware.and_then(|h| h.hostname.clone());
        let architecture = hardware.and_then(|h| h.architecture.clone());
        let cpu_brand = hardware.and_then(|h| h.cpu_brand.clone());
        let cpu_cores = hardware.and_then(|h| h.cpu_cores);
        let sockets = hardware.and_then(|h| h.sockets);
        let base_freq_mhz = hardware.and_then(|h| h.base_freq_mhz);
        let virt = hardware.and_then(|h| h.virt.clone());

        let uptime_secs = snapshot.map_or(0, |s| s.uptime_secs);
        let cpu_usage_pct = snapshot.and_then(|s| s.cpu.current_global_usage_pct());
        let memory_total_bytes = snapshot
            .and_then(|s| s.memory.current_total_bytes())
            .or_else(|| hardware.and_then(|h| h.total_memory_mb.map(|mb| mb * 1024 * 1024)));
        let memory_used_bytes = snapshot.and_then(|s| s.memory.current_used_bytes());
        let memory_available_bytes = snapshot.and_then(|s| s.memory.current_available_bytes());
        let swap_total_bytes = snapshot.and_then(|s| s.memory.current_swap_total_bytes());
        let swap_used_bytes = snapshot.and_then(|s| s.memory.current_swap_used_bytes());

        let load_average =
            snapshot
                .and_then(|s| s.load_average.as_ref())
                .map(|l| DiagnosticLoadAverage {
                    one_minute: l.one_minute,
                    five_minutes: l.five_minutes,
                    fifteen_minutes: l.fifteen_minutes,
                });

        let pressure = snapshot
            .and_then(|s| s.pressure.as_ref())
            .map(|p| DiagnosticPressure {
                cpu_some_10s: p.cpu.current_value().map(|r| r.some.avg10),
                memory_some_10s: p.memory.current_value().map(|r| r.some.avg10),
                io_some_10s: p.io.current_value().map(|r| r.some.avg10),
            });

        let disks_count = snapshot.map_or(0, |s| s.disks.len());
        let networks_count = snapshot.map_or(0, |s| s.networks.len());
        let gpu_count = snapshot.map_or(0, |s| s.gpu.len());

        Self {
            os_name,
            os_version,
            kernel_version,
            hostname,
            architecture,
            cpu_brand,
            cpu_cores,
            sockets,
            base_freq_mhz,
            cpu_usage_pct,
            memory_total_bytes,
            memory_used_bytes,
            memory_available_bytes,
            swap_total_bytes,
            swap_used_bytes,
            uptime_secs,
            load_average,
            pressure,
            disks_count,
            networks_count,
            gpu_count,
            virt,
        }
    }
}

/// Structured platform capabilities facts for the diagnostic bundle.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct DiagnosticPlatformCapabilities {
    pub capabilities: Vec<DiagnosticCapabilityEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticCapabilityEntry {
    pub id: String,
    pub status: String,
    pub providers: Vec<String>,
    pub observed_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success_at_ms: Option<u64>,
}

impl DiagnosticPlatformCapabilities {
    #[must_use]
    pub fn from_catalog(catalog: &dyn CapabilityCatalog) -> Self {
        Self::from_snapshot(&catalog.snapshot())
    }

    #[must_use]
    pub fn from_snapshot(snapshot: &CapabilitySnapshot) -> Self {
        let capabilities = snapshot
            .iter()
            .map(|desc| DiagnosticCapabilityEntry {
                id: desc.id.as_str().to_owned(),
                status: match desc.status {
                    CapabilityStatus::Available => "available".to_owned(),
                    CapabilityStatus::Degraded(kind) => {
                        format!("degraded:{}", format_failure_kind(kind))
                    }
                    CapabilityStatus::Unsupported => "unsupported".to_owned(),
                    CapabilityStatus::PermissionRequired => "permission_required".to_owned(),
                    CapabilityStatus::MissingDependency => "missing_dependency".to_owned(),
                    CapabilityStatus::TemporarilyUnavailable => {
                        "temporarily_unavailable".to_owned()
                    }
                    CapabilityStatus::Stale => "stale".to_owned(),
                },
                providers: desc.providers.iter().map(|p| p.to_string()).collect(),
                observed_at_ms: desc.observed_at_ms,
                last_success_at_ms: desc.last_success_at_ms,
            })
            .collect();
        Self { capabilities }
    }
}

/// Structured process inventory and aggregate summary facts for the diagnostic bundle.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct DiagnosticProcessSummary {
    pub total_processes: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_threads: Option<usize>,
    pub status_counts: BTreeMap<String, usize>,
    pub distinct_users: usize,
    pub top_cpu: Vec<DiagnosticProcessTopEntry>,
    pub top_memory: Vec<DiagnosticProcessTopEntry>,
    pub anomalies_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticProcessTopEntry {
    pub pid: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_pct: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
}

impl DiagnosticProcessSummary {
    #[must_use]
    pub fn from_processes(processes: &[ProcessItem], threads: Option<usize>) -> Self {
        let total_processes = processes.len();
        let total_threads = threads;
        let mut status_counts = BTreeMap::new();
        let mut users = HashSet::new();

        for proc in processes {
            *status_counts.entry(proc.status.clone()).or_insert(0) += 1;
            if let Some(user) = proc.current_user()
                && !user.trim().is_empty()
            {
                users.insert(user);
            }
        }
        let distinct_users = users.len();

        let mut cpu_sorted: Vec<&ProcessItem> = processes.iter().collect();
        cpu_sorted.sort_by(|a, b| {
            let a_val = a.current_cpu_percentage().unwrap_or(0.0);
            let b_val = b.current_cpu_percentage().unwrap_or(0.0);
            b_val
                .partial_cmp(&a_val)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let top_cpu = cpu_sorted
            .into_iter()
            .take(5)
            .map(|proc| DiagnosticProcessTopEntry {
                pid: proc.pid,
                name: proc.name.clone(),
                user: proc.current_user().filter(|u| !u.trim().is_empty()),
                cpu_pct: proc.current_cpu_percentage(),
                memory_bytes: proc.current_memory_bytes(),
            })
            .collect();

        let mut mem_sorted: Vec<&ProcessItem> = processes.iter().collect();
        mem_sorted.sort_by(|a, b| {
            let a_val = a.current_memory_bytes().unwrap_or(0);
            let b_val = b.current_memory_bytes().unwrap_or(0);
            b_val.cmp(&a_val)
        });
        let top_memory = mem_sorted
            .into_iter()
            .take(5)
            .map(|proc| DiagnosticProcessTopEntry {
                pid: proc.pid,
                name: proc.name.clone(),
                user: proc.current_user().filter(|u| !u.trim().is_empty()),
                cpu_pct: proc.current_cpu_percentage(),
                memory_bytes: proc.current_memory_bytes(),
            })
            .collect();

        let anomalies_count = taskmanager_core::detect_process_anomalies(processes).len();

        Self {
            total_processes,
            total_threads,
            status_counts,
            distinct_users,
            top_cpu,
            top_memory,
            anomalies_count,
        }
    }
}

/// Structured telemetry domain, source and provider health facts for the diagnostic bundle.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct DiagnosticTelemetryHealth {
    pub domains: Vec<DiagnosticDomainHealth>,
    pub sources: Vec<DiagnosticSourceHealth>,
    pub providers: Vec<DiagnosticProviderHealth>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticDomainHealth {
    pub domain: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticSourceHealth {
    pub provider: String,
    pub outcome: String,
    pub item_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticProviderHealth {
    pub provider: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success_ms: Option<u64>,
}

impl DiagnosticTelemetryHealth {
    #[must_use]
    pub fn from_telemetry(
        projection: Option<&ProjectedSystemTelemetry>,
        snapshot: Option<&SystemSnapshot>,
    ) -> Self {
        let mut domains = Vec::new();
        if let Some(proj) = projection {
            domains.push(map_domain_state("host", &proj.host));
            domains.push(map_domain_state("cpu", &proj.cpu));
            domains.push(map_domain_state("memory", &proj.memory));
            domains.push(map_domain_state("storage", &proj.storage));
            domains.push(map_domain_state("network", &proj.network));
            domains.push(map_domain_state("gpu", &proj.gpu));
        } else if snapshot.is_some() {
            for name in ["host", "cpu", "memory", "storage", "network", "gpu"] {
                domains.push(DiagnosticDomainHealth {
                    domain: name.to_owned(),
                    state: "current".to_owned(),
                    failure_reason: None,
                });
            }
        }

        let sources = snapshot.map_or_else(Vec::new, |snap| {
            snap.telemetry_sources
                .iter()
                .map(|s| DiagnosticSourceHealth {
                    provider: s.provider.to_string(),
                    outcome: match s.outcome {
                        SourceOutcome::Available => "available".to_owned(),
                        SourceOutcome::Empty => "empty".to_owned(),
                        SourceOutcome::Partial(kind) => {
                            format!("partial:{}", format_failure_kind(kind))
                        }
                        SourceOutcome::Unavailable(kind) => {
                            format!("unavailable:{}", format_failure_kind(kind))
                        }
                    },
                    item_count: s.item_count,
                })
                .collect()
        });

        let providers = snapshot.map_or_else(Vec::new, |snap| {
            snap.provider_states
                .iter()
                .map(|p| DiagnosticProviderHealth {
                    provider: p.provider.to_string(),
                    status: format!("{:?}", p.status).to_lowercase(),
                    last_success_ms: p.last_success_ms,
                })
                .collect()
        });

        Self {
            domains,
            sources,
            providers,
        }
    }
}

fn map_domain_state<T>(
    name: &str,
    state: &SystemTelemetryDomainState<T>,
) -> DiagnosticDomainHealth {
    let (state_str, reason) = match state {
        SystemTelemetryDomainState::Pending => ("pending", None),
        SystemTelemetryDomainState::Current(_) => ("current", None),
        SystemTelemetryDomainState::Partial(_) => ("partial", None),
        SystemTelemetryDomainState::Stale(_) => ("stale", None),
        SystemTelemetryDomainState::Unavailable { reason, .. } => match reason {
            SystemTelemetryUnavailable::Provider(failure) => (
                "unavailable",
                Some(format!("provider:{}", format_failure_kind(*failure))),
            ),
            SystemTelemetryUnavailable::Submission(sub) => (
                "unavailable",
                Some(format!("submission:{}", format_submission_error_kind(*sub))),
            ),
        },
    };
    DiagnosticDomainHealth {
        domain: name.to_owned(),
        state: state_str.to_owned(),
        failure_reason: reason,
    }
}

fn format_failure_kind(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::Unsupported => "unsupported",
        FailureKind::PermissionDenied => "permission_denied",
        FailureKind::MissingDependency => "missing_dependency",
        FailureKind::TimedOut => "timed_out",
        FailureKind::IdentityChanged => "identity_changed",
        FailureKind::TemporarilyUnavailable => "temporarily_unavailable",
        FailureKind::Rejected => "rejected",
        FailureKind::ProviderFault => "provider_fault",
        FailureKind::RequiresEscalation => "requires_escalation",
    }
}

fn format_submission_error_kind(kind: SubmissionErrorKind) -> &'static str {
    match kind {
        SubmissionErrorKind::Busy => "busy",
        SubmissionErrorKind::RuntimeStopped => "runtime_stopped",
        SubmissionErrorKind::InvalidRequest => "invalid_request",
        SubmissionErrorKind::UnsupportedCapability => "unsupported_capability",
    }
}

/// Metadata manifest for one diagnostic bundle.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticBundleManifest {
    pub version: u32,
    pub timestamp_ms: u64,
    pub sources: Vec<String>,
    pub total_processes: usize,
    pub capabilities_count: usize,
}
