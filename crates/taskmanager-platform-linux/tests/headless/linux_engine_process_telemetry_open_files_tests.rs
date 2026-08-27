use super::*;
use taskmanager_core::core::device_state::DeviceState;

#[test]
fn classify_covers_socket_pipe_anon_and_paths() {
    assert_eq!(
        classify_open_file_target("socket:[4242]"),
        OpenFileKind::Socket
    );
    assert_eq!(
        classify_open_file_target("pipe:[12345]"),
        OpenFileKind::Pipe
    );
    assert_eq!(
        classify_open_file_target("anon_inode:eventfd"),
        OpenFileKind::Other
    );
    assert_eq!(classify_open_file_target("/dev/null"), OpenFileKind::File);
    assert_eq!(
        classify_open_file_target("/home/<user>/logs/app.log"),
        OpenFileKind::File
    );
    assert_eq!(
        classify_open_file_target("/run/taskmanager.sock"),
        OpenFileKind::File
    );
    assert_eq!(classify_open_file_target(""), OpenFileKind::File);
}

#[cfg(target_os = "linux")]
#[test]
fn collect_from_proc_dir_classifies_each_descriptor_and_preserves_order() {
    use std::os::unix::fs::symlink;
    let root = crate::test_support::repo_temp_dir()
        .join(format!("taskmanager-open-files-{}", std::process::id()));
    let fd_dir = root.join("fd");
    std::fs::create_dir_all(&fd_dir).expect("create fd fixture");
    // Deliberately create descriptors out of fd order to prove sorting.
    symlink("/var/log/app.log", fd_dir.join("7")).expect("ln file");
    symlink("socket:[4242]", fd_dir.join("3")).expect("ln socket");
    symlink("pipe:[12345]", fd_dir.join("4")).expect("ln pipe");
    symlink("anon_inode:eventfd", fd_dir.join("10")).expect("ln anon");

    let facet = collect_open_files_from_proc_dir(&root, 1_000);

    assert_eq!(facet.state, DeviceState::healthy(1_000));
    assert_eq!(facet.unreadable_count, 0);
    let fds: Vec<u32> = facet.entries.iter().map(|entry| entry.fd).collect();
    assert_eq!(fds, vec![3, 4, 7, 10], "entries are sorted by fd");
    assert_eq!(facet.entries[0].kind, OpenFileKind::Socket);
    assert_eq!(facet.entries[1].kind, OpenFileKind::Pipe);
    assert_eq!(facet.entries[2].kind, OpenFileKind::File);
    assert_eq!(facet.entries[2].target.as_deref(), Some("/var/log/app.log"));
    assert_eq!(facet.entries[3].kind, OpenFileKind::Other);

    // A missing fd directory (vanished process) is a typed Stale state.
    std::fs::remove_dir_all(&root).expect("remove fixture");
    let stale = collect_open_files_from_proc_dir(&root, 2_000);
    assert_eq!(stale.state.status, DeviceStatus::Stale);
    assert!(stale.entries.is_empty());
}
