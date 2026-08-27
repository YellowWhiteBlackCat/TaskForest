use super::*;
#[cfg(unix)]
use std::cell::Cell;
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
struct TemporaryRulePath {
    directory: PathBuf,
    path: PathBuf,
}

#[cfg(unix)]
impl TemporaryRulePath {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let directory = crate::test_support::repo_temp_dir().join(format!(
            "taskmanager-setup-helper-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap_or_else(|error| {
            panic!(
                "could not create test directory {}: {error}",
                directory.display()
            )
        });
        let path = directory.join("99-taskmanager.rules");
        Self { directory, path }
    }
}

#[cfg(unix)]
impl Drop for TemporaryRulePath {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn parser_accepts_only_the_two_fixed_actions() {
    assert_eq!(Operation::parse(Some("install")), Ok(Operation::Install));
    assert_eq!(Operation::parse(Some("revert")), Ok(Operation::Revert));
    assert!(matches!(
        Operation::parse(Some("sh -c whoami")),
        Err(HelperError::InvalidArgument(_))
    ));
    assert!(matches!(
        Operation::parse(None),
        Err(HelperError::InvalidArgument(_))
    ));
}

#[test]
fn json_escape_keeps_error_envelope_valid() {
    assert_eq!(
        json_escape("quote=\" slash=\\ line=\n"),
        "quote=\\\" slash=\\\\ line=\\n"
    );
}

#[test]
fn only_exact_embedded_rule_content_is_safe_to_replace() {
    assert!(!RULE_CONTENT.is_empty());
    assert_eq!(
        HelperError::Conflict("x".to_owned()).exit_code(),
        EXIT_CONFLICT
    );
    assert_ne!(RULE_CONTENT.as_bytes(), b"arbitrary caller input");
}

#[cfg(unix)]
#[test]
fn install_writes_exact_rule_and_reloads_after_the_atomic_publish() {
    let temporary = TemporaryRulePath::new();
    let reloads = Cell::new(0);

    let changed = install_with(&temporary.path, || {
        assert_eq!(
            fs::read(&temporary.path).ok(),
            Some(RULE_CONTENT.as_bytes().to_vec())
        );
        reloads.set(reloads.get() + 1);
        Ok(())
    })
    .unwrap_or_else(|error| panic!("install should succeed: {error:?}"));

    assert!(changed);
    assert_eq!(reloads.get(), 1);
    assert_eq!(
        fs::read(&temporary.path).ok(),
        Some(RULE_CONTENT.as_bytes().to_vec())
    );
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(&temporary.path)
            .unwrap_or_else(|error| panic!("installed rule metadata missing: {error}"))
            .permissions()
            .mode()
            & 0o777,
        0o644
    );
}

#[cfg(unix)]
#[test]
fn install_is_idempotent_and_does_not_reload_an_unchanged_rule() {
    let temporary = TemporaryRulePath::new();
    fs::write(&temporary.path, RULE_CONTENT).unwrap_or_else(|error| {
        panic!(
            "could not seed rule fixture {}: {error}",
            temporary.path.display()
        )
    });
    let reloads = Cell::new(0);

    let changed = install_with(&temporary.path, || {
        reloads.set(reloads.get() + 1);
        Ok(())
    })
    .unwrap_or_else(|error| panic!("identical install should succeed: {error:?}"));

    assert!(!changed);
    assert_eq!(reloads.get(), 0);
    assert_eq!(
        fs::read(&temporary.path).ok(),
        Some(RULE_CONTENT.as_bytes().to_vec())
    );
}

#[cfg(unix)]
#[test]
fn install_rejects_conflicting_content_without_overwriting_it() {
    let temporary = TemporaryRulePath::new();
    let original = b"an administrator-owned rule\n";
    fs::write(&temporary.path, original)
        .unwrap_or_else(|error| panic!("could not seed conflict fixture: {error}"));
    let result = install_with(&temporary.path, || Ok(()));

    assert!(matches!(result, Err(HelperError::Conflict(_))));
    assert_eq!(fs::read(&temporary.path).ok(), Some(original.to_vec()));
}

#[cfg(unix)]
#[test]
fn atomic_publish_refuses_a_target_that_appears_before_publish() {
    let temporary = TemporaryRulePath::new();
    let original = b"administrator-owned race winner\n";
    fs::write(&temporary.path, original)
        .unwrap_or_else(|error| panic!("could not seed publish fixture: {error}"));

    let result = atomic_write(&temporary.path);

    assert!(matches!(result, Err(HelperError::Conflict(_))));
    assert_eq!(fs::read(&temporary.path).ok(), Some(original.to_vec()));
    // No temporary file may be left behind, whatever random suffix it had.
    assert!(temporary_residue(&temporary.directory).is_empty());
}

/// Every file in `directory` whose name looks like one of this helper's
/// scratch temporaries (`.99-taskmanager.rules.taskforest-*`).
#[cfg(unix)]
fn temporary_residue(directory: &Path) -> Vec<String> {
    let mut residue = Vec::new();
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("could not scan {}: {error}", directory.display()));
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str()
            && name.starts_with(".99-taskmanager.rules.taskforest-")
        {
            residue.push(name.to_owned());
        }
    }
    residue
}

