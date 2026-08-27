use super::*;
#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
const MODE_ENV: &str = "TASKFOREST_PORTABLE_COMMAND_FIXTURE";
#[cfg(unix)]
const MARKER_ENV: &str = "TASKFOREST_PORTABLE_COMMAND_MARKER";

#[cfg(unix)]
fn wall_clock_ms() -> u64 {
    taskmanager_core::core::time::unix_millis(std::time::SystemTime::now())
}

#[cfg(unix)]
fn fixture_command(mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("current test binary"));
    command
        .args([
            "portable_command_fixture",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(MODE_ENV, mode);
    command
}

#[test]
#[cfg(unix)]
fn bounded_command_preserves_success_output_from_both_streams() {
    let mut command = fixture_command("success");
    let output = run_with_timeout(&mut command, Duration::from_secs(5)).expect("fixture success");
    assert!(output.status.success());
    assert!(
        output
            .stdout
            .windows(b"portable-stdout".len())
            .any(|bytes| bytes == b"portable-stdout")
    );
    assert!(
        output
            .stderr
            .windows(b"portable-stderr".len())
            .any(|bytes| bytes == b"portable-stderr")
    );
}

#[test]
#[cfg(unix)]
fn continuous_output_is_killed_as_output_too_large_before_timeout() {
    let mut command = fixture_command("continuous");
    let started = Instant::now();
    assert!(matches!(
        run_with_timeout(&mut command, Duration::from_secs(5)),
        Err(BoundedCommandError::OutputTooLarge)
    ));
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
#[cfg(unix)]
fn combined_stream_limit_is_enforced_not_just_each_stream_limit() {
    let mut command = fixture_command("combined");
    assert!(matches!(
        run_with_timeout(&mut command, Duration::from_secs(5)),
        Err(BoundedCommandError::OutputTooLarge)
    ));
}

#[test]
#[cfg(unix)]
fn timeout_kills_child_before_its_delayed_side_effect() {
    let marker = crate::test_support::repo_temp_dir().join(format!(
        "portable-command-timeout-{}-{}",
        std::process::id(),
        wall_clock_ms()
    ));
    let mut command = fixture_command("delayed-marker");
    command.env(MARKER_ENV, &marker);
    assert!(matches!(
        run_with_timeout(&mut command, Duration::from_millis(30)),
        Err(BoundedCommandError::TimedOut)
    ));
    std::thread::sleep(Duration::from_millis(650));
    assert!(
        !marker.exists(),
        "timed-out child survived long enough to mutate"
    );
}

#[test]
#[cfg(unix)]
fn direct_exit_cannot_leave_pipe_holding_descendants_or_side_effects() {
    let mut markers = Vec::new();
    for attempt in 0..8 {
        let marker = crate::test_support::repo_temp_dir().join(format!(
            "portable-command-descendant-{}-{attempt}-{}",
            std::process::id(),
            wall_clock_ms()
        ));
        let mut command = fixture_command("spawn-descendant");
        command.env(MARKER_ENV, &marker);
        let output = run_with_timeout(&mut command, Duration::from_secs(5))
            .expect("direct child exits after spawning owned descendant");
        assert!(output.status.success());
        markers.push(marker);
    }
    std::thread::sleep(Duration::from_millis(650));
    assert!(
        markers.iter().all(|marker| !marker.exists()),
        "a pipe-holding descendant escaped the owned process tree"
    );
}

#[test]
#[cfg(unix)]
fn escaped_session_holding_a_pipe_is_bounded_by_the_original_deadline() {
    let marker = crate::test_support::repo_temp_dir().join(format!(
        "portable-command-escaped-pipe-{}-{}",
        std::process::id(),
        wall_clock_ms()
    ));
    let mut command = fixture_command("spawn-escaped-descendant");
    command.env(MARKER_ENV, &marker);
    let started = Instant::now();
    assert!(matches!(
        run_with_timeout(&mut command, Duration::from_millis(150)),
        Err(BoundedCommandError::ReaderTimedOut)
    ));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "an escaped pipe holder must not make reader join unbounded"
    );
    std::thread::sleep(Duration::from_millis(500));
    let _ = std::fs::remove_file(marker);
}

#[test]
#[cfg(unix)]
fn portable_command_fixture() {
    let Some(mode) = std::env::var_os(MODE_ENV) else {
        return;
    };
    match mode.to_string_lossy().as_ref() {
        "success" => {
            std::io::stdout()
                .write_all(b"portable-stdout")
                .expect("stdout fixture");
            std::io::stderr()
                .write_all(b"portable-stderr")
                .expect("stderr fixture");
        }
        "continuous" => {
            let mut stdout = std::io::stdout().lock();
            let chunk = [b'x'; 8 * 1024];
            while stdout.write_all(&chunk).is_ok() {}
        }
        "combined" => {
            let chunk = vec![b'x'; 3 * 1024 * 1024];
            std::io::stdout()
                .write_all(&chunk)
                .expect("stdout combined fixture");
            let _ = std::io::stderr().write_all(&chunk);
        }
        "delayed-marker" => {
            std::thread::sleep(Duration::from_millis(500));
            let marker = PathBuf::from(std::env::var_os(MARKER_ENV).expect("marker path"));
            std::fs::write(marker, b"survived").expect("marker write");
        }
        "spawn-descendant" => {
            let marker = std::env::var_os(MARKER_ENV).expect("descendant marker path");
            let mut descendant =
                std::process::Command::new(std::env::current_exe().expect("current test binary"))
                    .args([
                        "portable_command_fixture",
                        "--nocapture",
                        "--test-threads=1",
                    ])
                    .env(MODE_ENV, "descendant-holder")
                    .env(MARKER_ENV, marker)
                    .spawn()
                    .expect("spawn pipe-holding descendant");
            assert!(
                descendant
                    .try_wait()
                    .expect("inspect pipe-holding descendant")
                    .is_none(),
                "the descendant must still hold the inherited pipes when its direct parent exits"
            );
            // This fixture intentionally exits while the descendant is live:
            // the runner owns and reaps the process tree, which is the
            // behavior under test. `try_wait` still reaps an unexpectedly
            // early exit instead of leaving an accidental zombie fixture.
        }
        "spawn-escaped-descendant" => {
            let marker = std::env::var_os(MARKER_ENV).expect("escaped readiness marker");
            let mut descendant =
                std::process::Command::new(std::env::current_exe().expect("current test binary"))
                    .args([
                        "portable_command_fixture",
                        "--nocapture",
                        "--test-threads=1",
                    ])
                    .env(MODE_ENV, "escaped-descendant-holder")
                    .env(MARKER_ENV, &marker)
                    .spawn()
                    .expect("spawn session-escaping pipe holder");
            let marker = PathBuf::from(marker);
            let deadline = Instant::now() + Duration::from_millis(100);
            while !marker.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(marker.exists(), "escaped descendant did not become ready");
            assert!(descendant.try_wait().expect("inspect descendant").is_none());
        }
        "escaped-descendant-holder" => {
            nix::unistd::setsid().expect("escape the runner process group");
            let marker = PathBuf::from(std::env::var_os(MARKER_ENV).expect("readiness marker"));
            std::fs::write(marker, b"ready").expect("publish escaped readiness");
            std::thread::sleep(Duration::from_millis(400));
        }
        "descendant-holder" => {
            std::thread::sleep(Duration::from_millis(500));
            let marker = PathBuf::from(std::env::var_os(MARKER_ENV).expect("marker path"));
            std::fs::write(marker, b"escaped").expect("descendant marker write");
        }
        mode => panic!("unknown fixture mode {mode}"),
    }
}
