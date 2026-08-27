use super::*;

fn process(pid: u32, memory: UsedGpuMemory) -> ProcessInfo {
    ProcessInfo {
        pid,
        used_gpu_memory: memory,
        gpu_instance_id: None,
        compute_instance_id: None,
    }
}

#[test]
fn compute_and_graphics_duplicates_take_max_instead_of_double_counting() {
    let mut counters = BTreeMap::new();
    merge_process_memory(
        &mut counters,
        "gpu:pci:0000:03:00.0",
        42,
        &[
            process(42, UsedGpuMemory::Used(1_024)),
            process(7, UsedGpuMemory::Used(8_192)),
        ],
    );
    merge_process_memory(
        &mut counters,
        "gpu:pci:0000:03:00.0",
        42,
        &[process(42, UsedGpuMemory::Used(2_048))],
    );

    assert_eq!(counters.get("gpu:pci:0000:03:00.0"), Some(&2_048));
}

#[test]
fn unavailable_memory_and_other_pids_never_create_a_counter() {
    let mut counters = BTreeMap::new();
    merge_process_memory(
        &mut counters,
        "gpu:pci:0000:03:00.0",
        42,
        &[
            process(42, UsedGpuMemory::Unavailable),
            process(7, UsedGpuMemory::Used(8_192)),
        ],
    );

    assert!(counters.is_empty());
}

#[test]
fn proc_start_token_must_match_the_exact_process_identity() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_nvml_process_identity_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock is after epoch")
            .as_nanos()
    ));
    let process_dir = root.join("42");
    std::fs::create_dir_all(&process_dir).expect("fixture directory");
    let stat = |start| {
        format!("42 (gpu worker) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 {start} 20")
    };
    std::fs::write(process_dir.join("stat"), stat(100)).expect("fixture stat");

    assert!(process_identity_matches(
        &root,
        ProcessIdentity {
            pid: 42,
            start_token: 100,
        }
    ));
    assert!(!process_identity_matches(
        &root,
        ProcessIdentity {
            pid: 42,
            start_token: 101,
        }
    ));

    std::fs::remove_dir_all(root).ok();
}
