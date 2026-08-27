use super::*;

const VALID_ROW: &str = "259 0 nvme0n1 10 0 20 0 30 0 40 0 0 50 0";

#[test]
fn malformed_counter_is_partial_and_never_becomes_zero() {
    let observed = parse_diskstats_observation(&format!(
        "{VALID_ROW}\n259 1 nvme1n1 broken 0 20 0 30 0 40 0 0 50 0\n"
    ));

    assert_eq!(
        observed.source.outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(observed.source.item_count, 1);
    assert_eq!(
        observed.get("nvme0n1").map(|row| row.sectors_read),
        Some(20)
    );
    assert!(
        !observed.contains_key("nvme1n1"),
        "malformed counters must not appear as believable zeroes"
    );
    assert_eq!(observed.failure_for("nvme1n1"), FailureKind::ProviderFault);
}

#[test]
fn empty_diskstats_is_authoritative_empty_but_malformed_only_is_unavailable() {
    let empty = parse_diskstats_observation("");
    assert_eq!(empty.source.outcome, SourceOutcome::Empty);

    let malformed = parse_diskstats_observation("not a diskstats row\n");
    assert_eq!(
        malformed.source.outcome,
        SourceOutcome::Unavailable(FailureKind::ProviderFault)
    );
}

#[test]
fn missing_diskstats_file_is_typed_unsupported() {
    let missing = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-missing-diskstats-{}",
        std::process::id()
    ));
    let observed = read_proc_diskstats_from(&missing);

    assert!(observed.is_empty());
    assert_eq!(
        observed.source.outcome,
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    );
}
