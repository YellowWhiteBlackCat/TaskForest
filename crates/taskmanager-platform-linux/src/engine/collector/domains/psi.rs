//! Linux kernel Pressure Stall Information (/proc/pressure/{cpu,memory,io}) reader.

use std::fs;
use std::path::Path;

use taskmanager_core::{
    FailureKind, PressureWindow, ProviderId, ResourcePressure, ScalarObservation, SourceOutcome,
    SourceStatus, SystemPressureSnapshot,
};

pub(super) const PSI_PROVIDER: ProviderId = ProviderId::borrowed("linux.host.proc-pressure");

pub(super) fn parse_psi_window(line: &str) -> Option<PressureWindow> {
    let mut fields = line.split_whitespace();
    match fields.next()? {
        "some" | "full" => {}
        _ => return None,
    }
    let mut avg10 = None;
    let mut avg60 = None;
    let mut avg300 = None;
    let mut total_us = None;

    for token in fields {
        if let Some((k, v)) = token.split_once('=') {
            match k {
                "avg10" => avg10 = parse_psi_percentage(v),
                "avg60" => avg60 = parse_psi_percentage(v),
                "avg300" => avg300 = parse_psi_percentage(v),
                "total" => total_us = v.parse::<u64>().ok(),
                _ => {}
            }
        }
    }
    Some(PressureWindow::new(avg10?, avg60?, avg300?, total_us?))
}

fn parse_psi_percentage(value: &str) -> Option<f32> {
    let value = value.parse::<f32>().ok()?;
    value
        .is_finite()
        .then_some(value)
        .filter(|value| (0.0..=100.0).contains(value))
}

pub(super) fn parse_psi_file(content: &str) -> Option<ResourcePressure> {
    let mut some = None;
    let mut full = None;
    for line in content.lines() {
        if line.starts_with("some ") {
            some = parse_psi_window(line);
        } else if line.starts_with("full ") {
            full = parse_psi_window(line);
        }
    }
    some.map(|s| ResourcePressure { some: s, full })
}

pub(super) fn read_psi_resource(path: &Path) -> Result<ResourcePressure, FailureKind> {
    let content = fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            FailureKind::Unsupported
        } else if e.kind() == std::io::ErrorKind::PermissionDenied {
            FailureKind::PermissionDenied
        } else {
            FailureKind::TemporarilyUnavailable
        }
    })?;
    parse_psi_file(&content).ok_or(FailureKind::ProviderFault)
}

pub(super) struct PsiObservation {
    pub(super) snapshot: ScalarObservation<SystemPressureSnapshot>,
    pub(super) source: SourceStatus,
}

pub(super) fn observe_psi(pressure_dir: &Path, now_ms: u64) -> PsiObservation {
    let cpu_res = read_psi_resource(&pressure_dir.join("cpu"));
    let mem_res = read_psi_resource(&pressure_dir.join("memory"));
    let io_res = read_psi_resource(&pressure_dir.join("io"));

    let any_success = cpu_res.is_ok() || mem_res.is_ok() || io_res.is_ok();

    let cpu_obs = match cpu_res {
        Ok(res) => ScalarObservation::available(res, now_ms),
        Err(f) => ScalarObservation::unavailable(f),
    };
    let mem_obs = match mem_res {
        Ok(res) => ScalarObservation::available(res, now_ms),
        Err(f) => ScalarObservation::unavailable(f),
    };
    let io_obs = match io_res {
        Ok(res) => ScalarObservation::available(res, now_ms),
        Err(f) => ScalarObservation::unavailable(f),
    };

    let snapshot = SystemPressureSnapshot {
        cpu: cpu_obs,
        memory: mem_obs,
        io: io_obs,
    };

    if any_success {
        PsiObservation {
            snapshot: ScalarObservation::available(snapshot, now_ms),
            source: SourceStatus {
                provider: PSI_PROVIDER,
                outcome: SourceOutcome::Available,
                item_count: 3,
            },
        }
    } else {
        let failure = match cpu_res.err().or(mem_res.err()).or(io_res.err()) {
            Some(f) => f,
            None => FailureKind::Unsupported,
        };
        PsiObservation {
            snapshot: ScalarObservation::unavailable(failure),
            source: SourceStatus {
                provider: PSI_PROVIDER,
                outcome: SourceOutcome::Unavailable(failure),
                item_count: 0,
            },
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_collector_domains_psi_tests.rs"]
mod tests;
