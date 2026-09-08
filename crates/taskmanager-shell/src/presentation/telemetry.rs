//! Shared presentation folds for pressure, CPU topology, service diagnostics,
//! process isolation, and system health.

use taskmanager_application::i18n;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::metrics::{
    CpuMetrics, PressureWindow, ResourcePressure, SystemLoadAverage, SystemSnapshot,
};
use taskmanager_core::core::process::detect_process_anomalies;
use taskmanager_core::core::process_telemetry::NetworkConnectionCounters;
use taskmanager_core::core::process_telemetry::{
    LinuxNamespaceAudit, NamespaceAuditStatus, ProcessCapabilities, ProcessIsolation,
};
use taskmanager_core::core::services::{ServiceDiagnostics, ServiceFailureCause};
use taskmanager_core::{HealthScoreInput, SystemHealthScore};

use super::{MISSING_VALUE, megahertz, service_exit};

/// Compact shared process-anomaly banner. It is intentionally a triage hint:
/// the typed core heuristic owns the evidence and the renderer only localizes
/// the kind and identifies a process when the rule can do so.
#[must_use]
pub fn process_anomaly_summary(
    processes: Option<&[taskmanager_core::ProcessItem]>,
) -> Option<String> {
    let anomalies = detect_process_anomalies(processes?);
    if anomalies.is_empty() {
        return None;
    }
    let labels = anomalies
        .iter()
        .take(3)
        .map(|anomaly| {
            let label = i18n::t(anomaly.kind.i18n_key());
            anomaly
                .pid
                .map_or_else(|| label.to_owned(), |pid| format!("{label} PID{pid}"))
        })
        .collect::<Vec<_>>()
        .join(" · ");
    let suffix = if anomalies.len() > 3 {
        format!(" · +{}", anomalies.len() - 3)
    } else {
        String::new()
    };
    Some(format!(
        "{}: {labels}{suffix}",
        i18n::t("proc.anomaly_summary")
    ))
}

/// Format host TCP open/retransmit counters that accompany a process socket
/// snapshot. The explicit `host` wording prevents aggregate kernel counters
/// from being misread as per-PID accounting.
#[must_use]
pub fn network_connection_counters_summary(counters: &NetworkConnectionCounters) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(value) = counters.active_opens {
        parts.push(format!("{} {value}", i18n::t("net.active_opens")));
    }
    if let Some(value) = counters.passive_opens {
        parts.push(format!("{} {value}", i18n::t("net.passive_opens")));
    }
    if let Some(value) = counters.retransmitted_segments {
        parts.push(format!("{} {value}", i18n::t("net.retransmits")));
    }
    (!parts.is_empty()).then(|| format!("{}: {}", i18n::t("net.host_tcp"), parts.join(" · ")))
}

/// Summarize the bounded native kernel-error capture. `None` remains an
/// unavailable source, while `Some(empty)` is an observed clean window.
#[must_use]
pub fn kernel_error_summary(hardware: &HardwareInfo) -> Option<String> {
    let errors = hardware.kernel_errors.as_ref()?;
    if errors.is_empty() {
        return Some(i18n::t("system.kernel_errors_none").to_owned());
    }
    let preview = errors
        .iter()
        .take(2)
        .map(|error| format!("[{}] {}", error.priority.number(), error.message))
        .collect::<Vec<_>>()
        .join(" · ");
    let suffix = if errors.len() > 2 {
        format!(" · +{}", errors.len() - 2)
    } else {
        String::new()
    };
    Some(format!(
        "{}: {preview}{suffix}",
        i18n::t("system.kernel_errors")
    ))
}

pub fn missing_value() -> String {
    MISSING_VALUE.to_owned()
}

/// Format one PSI moving-average window using the three kernel time horizons.
///
/// The values are already percentages from the typed provider observation;
/// this helper only joins them so every frontend presents the same 10s/60s/5m
/// evidence and never silently drops the longer-term signal.
#[must_use]
pub fn pressure_window_summary(window: &PressureWindow) -> String {
    format!(
        "10s {:.1}% · 60s {:.1}% · 5m {:.1}%",
        window.avg10, window.avg60, window.avg300
    )
}

