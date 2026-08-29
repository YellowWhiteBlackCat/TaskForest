//! Pure data-layer folds for the health overlay.
//!
//! Observation reads stay here so the Ratatui renderer consumes display-ready
//! values and verdicts instead of redefining typed availability at paint time.

use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_shell::presentation::{bytes, missing_value};

#[derive(Clone, Copy, Debug)]
pub(super) enum Verdict {
    Good,
    Warn,
    Danger,
    Inactive,
}

pub(super) fn cpu_value(snapshot: Option<&SystemSnapshot>) -> String {
    snapshot
        .and_then(|snapshot| snapshot.cpu.current_global_usage_pct())
        .filter(|usage| usage.is_finite())
        .map_or_else(missing_value, |usage| {
            let cores = snapshot.map_or(0, |snapshot| snapshot.cpu.current_core_usage_len());
            t("health.cpu_value")
                .replacen("{percent}", &format!("{usage:.1}"), 1)
                .replacen("{cores}", &cores.to_string(), 1)
        })
}

pub(super) fn cpu_verdict(snapshot: Option<&SystemSnapshot>) -> Verdict {
    match snapshot {
        None => Verdict::Inactive,
        Some(snapshot)
            if snapshot
                .cpu
                .current_global_usage_pct()
                .is_some_and(f32::is_finite) =>
        {
            Verdict::Good
        }
        Some(_) => Verdict::Inactive,
    }
}

pub(super) fn memory_value(snapshot: Option<&SystemSnapshot>) -> String {
    snapshot.map_or_else(missing_value, |snapshot| {
        match (
            snapshot.memory.current_used_bytes(),
            snapshot.memory.current_total_bytes(),
        ) {
            (Some(used), Some(total)) if total > 0 => format!("{} / {}", bytes(used), bytes(total)),
            _ => missing_value(),
        }
    })
}

pub(super) fn memory_verdict(snapshot: Option<&SystemSnapshot>) -> Verdict {
    match snapshot {
        None => Verdict::Inactive,
        Some(snapshot) => {
            let percentage = snapshot.memory.used_percentage_observed();
            if percentage.is_some_and(|value| value.is_finite() && value >= 95.0) {
                Verdict::Danger
            } else if percentage.is_some() {
                Verdict::Good
            } else {
                Verdict::Inactive
            }
        }
    }
}

pub(super) fn storage_value(snapshot: Option<&SystemSnapshot>) -> String {
    match snapshot {
        None => missing_value(),
        Some(snapshot) if snapshot.disks.is_empty() => t("health.no_disks").to_owned(),
        Some(snapshot) => {
            let read = snapshot
                .disks
                .iter()
                .map(|disk| disk.current_read_bytes_per_sec())
                .try_fold(0u64, |sum, value| {
                    value.map(|value| sum.saturating_add(value))
                });
            let write = snapshot
                .disks
                .iter()
                .map(|disk| disk.current_write_bytes_per_sec())
                .try_fold(0u64, |sum, value| {
                    value.map(|value| sum.saturating_add(value))
                });
            t("health.disks_value")
                .replacen("{count}", &snapshot.disks.len().to_string(), 1)
                .replacen("{read}", &read.map_or_else(missing_value, bytes), 1)
                .replacen("{write}", &write.map_or_else(missing_value, bytes), 1)
        }
    }
}

pub(super) fn storage_verdict(snapshot: Option<&SystemSnapshot>) -> Verdict {
    match snapshot {
        None => Verdict::Inactive,
        Some(snapshot) if snapshot.disks.is_empty() => Verdict::Inactive,
        Some(_) => Verdict::Good,
    }
}

pub(super) fn network_value(snapshot: Option<&SystemSnapshot>) -> String {
    match snapshot {
        None => missing_value(),
        Some(snapshot) if snapshot.networks.is_empty() => t("health.no_interfaces").to_owned(),
        Some(snapshot) => {
            let rx = snapshot
                .networks
                .iter()
                .map(|network| network.current_rx_bytes_per_sec())
                .try_fold(0u64, |sum, value| {
                    value.map(|value| sum.saturating_add(value))
                });
            let tx = snapshot
                .networks
                .iter()
                .map(|network| network.current_tx_bytes_per_sec())
                .try_fold(0u64, |sum, value| {
                    value.map(|value| sum.saturating_add(value))
                });
            t("health.interfaces_value")
                .replacen("{count}", &snapshot.networks.len().to_string(), 1)
                .replacen("{rx}", &rx.map_or_else(missing_value, bytes), 1)
                .replacen("{tx}", &tx.map_or_else(missing_value, bytes), 1)
        }
    }
}

pub(super) fn network_verdict(snapshot: Option<&SystemSnapshot>) -> Verdict {
    match snapshot {
        None => Verdict::Inactive,
        Some(snapshot) if snapshot.networks.is_empty() => Verdict::Inactive,
        Some(_) => Verdict::Good,
    }
}

pub(super) fn gpu_value(snapshot: Option<&SystemSnapshot>) -> String {
    snapshot
        .and_then(|snapshot| snapshot.gpu.first())
        .map_or_else(|| t("health.no_gpu").to_owned(), |gpu| gpu.brand.clone())
}

pub(super) fn gpu_verdict(snapshot: Option<&SystemSnapshot>) -> Verdict {
    match snapshot.and_then(|snapshot| snapshot.gpu.first()) {
        None => Verdict::Inactive,
        Some(gpu) if gpu.current_utilization_pct().is_some() => Verdict::Good,
        Some(_) => Verdict::Inactive,
    }
}
