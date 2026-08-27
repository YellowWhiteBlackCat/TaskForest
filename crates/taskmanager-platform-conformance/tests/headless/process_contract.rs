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
fn out_of_range_cpu_fails() {
    let mut item = row(1, None);
    item.apply_scalar_observations(ProcessScalarObservations {
        cpu_percentage: ScalarObservation::available(101.0, 1),
        ..Default::default()
    });
    assert!(assert_process_rows_consistent(&[item]).is_err());
}
