use super::*;

#[test]
fn boot_time_parser_never_invents_zero_for_missing_or_malformed_input() {
    assert_eq!(
        parse_boot_time("cpu 1 2 3\nbtime 1720000000\n"),
        Some(1_720_000_000)
    );
    assert_eq!(parse_boot_time("cpu 1 2 3\n"), None);
    assert_eq!(parse_boot_time("btime not-a-number\n"), None);
}

#[test]
fn procfs_io_failures_have_stable_typed_classification() {
    assert_eq!(
        io_failure(&io::Error::from(io::ErrorKind::PermissionDenied)),
        FailureKind::PermissionDenied
    );
    assert_eq!(
        io_failure(&io::Error::from(io::ErrorKind::NotFound)),
        FailureKind::TemporarilyUnavailable
    );
    assert_eq!(
        io_failure(&io::Error::from(io::ErrorKind::InvalidData)),
        FailureKind::ProviderFault
    );
}