#[cfg(unix)]
#[test]
fn temporary_paths_are_unique_so_a_kill_residue_cannot_block_the_next_run() {
    // Two attempts must never derive the same scratch name; the suffix comes
    // from /dev/urandom (or a clock fallback), never a fixed value.
    let rule = Path::new("/etc/udev/rules.d/99-taskmanager.rules");
    let first = temporary_path(rule);
    let second = temporary_path(rule);
    assert_ne!(first, second);
    for path in [&first, &second] {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        assert!(
            name.starts_with(".99-taskmanager.rules.taskforest-"),
            "unrecognizable temporary name: {name}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_stale_temporary_file_never_blocks_a_later_install() {
    // The defect: the old fixed name `.rule.taskforest-<pid>` collided with
    // a `kill -9` residue from a previous run, so `create_new` failed and
    // every later install was blocked until an admin cleaned up by hand.
    let temporary = TemporaryRulePath::new();
    let stale_name = format!(".99-taskmanager.rules.taskforest-{}", std::process::id());
    let stale = temporary.directory.join(&stale_name);
    fs::write(&stale, b"stale residue from a killed run\n").unwrap_or_else(|error| {
        panic!("could not seed stale residue {}: {error}", stale.display())
    });

    let reloads = Cell::new(0);
    let changed = install_with(&temporary.path, || {
        reloads.set(reloads.get() + 1);
        Ok(())
    })
    .unwrap_or_else(|error| panic!("stale residue must not block install: {error:?}"));

    assert!(changed);
    assert_eq!(reloads.get(), 1);
    assert_eq!(
        fs::read(&temporary.path).ok(),
        Some(RULE_CONTENT.as_bytes().to_vec())
    );
    // Only the fresh install's own temporary is cleaned up; the pre-existing
    // residue is left untouched for the admin, exactly as before.
    assert_eq!(
        temporary_residue(&temporary.directory).as_slice(),
        [stale_name.as_str()],
        "install must clean only its own scratch file"
    );
}

#[cfg(unix)]
#[test]
fn install_rolls_back_the_new_file_when_reload_fails() {
    let temporary = TemporaryRulePath::new();
    let result = install_with(&temporary.path, || {
        Err(HelperError::MissingDependency(
            "test udev is unavailable".to_owned(),
        ))
    });

    assert!(matches!(
        result,
        Err(HelperError::MissingDependency(detail)) if detail == "test udev is unavailable"
    ));
    assert!(!temporary.path.exists());
}

#[cfg(unix)]
#[test]
fn revert_removes_only_the_exact_rule_and_reloads() {
    let temporary = TemporaryRulePath::new();
    fs::write(&temporary.path, RULE_CONTENT).unwrap_or_else(|error| {
        panic!(
            "could not seed revert fixture {}: {error}",
            temporary.path.display()
        )
    });
    let reloads = Cell::new(0);

    let changed = revert_with(&temporary.path, || {
        assert!(!temporary.path.exists());
        reloads.set(reloads.get() + 1);
        Ok(())
    })
    .unwrap_or_else(|error| panic!("revert should succeed: {error:?}"));

    assert!(changed);
    assert_eq!(reloads.get(), 1);
    assert!(!temporary.path.exists());
}

#[cfg(unix)]
#[test]
fn revert_restores_the_rule_when_reload_fails() {
    let temporary = TemporaryRulePath::new();
    fs::write(&temporary.path, RULE_CONTENT).unwrap_or_else(|error| {
        panic!(
            "could not seed rollback fixture {}: {error}",
            temporary.path.display()
        )
    });
    let result = revert_with(&temporary.path, || {
        Err(HelperError::Io("test reload failure".to_owned()))
    });

    assert!(matches!(result, Err(HelperError::Io(detail)) if detail == "test reload failure"));
    assert_eq!(
        fs::read(&temporary.path).ok(),
        Some(RULE_CONTENT.as_bytes().to_vec())
    );
}

#[cfg(unix)]
#[test]
fn revert_is_noop_for_missing_rule_and_preserves_conflicts() {
    let missing = TemporaryRulePath::new();
    let reloads = Cell::new(0);
    assert_eq!(
        revert_with(&missing.path, || {
            reloads.set(reloads.get() + 1);
            Ok(())
        }),
        Ok(false)
    );
    assert_eq!(reloads.get(), 0);

    let conflict = TemporaryRulePath::new();
    let original = b"another administrator-owned rule\n";
    fs::write(&conflict.path, original)
        .unwrap_or_else(|error| panic!("could not seed conflict fixture: {error}"));
    assert!(matches!(
        revert_with(&conflict.path, || Ok(())),
        Err(HelperError::Conflict(_))
    ));
    assert_eq!(fs::read(&conflict.path).ok(), Some(original.to_vec()));
}

// --- bounded udevadm runner ------------------------------------------------

#[cfg(unix)]
#[test]
fn a_stuck_child_is_killed_at_the_deadline_with_a_typed_timeout() {
    // Own short-lived child only: no privileged or interactive binaries and
    // no OS-visible side effects; the `sleep` is killed by the runner itself.
    let mut command = Command::new("sleep");
    command
        .arg("1000")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    match run_bounded(&mut command, Duration::from_millis(200)) {
        Err(BoundedChildError::TimedOut { .. }) => {}
        Err(BoundedChildError::Spawn(error)) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!("skipping: sleep is not installed in this lane");
        }
        other => panic!("expected a typed timeout, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn a_runaway_stderr_stream_is_capped() {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg("exec head -c 262144 /dev/zero >&2")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    match run_bounded(&mut command, Duration::from_secs(10)) {
        Ok(output) => assert_eq!(output.stderr.len(), STREAM_CAP_BYTES),
        Err(BoundedChildError::Spawn(error)) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!("skipping: sh is not installed in this lane");
        }
        other => panic!("expected a capped completion, got {other:?}"),
    }
}
