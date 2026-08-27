//! Unit and integration tests for Windows process APIs.

use super::*;

#[test]
fn current_process_has_a_nonzero_kernel_creation_time() {
    let time = process_creation_time_100ns(std::process::id())
        .expect("current process creation time should be queryable");
    assert_ne!(time, 0);
}

#[test]
fn current_process_priority_and_elevation() {
    let _priority =
        process_priority(std::process::id()).expect("current process priority should be queryable");
    // CI launchers may intentionally lower the test process priority. The
    // contract is a typed native query, not a claim about the runner's policy.

    let _elevated = process_is_elevated(std::process::id())
        .expect("current process elevation should be queryable");
}

#[test]
fn mismatched_creation_time_is_rejected_before_termination() {
    let actual = process_creation_time_100ns(std::process::id())
        .expect("current process creation time should be queryable");
    assert_eq!(
        terminate_process_exact(std::process::id(), actual.saturating_add(1)),
        Err(WindowsApiError::IdentityChanged)
    );
}

#[test]
fn mismatched_creation_time_cannot_change_priority_or_affinity() {
    let pid = std::process::id();
    let actual = process_creation_time_100ns(pid).expect("current process creation time");
    let wrong = actual.saturating_add(1);

    let priority_before = process_priority(pid).expect("priority before rejected mutation");
    let requested_priority = match priority_before {
        ProcessPriorityClass::BelowNormal => ProcessPriorityClass::AboveNormal,
        _ => ProcessPriorityClass::BelowNormal,
    };
    assert_eq!(
        set_process_priority_exact(pid, wrong, requested_priority),
        Err(WindowsApiError::IdentityChanged)
    );
    assert_eq!(
        process_priority(pid).expect("priority after rejected mutation"),
        priority_before,
        "a wrong creation token must be rejected before SetPriorityClass"
    );

    let affinity_before = process_affinity(pid).expect("affinity before rejected mutation");
    let requested_affinity = affinity_before
        .last()
        .copied()
        .map(|cpu| vec![cpu])
        .expect("the current process must have at least one active processor");
    assert_eq!(
        set_process_affinity_exact(pid, wrong, &requested_affinity),
        Err(WindowsApiError::IdentityChanged)
    );
    assert_eq!(
        process_affinity(pid).expect("affinity after rejected mutation"),
        affinity_before,
        "a wrong creation token must be rejected before SetProcessAffinityMask"
    );
}

#[test]
fn current_process_affinity_and_threads() {
    let cpus =
        process_affinity(std::process::id()).expect("current process affinity should be queryable");
    assert!(!cpus.is_empty(), "affinity should include at least 1 core");

    let threads =
        process_threads(std::process::id()).expect("current process threads should be queryable");
    assert!(
        !threads.is_empty(),
        "thread list should contain at least 1 thread"
    );
}

#[test]
fn process_affinity_mutation_and_rollback() {
    let pid = std::process::id();
    let creation_time = process_creation_time_100ns(pid).expect("creation time");
    let original_affinity = process_affinity(pid).expect("original affinity");

    assert_eq!(
        set_process_affinity_exact(pid, creation_time, &[]),
        Err(WindowsApiError::InvalidInput),
        "empty affinity mask must be rejected"
    );

    if original_affinity.len() >= 2 {
        // Set affinity to just core 0
        set_process_affinity_exact(pid, creation_time, &[0]).expect("set affinity to core 0");
        let modified = process_affinity(pid).expect("modified affinity");
        assert_eq!(modified, vec![0]);

        // Revert / Rollback to original affinity ("管杀还管埋")
        set_process_affinity_exact(pid, creation_time, &original_affinity)
            .expect("rollback to original affinity");
        let restored = process_affinity(pid).expect("restored affinity");
        assert_eq!(restored, original_affinity);
    }
}

#[test]
fn stress_loop_has_zero_handle_leak() {
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

    let pid = std::process::id();
    // Warmup caches
    let _ = process_affinity(pid);
    let _ = process_threads(pid);

    let mut baseline_handles = 0u32;
    let ok = {
        // SAFETY: GetCurrentProcess returns a pseudo-handle for the current process, baseline_handles is valid.
        unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut baseline_handles) }
    }
    .is_ok();
    assert!(ok && baseline_handles > 0);

    // 1,000 iterations of rapid telemetry querying across audited APIs
    for _ in 0..1000 {
        let _ = process_affinity(pid);
        let _ = process_threads(pid);
        let _ = process_creation_time_100ns(pid);
        let _ = process_priority(pid);
        let _ = process_is_elevated(pid);
        let _ = process_isolation(pid);
        let _ = process_modules(pid);
    }

    let mut final_handles = 0u32;
    let ok = {
        // SAFETY: GetCurrentProcess returns a pseudo-handle for the current process, final_handles is valid.
        unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut final_handles) }
    }
    .is_ok();
    assert!(ok);
    assert!(
        final_handles <= baseline_handles + 2,
        "handle count must not leak after 1000 iterations (baseline: {baseline_handles}, final: {final_handles})"
    );
}

#[test]
fn live_process_isolation_query() {
    let pid = std::process::id();
    let result = process_isolation(pid);
    #[cfg(windows)]
    {
        let isolation = result.expect("current process isolation context");
        eprintln!("LIVE PROCESS ISOLATION: {isolation:?}");
        assert!(isolation.integrity_level.is_some());
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}

#[test]
fn live_process_modules_query() {
    let pid = std::process::id();
    let result = process_modules(pid);
    #[cfg(windows)]
    {
        let modules = result.expect("current process loaded modules");
        eprintln!("LIVE PROCESS MODULES COUNT: {}", modules.len());
        if let Some(first) = modules.first() {
            eprintln!("SAMPLE MODULE: {first:?}");
            assert!(!first.module_name.is_empty());
            assert!(!first.file_path.is_empty());
        }
        assert!(!modules.is_empty());
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}

#[test]
fn live_process_memory_counters_query() {
    let pid = std::process::id();
    let mem = process_memory_counters(pid).expect("current process memory counters");
    eprintln!("LIVE PROCESS MEMORY: {mem:?}");
    assert!(mem.working_set_size_bytes > 0);
    assert!(mem.pagefile_usage_bytes > 0);
}

#[test]
fn live_process_handle_count_query() {
    let pid = std::process::id();
    let handles = process_handle_count(pid).expect("current process handle count");
    eprintln!("LIVE PROCESS HANDLES: {handles}");
    assert!(handles > 0);
}

#[test]
fn live_process_gui_resources_query() {
    let pid = std::process::id();
    let gui = process_gui_resources(pid).expect("current process GUI resources");
    eprintln!("LIVE PROCESS GUI RESOURCES: {gui:?}");
    // Command line test runners may have 0 or small GDI/USER objects, but the query succeeds.
}

#[test]
fn live_enumerate_all_process_thread_counts() {
    let counts =
        enumerate_all_process_thread_counts().expect("enumerate all process thread counts");
    let pid = std::process::id();
    let current_threads = counts.get(&pid).copied().unwrap_or(0);
    eprintln!("CURRENT PROCESS THREAD COUNT FROM SNAPSHOT: {current_threads}");
    assert!(current_threads > 0);
    assert!(!counts.is_empty());
}

#[test]
fn live_query_process_user() {
    let pid = std::process::id();
    let user = query_process_user(pid).expect("current process user");
    eprintln!("CURRENT PROCESS USER: {user}");
    assert!(!user.is_empty());
}
