use super::*;

const DPM_CLOCKS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/amdgpu_pp_dpm_sclk.txt"
));

#[test]
fn dpm_clock_fixture_selects_active_and_highest_runtime_level() {
    assert_eq!(
        parse_amdgpu_dpm_clock(DPM_CLOCKS),
        (Some(1_800), Some(2_400))
    );
}

#[test]
fn zero_throttle_status_is_a_supported_empty_reason() {
    assert!(parses_as_zero("0"));
    assert!(parses_as_zero("0x00000000"));
    assert!(!parses_as_zero("0x00000001"));
    assert!(parses_as_nonzero("1"));
    assert!(parses_as_nonzero("0x00000001"));
    assert!(!parses_as_nonzero("not-a-bitmask"));
}

#[test]
fn native_io_failures_keep_missing_permission_and_disappearance_distinct() {
    assert_eq!(
        io_failure(&std::io::Error::from(std::io::ErrorKind::NotFound), false),
        FailureKind::Unsupported
    );
    assert_eq!(
        io_failure(&std::io::Error::from(std::io::ErrorKind::NotFound), true),
        FailureKind::TemporarilyUnavailable
    );
    assert_eq!(
        io_failure(
            &std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            false
        ),
        FailureKind::PermissionDenied
    );
}
