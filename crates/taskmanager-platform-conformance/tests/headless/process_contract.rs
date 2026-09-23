use super::*;
use taskmanager_core::{ProcessScalarObservations, ScalarObservation};

fn row(pid: u32, parent_pid: Option<u32>) -> ProcessItem {
    let mut item = ProcessItem::new(pid, format!("worker-{pid}"));
    item.parent_pid = parent_pid;
    item
}

#[test]
fn consistent_rows_pass() {
    assert_eq!(
        assert_process_rows_consistent(&[row(1, None), row(2, Some(1))]),
        Ok(())
    );
}

#[test]
fn duplicate_pids_fail() {
    assert!(assert_process_rows_consistent(&[row(1, None), row(1, Some(0))]).is_err());
}

#[test]
fn multi_core_cpu_above_one_hundred_percent_passes() {
    // Per-core normalisation: 212% means the process used 2.12 cores, which a
    // multi-threaded build legitimately does on a loaded host.
    let mut item = row(1, None);
    item.apply_scalar_observations(ProcessScalarObservations {
        cpu_percentage: ScalarObservation::available(212.0, 1),
        ..Default::default()
    });
    assert_eq!(assert_process_rows_consistent(&[item]), Ok(()));
}

#[test]
fn implausible_cpu_fails() {
    let mut item = row(1, None);
    item.apply_scalar_observations(ProcessScalarObservations {
        cpu_percentage: ScalarObservation::available(1.0e9, 1),
        ..Default::default()
    });
    assert!(assert_process_rows_consistent(&[item]).is_err());
}
