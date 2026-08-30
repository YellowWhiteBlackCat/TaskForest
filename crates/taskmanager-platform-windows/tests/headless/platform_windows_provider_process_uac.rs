//! Headless behavior tests for the stage-2 runas transport's pure surface
//! (ADR-035). Everything here runs on ANY host: the launch-result mapping
//! and the bounded reply read are pure/seam logic; the real consent, launch,
//! and channel behavior is Windows on-box receipt territory and is never
//! fabricated headless.

use super::*;

use taskmanager_escalation::uac::UacCrossingObservation;
use taskmanager_windows_api::RunasLaunchOutcome;

#[test]
fn boundary_launch_results_keep_their_distinct_transport_facts() {
    // A user refusal (ERROR_CANCELLED via ShellExecuteExW), a missing helper
    // install (ERROR_FILE_NOT_FOUND), the abandoned deadline, and the dormant
    // call group are four distinct boundary results; none may collapse.
    assert_eq!(
        map_runas_launch(
            RunasLaunchOutcome::LaunchFailed { win32_error: 1223 },
            || unreachable!("no reply read on a failed launch")
        ),
        UacCrossingObservation::LaunchFailed { win32_error: 1223 }
    );
    assert_eq!(
        map_runas_launch(RunasLaunchOutcome::LaunchFailed { win32_error: 2 }, || None),
        UacCrossingObservation::LaunchFailed { win32_error: 2 }
    );
    assert_eq!(
        map_runas_launch(RunasLaunchOutcome::DeadlineExceeded, || None),
        UacCrossingObservation::DeadlineExceeded
    );
    assert_eq!(
        map_runas_launch(RunasLaunchOutcome::Unsupported, || None),
        UacCrossingObservation::TransportUnwired
    );
}

#[test]
fn a_completed_launch_reads_the_reply_channel_exactly_once_on_success() {
    let payload =
        br#"{"schema":1,"status":"applied","pid":42,"start_token":9000,"operation":"kill"}"#
            .to_vec();
    let mut reads = 0;
    let observation = map_runas_launch(RunasLaunchOutcome::Completed { exit_code: 0 }, || {
        reads += 1;
        Some(payload.clone())
    });
    assert_eq!(observation, UacCrossingObservation::HelperReply { payload });
    assert_eq!(reads, 1, "the channel is read exactly once");
}

#[test]
fn an_unreadable_reply_after_completion_is_an_empty_reply_never_success() {
    // The helper completed but the channel could not be read: the honest fact
    // is an empty reply, which the shared contract reader classifies as a
    // protocol violation — never a fabricated Applied.
    let observation = map_runas_launch(RunasLaunchOutcome::Completed { exit_code: 0 }, || None);
    assert_eq!(
        observation,
        UacCrossingObservation::HelperReply {
            payload: Vec::new()
        }
    );
}

#[test]
fn the_reply_read_is_bounded_and_truncation_breaks_the_contract_not_the_bound() {
    // A scratch file under the repository test root exercises the bounded
    // reader on any host: normal payloads pass through byte-exact and an
    // oversized payload is cut at the cap (the truncated JSON then fails
    // contract parsing downstream — it can never parse into a fabricated
    // Applied).
    let directory = crate::test_support::repo_temp_dir();
    let path = directory.join(format!(
        "taskforest-uac-driver-test-{}-bounded.json",
        std::process::id()
    ));
    std::fs::write(&path, b"{\"schema\":1}").expect("write the fixture channel");
    assert_eq!(read_reply_bounded(&path), Some(b"{\"schema\":1}".to_vec()));

    let oversized = directory.join(format!(
        "taskforest-uac-driver-test-{}-oversized.json",
        std::process::id()
    ));
    let filler = vec![b'x'; MAX_REPLY_BYTES + 1024];
    std::fs::write(&oversized, &filler).expect("write the oversized fixture");
    let read = read_reply_bounded(&oversized).expect("the read itself succeeds");
    assert_eq!(read.len(), MAX_REPLY_BYTES, "the read is cut at the cap");

    let missing = directory.join("taskforest-uac-driver-test-definitely-absent.json");
    let _ = std::fs::remove_file(&missing);
    assert_eq!(read_reply_bounded(&missing), None);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&oversized);
}
