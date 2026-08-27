use super::*;

fn stat(pid: u32, start: u64) -> String {
    format!("{pid} (worker with spaces) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 {start} 20")
}

#[test]
fn stat_identity_handles_spaced_comm() {
    assert_eq!(parse_start_time_ticks(&stat(42, 987_654)), Some(987_654));
    assert_eq!(parse_start_time_ticks("malformed"), None);
}

#[test]
fn permission_errors_are_not_collapsed_into_stale() {
    assert_eq!(
        status_from_io_error(&std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        DeviceStatus::PermissionDenied
    );
    assert_eq!(
        status_from_io_error(&std::io::Error::from(std::io::ErrorKind::NotFound)),
        DeviceStatus::Stale
    );
}
