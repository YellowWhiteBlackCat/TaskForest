//! Linux kernel Pressure Stall Information (/proc/pressure/{cpu,memory,io}) reader.

use std::fs;
use std::path::Path;

use taskmanager_core::{
    FailureKind, PressureWindow, ProviderId, ResourcePressure, ScalarObservation, SourceOutcome,
    SourceStatus, SystemPressureSnapshot,
};

pub(super) const PSI_PROVIDER: ProviderId = ProviderId::borrowed("linux.host.proc-pressure");

pub(super) fn parse_psi_window(line: &str) -> Option<PressureWindow> {
    let mut avg10 = 0.0f32;
    let mut avg60 = 0.0f32;
    let mut avg300 = 0.0f32;
    let mut total_us = 0u64;

    for token in line.split_whitespace().skip(1) {
        if let Some((k, v)) = token.split_once('=') {
            match k {
                "avg10" => avg10 = v.parse().unwrap_or(0.0),
                "avg60" => avg60 = v.parse().unwrap_or(0.0),
                "avg300" => avg300 = v.parse().unwrap_or(0.0),
                "total" => total_us = v.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    Some(PressureWindow::new(avg10, avg60, avg300, total_us))
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
mod tests {
    use super::*;

    #[test]
    fn parse_psi_window_parses_real_kernel_line() {
        let line = "some avg10=0.03 avg60=0.17 avg300=0.19 total=703209861";
        let window = parse_psi_window(line).expect("must parse psi window");
        assert!((window.avg10 - 0.03).abs() < f32::EPSILON);
        assert!((window.avg60 - 0.17).abs() < f32::EPSILON);
        assert!((window.avg300 - 0.19).abs() < f32::EPSILON);
        assert_eq!(window.total_us, 703209861);
    }

    #[test]
    fn parse_psi_file_parses_some_and_full() {
        let content = "some avg10=1.25 avg60=0.50 avg300=0.10 total=50000\nfull avg10=0.10 avg60=0.05 avg300=0.01 total=12000\n";
        let resource = parse_psi_file(content).expect("must parse psi file");
        assert!((resource.some.avg10 - 1.25).abs() < f32::EPSILON);
        assert_eq!(resource.some.total_us, 50000);
        let full = resource.full.expect("full window must be present");
        assert!((full.avg10 - 0.10).abs() < f32::EPSILON);
        assert_eq!(full.total_us, 12000);
    }

    #[test]
    fn parse_psi_file_handles_some_only_like_cpu() {
        let content = "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
        let resource = parse_psi_file(content).expect("must parse psi file with some only");
        assert_eq!(resource.some.total_us, 0);
        assert!(resource.full.is_none());
    }
}

