use super::*;

#[test]
fn open_file_item_alias_is_identical_to_entry() {
    let item = OpenFileItem::new(1, OpenFileKind::File, Some("/etc/hosts".into()), false);
    assert_eq!(item.fd, 1);
    assert!(item.is_regular_file());
}

#[test]
fn classification_helpers_identify_sockets() {
    let entry = OpenFileEntry::new(
        3,
        OpenFileKind::Socket,
        Some("socket:[12345]".into()),
        false,
    );
    assert!(entry.is_socket());
    assert!(!entry.is_pipe());
    assert!(!entry.is_eventfd());
    assert!(!entry.is_regular_file());
    assert_eq!(entry.resolved_kind(), OpenFileKind::Socket);

    // Even with OpenFileKind::Other, target heuristic identifies socket
    let fallback = OpenFileEntry::new(4, OpenFileKind::Other, Some("socket:[999]".into()), false);
    assert!(fallback.is_socket());
    assert_eq!(fallback.resolved_kind(), OpenFileKind::Socket);
}

#[test]
fn classification_helpers_identify_pipes() {
    let entry = OpenFileEntry::new(4, OpenFileKind::Pipe, Some("pipe:[67890]".into()), false);
    assert!(entry.is_pipe());
    assert!(!entry.is_socket());
    assert!(!entry.is_eventfd());
    assert!(!entry.is_regular_file());
    assert_eq!(entry.resolved_kind(), OpenFileKind::Pipe);

    // Anonymous FIFO target heuristic
    let fifo = OpenFileEntry::new(5, OpenFileKind::Other, Some("FIFO:[123]".into()), false);
    assert!(fifo.is_pipe());
    assert_eq!(fifo.resolved_kind(), OpenFileKind::Pipe);
}

#[test]
fn classification_helpers_identify_eventfds() {
    // Direct classification
    let entry = OpenFileEntry::new(
        5,
        OpenFileKind::Eventfd,
        Some("anon_inode:[eventfd]".into()),
        false,
    );
    assert!(entry.is_eventfd());
    assert!(!entry.is_socket());
    assert!(!entry.is_pipe());
    assert!(!entry.is_regular_file());
    assert_eq!(entry.resolved_kind(), OpenFileKind::Eventfd);

    // Linux procfs coarse OpenFileKind::Other with anon_inode:[eventfd] target
    let linux_style = OpenFileEntry::new(
        6,
        OpenFileKind::Other,
        Some("anon_inode:[eventfd]".into()),
        false,
    );
    assert!(linux_style.is_eventfd());
    assert!(!linux_style.is_socket());
    assert!(!linux_style.is_pipe());
    assert!(!linux_style.is_regular_file());
    assert_eq!(linux_style.resolved_kind(), OpenFileKind::Eventfd);

    // anon_inode:eventfd variant
    let linux_colon = OpenFileEntry::new(
        7,
        OpenFileKind::Other,
        Some("anon_inode:eventfd".into()),
        false,
    );
    assert!(linux_colon.is_eventfd());
    assert_eq!(linux_colon.resolved_kind(), OpenFileKind::Eventfd);
}

#[test]
fn classification_helpers_identify_regular_files() {
    let entry = OpenFileEntry::new(
        0,
        OpenFileKind::File,
        Some("/var/log/app.log".into()),
        false,
    );
    assert!(entry.is_regular_file());
    assert!(entry.is_file());
    assert!(!entry.is_socket());
    assert!(!entry.is_pipe());
    assert!(!entry.is_eventfd());
    assert_eq!(entry.resolved_kind(), OpenFileKind::File);

    // Unlinked/deleted regular file
    let deleted = OpenFileEntry::new(
        1,
        OpenFileKind::File,
        Some("/tmp/deleted.txt (deleted)".into()),
        true,
    );
    assert!(deleted.is_regular_file());
    assert!(deleted.deleted);

    // Unreadable file descriptor (target: None)
    let unreadable = OpenFileEntry::new(2, OpenFileKind::File, None, false);
    assert!(unreadable.is_regular_file());
    assert_eq!(unreadable.target_or("[unreadable]"), "[unreadable]");
}

#[test]
fn open_file_kind_display_and_helpers() {
    assert_eq!(OpenFileKind::Socket.as_str(), "socket");
    assert_eq!(OpenFileKind::Socket.to_string(), "socket");
    assert!(OpenFileKind::Socket.is_socket());
    assert!(OpenFileKind::Pipe.is_pipe());
    assert!(OpenFileKind::Eventfd.is_eventfd());
    assert!(OpenFileKind::File.is_regular_file());
    assert!(OpenFileKind::File.is_file());
    assert!(OpenFileKind::Directory.is_directory());
    assert!(OpenFileKind::Device.is_device());
    assert!(OpenFileKind::Epoll.is_epoll());
    assert!(OpenFileKind::Signalfd.is_signalfd());
    assert!(OpenFileKind::Timerfd.is_timerfd());
    assert!(OpenFileKind::Pidfd.is_pidfd());
    assert!(OpenFileKind::Memfd.is_memfd());
    assert!(OpenFileKind::Other.is_other());
}