/// Dense-rail PSI summary. It retains all three horizons while removing the
/// leading zero from sub-percent values, keeping the complete value inside a
/// narrow label/value row instead of right-truncating the first horizon.
#[must_use]
pub fn pressure_window_compact_summary(window: &PressureWindow) -> String {
    let compact = |value: f32| {
        let value = format!("{value:.1}%");
        value
            .strip_prefix("0.")
            .map_or(value.clone(), |tail| format!(".{tail}"))
    };
    format!(
        "10s {} · 60s {} · 5m {}",
        compact(window.avg10),
        compact(window.avg60),
        compact(window.avg300),
    )
}

/// Compact pressure values for a fixed label/value rail. The horizon order is
/// the same as [`pressure_window_summary`] (10s, 60s, 5m), but the labels are
/// omitted because the surrounding row already identifies the metric and this
/// keeps the beginning of the value from being clipped by right alignment.
#[must_use]
pub fn pressure_window_values_compact_summary(window: &PressureWindow) -> String {
    let compact = |value: f32| {
        let value = format!("{value:.1}%");
        value
            .strip_prefix("0.")
            .map_or(value.clone(), |tail| format!(".{tail}"))
    };
    format!(
        "{} · {} · {}",
        compact(window.avg10),
        compact(window.avg60),
        compact(window.avg300),
    )
}

/// Format PSI `some` and optional `full` windows without collapsing a missing
/// full observation into a successful zero. `full` is the severe all-tasks
/// stalled signal and is included only when the provider actually exposes it.
#[must_use]
pub fn pressure_summary(pressure: &ResourcePressure) -> String {
    let some = format!("some {}", pressure_window_summary(&pressure.some));
    pressure.full.as_ref().map_or(some.clone(), |full| {
        format!("{some} · full {}", pressure_window_summary(full))
    })
}

/// Format the load triple using the logical-processor-normalized values. The
/// raw values remain available in the typed fact and export; the compact UI
/// value is the portable quantity that can be compared across host sizes.
#[must_use]
pub fn load_average_summary(load: &SystemLoadAverage) -> String {
    format!(
        "{} {}",
        i18n::t("system.load_normalized"),
        load_average_values_summary(load),
    )
}

/// The normalized load values without the repeated row label. This is used
/// when a label/value component already owns the `Load` label.
#[must_use]
pub fn load_average_values_summary(load: &SystemLoadAverage) -> String {
    format!(
        "1m {:.2}× · 5m {:.2}× · 15m {:.2}×",
        load.normalized_one_minute, load.normalized_five_minutes, load.normalized_fifteen_minutes,
    )
}

/// Label-free load values for a fixed-width key/value row. The horizon order
/// remains 1m, 5m, 15m; omitting repeated labels keeps the row label visible
/// and prevents a right-aligned value from losing its first horizon.
#[must_use]
pub fn load_average_values_compact_summary(load: &SystemLoadAverage) -> String {
    format!(
        "{:.2}× · {:.2}× · {:.2}×",
        load.normalized_one_minute, load.normalized_five_minutes, load.normalized_fifteen_minutes,
    )
}

/// Explain the denominator behind a normalized load value. The physical-core
/// count is preferred when the native host provider proved it; otherwise the
/// wording names the logical fallback explicitly.
#[must_use]
pub fn load_average_basis_summary(load: &SystemLoadAverage) -> String {
    match load.physical_cores {
        Some(cores) => format!("{} {cores}", i18n::t("system.load_basis_physical")),
        None => format!(
            "{} {}",
            i18n::t("system.load_basis_logical"),
            load.logical_processors
        ),
    }
}

