//! Fire-and-forget launch reaping regressions.
//!
//! `xdg_open` / `open_file_location` cannot be exercised here (they launch
//! real desktop binaries); they share the one-shot reaper with `run`, whose
//! child is a plain `sh -c` and therefore safe to observe.

use super::*;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
#[test]
fn fire_and_forget_launch_is_reaped_after_exit_instead_of_becoming_a_zombie() {
    let pid = ProcessManager::run("exit 0").expect("sh -c 'exit 0' must spawn");
    let stat_path = format!("/proc/{pid}/stat");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut zombie_since = None;
    loop {
        match std::fs::read_to_string(&stat_path) {
            // The /proc entry is gone: the one-shot reaper waited for the
            // child and the kernel reaped it.
            Err(_) => return,
            Ok(stat) => {
                // After the last ')' the first whitespace field is the state.
                let state = stat
                    .rsplit(')')
                    .next()
                    .and_then(|rest| rest.split_whitespace().next())
                    .unwrap_or_default();
                // A transient zombie window while the reaper thread gets
                // scheduled is fine; a persistent zombie is the regression.
                if state == "Z" && zombie_since.is_none() {
                    zombie_since = Some(Instant::now());
                }
                if zombie_since.is_some_and(|since| since.elapsed() > Duration::from_secs(2)) {
                    panic!("launched child {pid} lingered as a zombie after exit");
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "launched child {pid} was never reaped"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn run_rejects_blank_commands_without_spawning() {
    assert!(ProcessManager::run("   ").is_err());
    assert!(ProcessManager::run("").is_err());
}
