use std::path::Path;

use taskmanager_core::ProcessMetadataAvailability;

use super::*;

fn owner(uid: u32, label: Option<&str>, observed_at_ms: u64) -> ProcessOwner {
    let mut labels = HashMap::new();
    if let Some(label) = label {
        labels.insert(uid, label.to_owned());
    }
    owner_observation(Ok(uid), &Ok(labels), observed_at_ms)
        .current_value()
        .cloned()
        .expect("owner identity should be current")
}

#[test]
fn status_and_passwd_parsers_preserve_numeric_identity() {
    assert_eq!(
        parse_status_uid("Name:\tworker\nUid:\t1000\t1000\t1000\t1000\n"),
        Ok(1000)
    );
    assert_eq!(
        parse_status_uid("Name:\tworker\n"),
        Err(ProcessMetadataFailure::ProviderFault)
    );

    let labels = parse_passwd_labels(
        "root:x:0:0:root:/root:/bin/sh\nbad:x:nope:0::/:/bin/false\nalice:x:1000:1000::/home/<user>:/bin/sh\n",
    );
    assert_eq!(labels.get(&0).map(String::as_str), Some("root"));
    assert_eq!(labels.get(&1000).map(String::as_str), Some("alice"));
    assert!(!labels.contains_key(&1));
}

#[test]
fn missing_label_and_failed_label_source_remain_distinct() {
    let missing = owner_observation(Ok(4242), &Ok(HashMap::new()), 10);
    assert_eq!(
        missing.availability(),
        ProcessMetadataAvailability::Available
    );
    assert_eq!(
        missing.current_value().map(ProcessOwner::display_value),
        Some("4242".to_owned())
    );
    assert_eq!(
        label_outcome(&Ok(4242), &Ok(HashMap::new())),
        SourceOutcome::Empty
    );

    let failed = owner_observation(Ok(4242), &Err(ProcessMetadataFailure::PermissionDenied), 20);
    assert_eq!(
        failed.availability(),
        ProcessMetadataAvailability::Partial(ProcessMetadataFailure::PermissionDenied)
    );
    assert_eq!(
        failed.current_value().map(ProcessOwner::display_value),
        Some("4242".to_owned())
    );
    assert_eq!(failed.last_success_ms(), Some(20));
    assert_eq!(
        label_outcome(&Ok(4242), &Err(ProcessMetadataFailure::PermissionDenied)),
        SourceOutcome::Unavailable(taskmanager_core::FailureKind::PermissionDenied)
    );
}

#[test]
fn executable_io_distinguishes_absence_race_and_failures() {
    let absent = executable_observation(
        Err(io::Error::from(io::ErrorKind::NotFound)),
        Some(Ok(())),
        10,
    );
    assert_eq!(absent.availability(), ProcessMetadataAvailability::Absent);
    assert_eq!(absent.last_success_ms(), Some(10));
    assert_eq!(observation_outcome(&absent), SourceOutcome::Empty);

    let raced = executable_observation(
        Err(io::Error::from(io::ErrorKind::NotFound)),
        Some(Err(io::Error::from(io::ErrorKind::NotFound))),
        20,
    );
    assert_eq!(
        raced.availability(),
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::PidRace)
    );
    assert_eq!(
        observation_outcome(&raced),
        SourceOutcome::Unavailable(taskmanager_core::FailureKind::IdentityChanged)
    );

    for (kind, failure) in [
        (
            io::ErrorKind::PermissionDenied,
            ProcessMetadataFailure::PermissionDenied,
        ),
        (
            io::ErrorKind::Unsupported,
            ProcessMetadataFailure::Unsupported,
        ),
        (
            io::ErrorKind::InvalidData,
            ProcessMetadataFailure::ProviderFault,
        ),
    ] {
        let observed = executable_observation(Err(io::Error::from(kind)), None, 30);
        assert_eq!(
            observed.availability(),
            ProcessMetadataAvailability::Unavailable(failure)
        );
    }
}

#[test]
fn metadata_failure_recovery_retains_only_the_same_identity() {
    let previous = ProcessMetadataObservations {
        owner: ProcessMetadataObservation::available(owner(1000, Some("alice"), 10), 10),
        executable_path: ProcessMetadataObservation::available(
            Path::new("/usr/bin/worker").to_path_buf(),
            10,
        ),
    };
    let previous_item = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .metadata_observations(previous)
        .scalar_observations(taskmanager_core::ProcessScalarObservations {
            start_token: taskmanager_core::ScalarObservation::available(600, 10),
            start_time_secs: taskmanager_core::ScalarObservation::available(1_720_000_000, 10),
            ..taskmanager_core::ProcessScalarObservations::default()
        })
        .build();
    let failed = observations_from_results(
        Err(ProcessMetadataFailure::PermissionDenied),
        &Ok(HashMap::new()),
        ProcessMetadataObservation::unavailable(ProcessMetadataFailure::PermissionDenied),
        20,
    );
    let retained = retain_for_same_identity(failed.clone(), Some(600), Some(&previous_item));
    assert_eq!(
        retained.owner.availability(),
        ProcessMetadataAvailability::Stale(ProcessMetadataFailure::PermissionDenied)
    );
    assert_eq!(retained.owner.last_success_ms(), Some(10));
    assert_eq!(failed.owner.last_known_value(), None);

    let reused_pid = retain_for_same_identity(failed, Some(601), Some(&previous_item));
    assert_eq!(
        reused_pid.owner.availability(),
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::PermissionDenied)
    );
    assert_eq!(reused_pid.owner.last_success_ms(), None);

    let recovered = observations_from_results(
        Ok(1000),
        &Ok(HashMap::from([(1000, "alice".to_owned())])),
        ProcessMetadataObservation::available(Path::new("/usr/bin/worker-v2").to_path_buf(), 30),
        30,
    );
    assert_eq!(
        recovered.owner.availability(),
        ProcessMetadataAvailability::Available
    );
    assert_eq!(recovered.owner.last_success_ms(), Some(30));
    assert_eq!(
        recovered
            .executable_path
            .current_value()
            .map(PathBuf::as_path),
        Some(Path::new("/usr/bin/worker-v2"))
    );
}

#[test]
fn metadata_io_classification_keeps_not_found_separate_from_pid_race() {
    assert_eq!(
        classify_metadata_io(&io::Error::from(io::ErrorKind::NotFound)),
        ProcessMetadataFailure::NotFound
    );
    assert_eq!(
        classify_process_io(&io::Error::from(io::ErrorKind::NotFound)),
        ProcessMetadataFailure::PidRace
    );
}

#[test]
fn unconfirmed_start_token_blocks_all_metadata_reads_and_current_evidence() {
    let (observations, evidence) = observe_process_metadata(
        u32::MAX,
        &Ok(HashMap::from([(1000, "alice".to_owned())])),
        20,
        Err(FailureKind::IdentityChanged),
        None::<&taskmanager_core::ProcessItem>,
    );

    assert_eq!(
        observations.owner.availability(),
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::PidRace)
    );
    assert_eq!(
        observations.executable_path.availability(),
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::PidRace)
    );
    assert_eq!(
        evidence.owner_identity,
        SourceOutcome::Unavailable(FailureKind::IdentityChanged)
    );
    assert_eq!(
        evidence.owner_label,
        SourceOutcome::Unavailable(FailureKind::IdentityChanged)
    );
    assert_eq!(
        evidence.executable_path,
        SourceOutcome::Unavailable(FailureKind::IdentityChanged)
    );
}
