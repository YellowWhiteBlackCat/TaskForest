//! CPU rail/detail folds shared by every frontend: package topology, cpuidle
//! evidence, power limits, interrupt distribution, the cumulative
//! thermal-throttle trigger counters, and the real-time thermal-status /
//! PROCHOT assertion from the privileged MSR lane. Each fold is
//! renderer-neutral and never fabricates an unobserved value.

use taskmanager_application::MsrReadoutState;
use taskmanager_application::i18n;
use taskmanager_core::core::metrics::{CpuMetrics, MsrReadoutSnapshot, MsrThermalStatusReadout};

use super::{MISSING_VALUE, megahertz, missing_value};

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

/// Compact per-package summary of the cumulative thermal-throttle trigger
/// counters (`power.thermal-throttle-events`) carried by every
/// `CpuPackageMetrics`: one `S{package_id}` segment per package that observed
/// at least one counter, naming the package-level and per-core event counts.
/// An observed package with an unobserved sibling counter keeps the shared
/// dash; a package with no observed counter contributes no segment and a
/// projection with no observed counter at all returns `None` so the caller
/// omits the row entirely — a missing observation is never fabricated as `0`.
/// This is the SINGLE fold (ADR-020): the GPUI, Iced, TUI, and Bevy CPU
/// surfaces must not re-implement it.
#[must_use]
pub fn cpu_thermal_throttle_summary(cpu: &CpuMetrics) -> Option<String> {
    let mut packages = Vec::new();
    for package in &cpu.packages {
        if package.package_throttle_count.is_none() && package.core_throttle_count.is_none() {
            continue;
        }
        let package_count = package
            .package_throttle_count
            .map_or_else(missing_value, |value| value.to_string());
        let core_count = package
            .core_throttle_count
            .map_or_else(missing_value, |value| value.to_string());
        packages.push(format!(
            "S{} {} {package_count} · {} {core_count}",
            package.package_id,
            i18n::t("cpu.throttle_package"),
            i18n::t("cpu.throttle_core"),
        ));
    }
    (!packages.is_empty()).then(|| packages.join(" | "))
}

/// The shared word for one real-time `IA32_THERM_STATUS` bit (`Some(true)` =
/// asserted, `Some(false)` = clear). An absent bit — the register is
/// unimplemented or unreadable — is the shared honest dash, never a fabricated
/// clear state. This is the SINGLE real-time thermal-status vocabulary
/// (ADR-020); no frontend spells the state itself.
#[must_use]
pub fn thermal_status_state_label(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => i18n::t("cpu.thermal_status_asserted"),
        Some(false) => i18n::t("cpu.thermal_status_clear"),
        None => MISSING_VALUE,
    }
}

/// Compact per-node summary of the real-time thermal-status / PROCHOT
/// assertion (`IA32_THERM_STATUS` bit 0) carried by the privileged
/// `telemetry.cpu.msr` lane: one `CPU {n} {state}` segment per readout node.
/// An empty slice — the lane never ran, or the CPU exposes no
/// `IA32_THERM_STATUS` — returns `None` so the caller omits the whole row
/// rather than fabricating a clear state; a node whose register is
/// unreadable/unimplemented keeps the shared dash. This is the SINGLE
/// real-time-status fold (ADR-020): GPUI, Iced, TUI, and Bevy must not
/// re-implement it.
#[must_use]
pub fn cpu_thermal_status_summary(thermal: &[MsrThermalStatusReadout]) -> Option<String> {
    (!thermal.is_empty()).then(|| {
        thermal
            .iter()
            .map(|readout| {
                format!(
                    "CPU {} {}",
                    readout.cpu,
                    thermal_status_state_label(readout.thermal_status)
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    })
}

/// The last accepted MSR-readout snapshot of the current request session, or
/// `None` when the privileged lane produced none (it never ran, is in flight
/// with no last-good, or failed with no last-good). The request-session
/// lifecycle is owned by `taskmanager-application`; this fold only reads it so
/// every frontend observes the same absence semantics.
#[must_use]
pub fn msr_readout_snapshot(state: &MsrReadoutState) -> Option<&MsrReadoutSnapshot> {
    match state {
        MsrReadoutState::Ready(ready) => Some(&ready.snapshot),
        MsrReadoutState::Loading {
            last_good: Some(ready),
            ..
        } => Some(&ready.snapshot),
        MsrReadoutState::Failed(failed) => failed.last_good.as_ref().map(|ready| &ready.snapshot),
        MsrReadoutState::Loading {
            last_good: None, ..
        }
        | MsrReadoutState::Closed => None,
    }
}

/// Session-aware fold: the real-time thermal-status summary of the current
/// MSR-readout session, or `None` when no accepted snapshot exists. This is
/// the entry every frontend calls so the privileged-lane absence is one fact,
/// not four copies.
#[must_use]
pub fn msr_thermal_status_summary(state: &MsrReadoutState) -> Option<String> {
    msr_readout_snapshot(state).and_then(|snapshot| cpu_thermal_status_summary(&snapshot.thermal))
}
