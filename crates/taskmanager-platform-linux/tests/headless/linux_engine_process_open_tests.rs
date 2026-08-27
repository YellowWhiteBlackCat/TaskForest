use super::{open_file_location, parent_dir, read_exe_path};
use std::path::Path;

/// `/usr/bin/foo` → `/usr/bin`: stripping just the file component is the
/// directory a file manager should open at. Pure — no syscall, no spawn.
#[test]
fn parent_dir_strips_the_file_component() {
    assert_eq!(
        parent_dir(Path::new("/usr/bin/foo")),
        Some(Path::new("/usr/bin").to_path_buf())
    );
}

/// `/` itself has no parent → `None`. This is the only absolute path that
/// yields `None`; the caller surfaces it as "Location unavailable".
#[test]
fn parent_dir_none_for_root() {
    assert_eq!(parent_dir(Path::new("/")), None);
}

/// A top-level file `/foo`'s parent is the root `/` — so an exe literally in
/// `/` still opens the root dir rather than reporting "unavailable".
#[test]
fn parent_dir_of_top_level_file_is_root() {
    assert_eq!(
        parent_dir(Path::new("/foo")),
        Some(Path::new("/").to_path_buf())
    );
}

/// A bare relative filename (no directory) yields an EMPTY parent per
/// `Path::parent`'s relative-path semantics (not `None`). In practice this
/// never reaches `open_file_location`: real exe paths come from
/// `/proc/<pid>/exe` (always absolute) or are `None` (kernel threads have no
/// exe), so this case is documented here only to pin the helper's contract.
#[test]
fn parent_dir_of_bare_relative_name_is_empty() {
    assert_eq!(
        parent_dir(Path::new("kworker")),
        Some(Path::new("").to_path_buf())
    );
}

#[test]
#[cfg(target_os = "linux")]
fn current_process_executable_is_resolved_from_procfs() {
    let executable = read_exe_path(std::process::id())
        .expect("the running test process must expose /proc/<pid>/exe");

    assert!(executable.is_absolute());
    assert!(executable.exists());
}

#[test]
fn opening_a_path_without_a_parent_fails_before_spawning_a_file_manager() {
    let error = open_file_location(Path::new("/"))
        .expect_err("the filesystem root has no containing directory");

    assert!(error.contains("no parent directory"));
}
