use super::*;

struct FixedSetupProcess {
    reply: SetupReply,
}

enum SetupReply {
    Output {
        status_code: Option<i32>,
        stderr: Vec<u8>,
    },
    Error(io::ErrorKind, String),
}

impl SetupScriptProcess for FixedSetupProcess {
    fn run(&self, _operation: SetupScriptOperation) -> io::Result<SetupScriptProcessOutput> {
        match &self.reply {
            SetupReply::Output {
                status_code,
                stderr,
            } => Ok(SetupScriptProcessOutput {
                status_code: *status_code,
                stderr: stderr.clone(),
            }),
            SetupReply::Error(kind, detail) => Err(io::Error::new(*kind, detail.clone())),
        }
    }
}

#[test]
fn setup_operation_arguments_are_fixed_and_not_user_supplied() {
    assert_eq!(SetupScriptOperation::Install.argument(), "install");
    assert_eq!(SetupScriptOperation::Revert.argument(), "revert");
}

#[test]
fn setup_helper_success_and_typed_failures_preserve_exit_meaning() {
    let success = FixedSetupProcess {
        reply: SetupReply::Output {
            status_code: Some(0),
            stderr: Vec::new(),
        },
    };
    assert_eq!(
        invoke_setup_script_with(&success, SetupScriptOperation::Install),
        SetupScriptOutcome::Success
    );

    for (status_code, expected) in [
        (64, SetupScriptFailure::Rejected),
        (10, SetupScriptFailure::Rejected),
        (11, SetupScriptFailure::MissingDependency),
        (69, SetupScriptFailure::MissingDependency),
        (74, SetupScriptFailure::ProviderFault),
        (75, SetupScriptFailure::Rejected),
        (126, SetupScriptFailure::HelperUnavailable),
        (127, SetupScriptFailure::HelperUnavailable),
    ] {
        let process = FixedSetupProcess {
            reply: SetupReply::Output {
                status_code: Some(status_code),
                stderr: format!("setup status {status_code}").into_bytes(),
            },
        };
        assert!(matches!(
            invoke_setup_script_with(&process, SetupScriptOperation::Revert),
            SetupScriptOutcome::Failed { kind, .. } if kind == expected
        ));
    }
}

#[test]
fn setup_helper_spawn_and_signal_failures_never_claim_success() {
    let process = FixedSetupProcess {
        reply: SetupReply::Error(io::ErrorKind::NotFound, "pkexec missing".to_owned()),
    };
    assert!(matches!(
        invoke_setup_script_with(&process, SetupScriptOperation::Install),
        SetupScriptOutcome::Failed {
            kind: SetupScriptFailure::HelperUnavailable,
            ..
        }
    ));

    let process = FixedSetupProcess {
        reply: SetupReply::Output {
            status_code: None,
            stderr: b"terminated".to_vec(),
        },
    };
    assert!(matches!(
        invoke_setup_script_with(&process, SetupScriptOperation::Install),
        SetupScriptOutcome::Failed {
            kind: SetupScriptFailure::HelperUnavailable,
            ..
        }
    ));
}

#[test]
fn setup_helper_diagnostic_is_bounded() {
    let process = FixedSetupProcess {
        reply: SetupReply::Output {
            status_code: Some(75),
            stderr: vec![b'x'; 2048],
        },
    };
    let SetupScriptOutcome::Failed { detail, .. } =
        invoke_setup_script_with(&process, SetupScriptOperation::Install)
    else {
        panic!("expected typed setup failure");
    };
    assert_eq!(detail.len(), 512);
}

#[test]
fn setup_helper_passes_only_the_typed_operation_to_the_process_seam() {
    use std::cell::Cell;

    struct RecordingProcess {
        seen: Cell<Option<SetupScriptOperation>>,
    }

    impl SetupScriptProcess for RecordingProcess {
        fn run(&self, operation: SetupScriptOperation) -> io::Result<SetupScriptProcessOutput> {
            self.seen.set(Some(operation));
            Ok(SetupScriptProcessOutput {
                status_code: Some(0),
                stderr: Vec::new(),
            })
        }
    }

    let process = RecordingProcess {
        seen: Cell::new(None),
    };
    assert_eq!(
        invoke_setup_script_with(&process, SetupScriptOperation::Revert),
        SetupScriptOutcome::Success
    );
    assert_eq!(process.seen.get(), Some(SetupScriptOperation::Revert));
}

#[test]
fn runner_timeout_is_reported_as_an_abandoned_crossing() {
    // The bounded runner maps its deadline kill onto ErrorKind::TimedOut; the
    // first-run mapping must not mislabel it as a spawn failure.
    let process = FixedSetupProcess {
        reply: SetupReply::Error(
            io::ErrorKind::TimedOut,
            "did not finish within the bounded deadline and was killed".to_owned(),
        ),
    };
    let SetupScriptOutcome::Failed { kind, detail } =
        invoke_setup_script_with(&process, SetupScriptOperation::Install)
    else {
        panic!("expected typed setup failure");
    };
    assert_eq!(kind, SetupScriptFailure::HelperUnavailable);
    assert!(detail.contains("killed at its deadline"), "{detail}");
    assert!(!detail.contains("could not invoke"), "{detail}");
}

#[test]
fn invalid_utf8_diagnostics_remain_bounded_and_never_claim_success() {
    let process = FixedSetupProcess {
        reply: SetupReply::Output {
            status_code: Some(74),
            stderr: vec![0xff; 512],
        },
    };
    let SetupScriptOutcome::Failed { detail, kind } =
        invoke_setup_script_with(&process, SetupScriptOperation::Install)
    else {
        panic!("expected typed setup failure");
    };
    assert_eq!(kind, SetupScriptFailure::ProviderFault);
    assert!(detail.len() <= 512);
}

#[cfg(target_os = "linux")]
#[test]
fn setup_policy_matches_fixed_helper_path_and_action() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packaging/linux/io.github.YellowWhiteBlackCat.TaskForest.setup.policy");
    let content = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "setup policy should be readable at {}: {error}",
            path.display()
        )
    });
    for fragment in [
        "<policyconfig>",
        "io.github.YellowWhiteBlackCat.TaskForest.first-run-setup",
        "auth_admin",
        "/usr/libexec/taskforest-setup-helper",
        "</policyconfig>",
    ] {
        assert!(
            content.contains(fragment),
            "setup policy lost required fragment: {fragment}"
        );
    }
}