/// Compact, renderer-neutral summary of the package topology collected by the
/// native adapter. It intentionally uses only proven fields: a missing NUMA,
/// chiplet, or SMT fact is omitted rather than guessed.
#[must_use]
pub fn cpu_topology_summary(cpu: &CpuMetrics) -> Option<String> {
    if cpu.packages.is_empty() {
        return None;
    }
    let packages = cpu
        .packages
        .iter()
        .map(|package| {
            let cores = package
                .physical_core_count
                .map_or_else(|| "—".to_owned(), |value| value.to_string());
            let threads = package.logical_core_ids.len();
            let mut parts = vec![format!("S{} {cores}C/{threads}T", package.package_id)];
            if let Some(node) = package.numa_node_id {
                parts.push(format!("N{node}"));
            }
            if let Some(smt) = package.smt_threads_per_core {
                parts.push(format!("{smt}T/core"));
            }
            if !package.smt_sibling_groups.is_empty() {
                let pairing = package
                    .smt_sibling_groups
                    .iter()
                    .take(4)
                    .map(|group| {
                        group
                            .iter()
                            .map(usize::to_string)
                            .collect::<Vec<_>>()
                            .join("/")
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let suffix = if package.smt_sibling_groups.len() > 4 {
                    format!(" +{}", package.smt_sibling_groups.len() - 4)
                } else {
                    String::new()
                };
                parts.push(format!("SMT {pairing}{suffix}"));
            }
            if !package.chiplet_ids.is_empty() {
                let ids = package
                    .chiplet_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join("/");
                let label = cpu
                    .brand
                    .as_deref()
                    .is_some_and(|brand| brand.to_ascii_lowercase().contains("amd"));
                parts.push(format!("{}{}", if label { "CCD" } else { "Die" }, ids));
            }
            parts.join(" ")
        })
        .collect::<Vec<_>>();
    Some(packages.join(" | "))
}

/// Compact representation of cpuidle evidence. The source exposes cumulative
/// counters, so the summary shows the counter units explicitly and never
/// presents them as a fabricated percentage from a single sample.
#[must_use]
pub fn cpu_idle_state_summary(cpu: &CpuMetrics) -> Option<String> {
    let states = cpu
        .idle_states
        .iter()
        .filter(|state| state.residency_us.is_some() || state.usage_count.is_some())
        .take(2)
        .map(|state| {
            let compact_counter = |value: u64| {
                if value >= 1_000_000 {
                    format!("{:.1}M", value as f64 / 1_000_000.0)
                } else if value >= 1_000 {
                    format!("{:.1}k", value as f64 / 1_000.0)
                } else {
                    value.to_string()
                }
            };
            let value = state.residency_pct.map_or_else(
                || {
                    state.usage_count.map_or_else(
                        || {
                            state.residency_us.map_or_else(
                                || "—".to_owned(),
                                |value| format!("{}s", value / 1_000_000),
                            )
                        },
                        compact_counter,
                    )
                },
                |value| format!("{value:.1}%"),
            );
            format!("{} {value}", state.name)
        })
        .collect::<Vec<_>>();
    (!states.is_empty()).then(|| states.join(" · "))
}

/// Compact package power-limit summary for dense CPU details rails. PL1/PL2
/// and Tau are independent kernel facts; the row is absent when no limit was
/// observed and never substitutes a nominal platform TDP.
#[must_use]
pub fn cpu_power_limits_summary(cpu: &CpuMetrics) -> Option<String> {
    let policy = &cpu.performance_policy;
    let mut parts = Vec::new();
    if let Some(value) = policy.power_limit_1_w.filter(|value| value.is_finite()) {
        parts.push(format!("PL1 {value:.0}W"));
    }
    if let Some(value) = policy.power_limit_2_w.filter(|value| value.is_finite()) {
        parts.push(format!("PL2 {value:.0}W"));
    }
    if let Some(value) = policy.power_time_window_ms {
        if value >= 1_000 {
            parts.push(format!("Tau {}s", value / 1_000));
        } else {
            parts.push(format!("Tau {value}ms"));
        }
    }
    if let Some(enabled) = policy.boost_enabled {
        parts.push(format!(
            "{} {}",
            i18n::t("cpu.turbo"),
            i18n::t(if enabled {
                "common.enabled"
            } else {
                "common.disabled"
            })
        ));
    }
    if let Some(value) = policy.boost_max_frequency_mhz.filter(|value| *value > 0) {
        parts.push(format!(
            "{} {}",
            i18n::t("cpu.turbo_max"),
            megahertz(value as f32)
        ));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Compact interrupt-distribution evidence: the cumulative total plus the
/// busiest logical CPU. The per-CPU vector remains available to rich views;
/// this summary avoids turning a dense rail into an unreadable list.
#[must_use]
pub fn cpu_interrupt_summary(cpu: &CpuMetrics) -> Option<String> {
    let interrupts = cpu.interrupts.as_ref()?;
    let busiest = interrupts
        .per_logical_cpu
        .iter()
        .copied()
        .enumerate()
        .max_by_key(|(_, value)| *value);
    let total = interrupts.total.or_else(|| {
        Some(
            interrupts
                .per_logical_cpu
                .iter()
                .copied()
                .fold(0_u64, u64::saturating_add),
        )
    });
    match (total, busiest) {
        (Some(total), Some((cpu, count))) => {
            let busiest_pct = if total > 0 {
                count as f32 / total as f32 * 100.0
            } else {
                0.0
            };
            let distribution = interrupts
                .per_logical_cpu
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, value)| *value > 0)
                .take(8)
                .map(|(index, value)| {
                    let pct = if total > 0 {
                        value as f32 / total as f32 * 100.0
                    } else {
                        0.0
                    };
                    format!("CPU{index} {pct:.0}%")
                })
                .collect::<Vec<_>>()
                .join(",");
            let distribution = if distribution.is_empty() {
                String::new()
            } else {
                format!(" · {distribution}")
            };
            Some(format!(
                "{} {total} · CPU{cpu} {count} ({busiest_pct:.0}%){distribution}",
                i18n::t("cpu.interrupt_total")
            ))
        }
        (Some(total), None) => Some(format!("{} {total}", i18n::t("cpu.interrupt_total"))),
        _ => None,
    }
}

/// One-line interrupt evidence for a narrow statistics rail. The full
/// [`cpu_interrupt_summary`] keeps the first eight non-zero CPU buckets for
/// rich views; a fixed-width card must not let that diagnostic vector become
/// a multi-line row that overlaps the following facts.
#[must_use]
pub fn cpu_interrupt_compact_summary(cpu: &CpuMetrics) -> Option<String> {
    let interrupts = cpu.interrupts.as_ref()?;
    let total = interrupts.total.or_else(|| {
        Some(
            interrupts
                .per_logical_cpu
                .iter()
                .copied()
                .fold(0_u64, u64::saturating_add),
        )
    })?;
    let busiest = interrupts
        .per_logical_cpu
        .iter()
        .copied()
        .enumerate()
        .max_by_key(|(_, value)| *value);
    match busiest {
        Some((logical_cpu, count)) => {
            let percent = if total > 0 {
                count as f32 / total as f32 * 100.0
            } else {
                0.0
            };
            Some(format!(
                "{} {total} · CPU{logical_cpu} {count} ({percent:.0}%)",
                i18n::t("cpu.interrupt_total")
            ))
        }
        None => Some(format!("{} {total}", i18n::t("cpu.interrupt_total"))),
    }
}

/// Ordered service diagnostic rows shared by all four service-detail surfaces.
/// The provider may expose only a subset of these facts; absent properties are
/// omitted, while a confirmed negative boolean remains a real value.
#[must_use]
pub fn service_diagnostics_rows(diagnostics: &ServiceDiagnostics) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    if let Some(state) = diagnostics.unit_file_state.as_deref() {
        rows.push((i18n::t("svc.unit_file_state").to_owned(), state.to_owned()));
    }
    if let Some(result) = diagnostics.result.as_deref() {
        rows.push((i18n::t("svc.result").to_owned(), result.to_owned()));
    }
    if let Some(status) = diagnostics.exec_main_status {
        let value = service_exit::exit_status_text(status);
        rows.push((i18n::t("svc.exit_status").to_owned(), value));
    }
    if let Some(oom) = diagnostics.oom_killed {
        rows.push((
            i18n::t("svc.oom_killed").to_owned(),
            i18n::t(if oom { "common.yes" } else { "common.no" }).to_owned(),
        ));
    }
    if let Some(hit) = diagnostics.start_limit_hit {
        rows.push((
            i18n::t("svc.start_limit_hit").to_owned(),
            i18n::t(if hit { "common.yes" } else { "common.no" }).to_owned(),
        ));
    }
    if let Some(required) = diagnostics.daemon_reload_required {
        rows.push((
            i18n::t("svc.daemon_reload_required").to_owned(),
            i18n::t(if required { "common.yes" } else { "common.no" }).to_owned(),
        ));
    }
    if let Some(next) = diagnostics.next_trigger_realtime.as_deref() {
        rows.push((i18n::t("svc.next_trigger").to_owned(), next.to_owned()));
    } else if let Some(next) = diagnostics.next_trigger_monotonic_usec {
        rows.push((i18n::t("svc.next_trigger").to_owned(), format!("{next} µs")));
    }
    if let Some(restarts) = diagnostics.restart_count {
        rows.push((i18n::t("svc.restarts").to_owned(), restarts.to_string()));
    }
    if let Some(burst) = diagnostics.start_limit_burst {
        let interval = diagnostics
            .start_limit_interval_usec
            .map_or_else(String::new, |value| format!(" / {value} µs"));
        rows.push((
            i18n::t("svc.start_limit").to_owned(),
            format!("{burst}{interval}"),
        ));
    }
    if let Some(timeout) = diagnostics.timeout_usec {
        rows.push((i18n::t("svc.timeout").to_owned(), format!("{timeout} µs")));
    }
    if let Some(user) = diagnostics.user.as_deref() {
        rows.push((i18n::t("svc.user").to_owned(), user.to_owned()));
    }
    if !diagnostics.triggers.is_empty() {
        rows.push((
            i18n::t("svc.triggers").to_owned(),
            diagnostics.triggers.join(" "),
        ));
    }
    if !diagnostics.triggered_by.is_empty() {
        rows.push((
            i18n::t("svc.triggered_by").to_owned(),
            diagnostics.triggered_by.join(" "),
        ));
    }
    if let Some(cause) = diagnostics.failure_cause() {
        let value = match cause {
            ServiceFailureCause::OomKilled => i18n::t("svc.failure_oom").to_owned(),
            ServiceFailureCause::StartLimitHit => i18n::t("svc.failure_start_limit").to_owned(),
            ServiceFailureCause::Timeout => i18n::t("svc.failure_timeout").to_owned(),
            ServiceFailureCause::ExitCode(status) => service_exit::exit_status_text(status),
            ServiceFailureCause::Result(result) => result,
        };
        rows.push((i18n::t("svc.failure_cause").to_owned(), value));
    }
    rows
}

/// Render the shared process-capability risk summary. The input is already a
/// typed, identity-bound observation; this helper only folds its risk level
/// and bounded effective capability names so every frontend uses one order and
/// one truncation rule.
#[must_use]
pub fn capabilities_summary(capabilities: &ProcessCapabilities) -> String {
    let risk = i18n::t(match capabilities.risk_level() {
        taskmanager_core::core::process_telemetry::CapabilityRiskLevel::Unknown => {
            "proc_insights.capabilities_unknown"
        }
        taskmanager_core::core::process_telemetry::CapabilityRiskLevel::None => {
            "proc_insights.capabilities_none"
        }
        taskmanager_core::core::process_telemetry::CapabilityRiskLevel::Elevated => {
            "proc_insights.capabilities_elevated"
        }
        taskmanager_core::core::process_telemetry::CapabilityRiskLevel::Critical => {
            "proc_insights.capabilities_critical"
        }
    });
    let Some(effective) = capabilities.effective_capabilities() else {
        return risk.to_owned();
    };
    if effective.is_empty() {
        return risk.to_owned();
    }
    let shown = effective
        .iter()
        .take(4)
        .map(|capability| capability.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if effective.len() > 4 {
        format!("{risk}: {shown} (+{})", effective.len() - 4)
    } else {
        format!("{risk}: {shown}")
    }
}

/// Render all namespace comparison rows in one deterministic summary. An
/// unavailable class stays visible as `—`; the summary never infers isolation
/// from a missing read.
#[must_use]
pub fn namespaces_summary(audit: &LinuxNamespaceAudit) -> String {
    let rows = audit
        .entries
        .iter()
        .map(|entry| {
            let state = match entry.status {
                NamespaceAuditStatus::Host { .. } => i18n::t("proc_insights.namespace_host"),
                NamespaceAuditStatus::Isolated { .. } => {
                    i18n::t("proc_insights.namespace_isolated")
                }
                NamespaceAuditStatus::Unavailable(_) => MISSING_VALUE,
            };
            format!("{} {state}", entry.kind.as_str())
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return i18n::t("proc_insights.namespaces_unknown").to_owned();
    }
    format!(
        "{}: {} · {} {}",
        i18n::t("proc_insights.namespaces"),
        rows.join(" · "),
        audit.isolated_count(),
        i18n::t("proc_insights.namespace_isolated_count")
    )
}

/// Render the additional container hardening facts that do not belong to the
/// namespace inode table: UID-0 mapping, rootfs mode, Snap confinement and
/// declared sandbox permissions. Empty facts are omitted; an observed false or
/// zero remains visible because callers pass typed values directly.
#[must_use]
pub fn sandbox_details_summary(isolation: &ProcessIsolation) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(uid) = isolation.root_uid_host {
        parts.push(format!("{} {uid}", i18n::t("proc_insights.root_uid_host")));
    }
    if let Some(read_only) = isolation.rootfs_read_only {
        parts.push(format!(
            "{} {}",
            i18n::t("proc_insights.rootfs"),
            i18n::t(if read_only {
                "proc_insights.read_only"
            } else {
                "proc_insights.read_write"
            })
        ));
    }
    if let Some(subreaper) = isolation.child_subreaper {
        parts.push(format!(
            "{} {}",
            i18n::t("proc_insights.child_subreaper"),
            i18n::t(if subreaper {
                "common.enabled"
            } else {
                "common.disabled"
            })
        ));
    }
    if let Some(confinement) = isolation.sandbox_confinement.as_deref() {
        parts.push(format!(
            "{} {confinement}",
            i18n::t("proc_insights.confinement")
        ));
    }
    if !isolation.sandbox_permissions.is_empty() {
        parts.push(format!(
            "{} {}",
            i18n::t("proc_insights.permissions"),
            isolation.sandbox_permissions.join(", ")
        ));
    }
    if isolation.has_privilege_escape_risk() {
        parts.push(i18n::t("proc_insights.privilege_escape_risk").to_owned());
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Calculate the system score from the same immutable snapshot every
/// frontend paints. This is a presentation join only; the score rules remain
/// pure and owned by core.
#[must_use]
pub fn health_score_for_snapshot(snapshot: &SystemSnapshot) -> Option<SystemHealthScore> {
    let pressure = snapshot.pressure.as_ref();
    let swap_used_pct = snapshot
        .memory
        .current_swap_used_bytes()
        .zip(snapshot.memory.current_swap_total_bytes())
        .filter(|(_, total)| *total > 0)
        .map(|(used, total)| used as f32 / total as f32 * 100.0);
    let thermal_throttled = if snapshot.cpu.packages.is_empty() {
        None
    } else {
        Some(
            snapshot
                .cpu
                .packages
                .iter()
                .any(|package| package.is_currently_throttled()),
        )
    };
    SystemHealthScore::calculate(HealthScoreInput {
        cpu_some_avg10: pressure
            .and_then(|value| value.cpu.current_value())
            .map(|value| value.some.avg10),
        memory_some_avg10: pressure
            .and_then(|value| value.memory.current_value())
            .map(|value| value.some.avg10),
        memory_full_avg10: pressure
            .and_then(|value| value.memory.current_value())
            .and_then(|value| value.full)
            .map(|value| value.avg10),
        io_some_avg10: pressure
            .and_then(|value| value.io.current_value())
            .map(|value| value.some.avg10),
        swap_used_pct,
        thermal_throttled,
    })
}

/// Render the score and a bounded deduction bill for dense status surfaces.
#[must_use]
pub fn health_score_summary(score: &SystemHealthScore) -> String {
    if score.deductions.is_empty() {
        return format!("{}: {}/100", i18n::t("system.health_score"), score.score);
    }
    let deductions = score
        .deductions
        .iter()
        .take(3)
        .map(|deduction| format!("-{} {}", deduction.points, deduction.evidence))
        .collect::<Vec<_>>()
        .join(" · ");
    let suffix = if score.deductions.len() > 3 {
        format!(" · +{}", score.deductions.len() - 3)
    } else {
        String::new()
    };
    format!(
        "{}: {}/100 · {}{}",
        i18n::t("system.health_score"),
        score.score,
        deductions,
        suffix
    )
}

/// Render the process executable/argv[0] mismatch warning using the shared
/// application comparison. It is intentionally an optional line: no warning
/// is emitted unless both physical path and command line were observed.
#[must_use]
pub fn command_identity_summary(item: &taskmanager_core::ProcessItem) -> Option<String> {
    let comparison =
        taskmanager_application::process_details_vm::command_identity_comparison(item)?;
    Some(format!(
        "{}: {} ≠ {}",
        i18n::t("proc_insights.command_identity"),
        comparison.executable_name,
        comparison.argv0
    ))
}
