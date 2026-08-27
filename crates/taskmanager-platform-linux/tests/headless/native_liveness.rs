#[test]
fn current_process_is_not_reported_as_a_gone_lock_holder() {
    assert!(!taskmanager_platform_linux::history_lock_holder_is_gone(
        std::process::id()
    ));
}

#[test]
fn impossible_linux_pid_is_reported_as_a_gone_lock_holder() {
    // Linux PID values are bounded by pid_max (and pid_t), so u32::MAX can
    // never name a live procfs task. This exercises the missing-owner branch
    // without racing a short-lived fixture process.
    assert!(taskmanager_platform_linux::history_lock_holder_is_gone(
        u32::MAX
    ));
}
