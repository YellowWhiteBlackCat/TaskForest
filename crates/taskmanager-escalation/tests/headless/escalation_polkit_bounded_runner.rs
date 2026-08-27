//! Behavioral coverage for the bounded child runner: deadline kill with the
//! typed timeout, and the per-stream read cap. These run real short-lived
//! children owned by the test itself (no privileged/interactive binaries, no
//! OS-visible side effects); when `sh`/`head`/`sleep` are absent from the
//! lane the test records the skip instead of failing (STANDARDS §3.4 rule 7).
use super::*;
use std::io;
use std::time::{Duration, Instant};

#[test]
fn hanging_child_is_killed_at_the_deadline_with_a_typed_timeout() {
    // `exec` keeps one PID: the SIGKILL reaps the sleeping process itself,
    // so the test never orphans a `sleep` on the host.
    let mut command = std::process::Command::new("sh");
    command
        .arg("-c")
        .arg("echo partial; exec sleep 1000")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let started = Instant::now();
    match run_bounded(&mut command, Duration::from_millis(300)) {
        Err(BoundedChildError::TimedOut { stdout, .. }) => {
            // The deadline fired near 300 ms, not after the child's 1000 s.
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "runner waited {} for a child it should have killed",
                started.elapsed().as_secs()
            );
            // The drain ran concurrently with the wait: output produced
            // before the deadline is preserved in the typed timeout.
            assert!(String::from_utf8_lossy(&stdout).contains("partial"));
        }
        Err(BoundedChildError::Spawn(error)) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!("skipping: sh is not installed in this lane");
        }
        other => panic!("expected a typed timeout, got {other:?}"),
    }
}

#[test]
fn oversized_stdout_is_truncated_at_the_stream_cap() {
    // 256 KiB of zeros against a 64 KiB cap: the drain must stop at the cap
    // and the run must still complete (the truncated stream simply fails to
    // parse as a contract downstream).
    let mut command = std::process::Command::new("sh");
    command
        .arg("-c")
        .arg("exec head -c 262144 /dev/zero")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    match run_bounded(&mut command, Duration::from_secs(10)) {
        Ok(output) => {
            assert_eq!(output.stdout.len(), STREAM_CAP_BYTES);
            assert!(output.stdout.iter().all(|&byte| byte == 0));
        }
        Err(BoundedChildError::Spawn(error)) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!("skipping: sh is not installed in this lane");
        }
        other => panic!("expected a capped completion, got {other:?}"),
    }
}
