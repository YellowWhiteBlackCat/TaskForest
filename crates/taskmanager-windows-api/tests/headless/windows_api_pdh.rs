use super::cpu::{
    parse_engine_type_from_instance_name, parse_luid_from_instance_name,
    parse_pid_from_instance_name, per_core_current_mhz, total_frequency_from_counters,
};
#[cfg(windows)]
use super::{
    query_cpu_dynamic_frequencies, query_gpu_adapter_memory, query_gpu_engine_instances,
    query_gpu_engine_utilization, query_gpu_process_memory,
};

#[test]
fn engine_instance_names_parse_into_pid_luid_and_engine_type() {
    let name = "pid_1234_luid_0x00000000_0x0000ABCD_phys_0_eng_0_engtype_3D";
    assert_eq!(parse_pid_from_instance_name(name), Some(1234));
    assert_eq!(parse_luid_from_instance_name(name), Some(0xABCD));
    assert_eq!(parse_engine_type_from_instance_name(name), Some("3D"));
}

#[test]
fn engine_type_tokens_map_to_display_labels_and_keep_unknowns_verbatim() {
    let cases = [
        ("Copy", "Copy"),
        ("Compute", "Compute"),
        ("VideoDecode", "Video Decode"),
        ("VideoEncode", "Video Encode"),
        ("VideoProcessing", "Video Processing"),
        // The NPU marketing token normalizes onto the Neural label.
        ("NPU", "Neural"),
        ("Neural", "Neural"),
        // A future driver token must survive verbatim, not collapse to a guess.
        ("Overlay", "Overlay"),
    ];
    for (token, expected) in cases {
        let instance = format!("pid_7_luid_0x00000000_0x000000C1_phys_0_eng_3_engtype_{token}");
        assert_eq!(
            parse_engine_type_from_instance_name(&instance),
            Some(expected)
        );
    }
    assert_eq!(
        parse_engine_type_from_instance_name("pid_7_phys_0_eng_0"),
        None
    );
}

#[test]
fn process_memory_instance_names_carry_pid_and_luid() {
    let name = "pid_4212_luid_0x00000000_0x0000D3A0";
    assert_eq!(parse_pid_from_instance_name(name), Some(4212));
    assert_eq!(parse_luid_from_instance_name(name), Some(0xD3A0));
    assert_eq!(parse_pid_from_instance_name("_Total"), None);
    assert_eq!(parse_luid_from_instance_name("_Total"), None);
}

#[test]
fn per_core_frequency_uses_base_times_performance_like_task_manager() {
    assert_eq!(
        per_core_current_mhz(Some(1900), Some(135.0), Some(1900.0)),
        Some(2565)
    );
    assert_eq!(
        per_core_current_mhz(Some(1500), Some(121.5), Some(1500.0)),
        Some(1823)
    );
    // No performance ratio: the live `Processor Frequency` counter wins.
    assert_eq!(
        per_core_current_mhz(Some(1900), None, Some(2100.0)),
        Some(2100)
    );
    assert_eq!(per_core_current_mhz(None, None, Some(2100.0)), Some(2100));
    assert_eq!(per_core_current_mhz(None, None, None), None);
}

#[test]
fn total_frequency_falls_back_honestly() {
    assert_eq!(
        total_frequency_from_counters(Some(1900), Some(121.0), Some(2200.0)),
        Some(2299)
    );
    assert_eq!(
        total_frequency_from_counters(None, None, Some(2200.0)),
        Some(2200)
    );
    assert_eq!(total_frequency_from_counters(None, None, None), None);
}

#[test]
fn live_pdh_queries() {
    #[cfg(windows)]
    {
        let gpu_res = query_gpu_engine_utilization();
        eprintln!("LIVE GPU ENGINE UTILIZATION: {gpu_res:?}");
        assert!(gpu_res.is_ok());

        let engine_instances = query_gpu_engine_instances();
        eprintln!("LIVE GPU ENGINE INSTANCES: {engine_instances:?}");
        assert!(engine_instances.is_ok());

        let process_memory = query_gpu_process_memory();
        eprintln!("LIVE GPU PROCESS MEMORY: {process_memory:?}");
        assert!(process_memory.is_ok());

        let memory_res = query_gpu_adapter_memory();
        eprintln!("LIVE GPU ADAPTER MEMORY: {memory_res:?}");
        assert!(memory_res.is_ok());

        let cpu_res = query_cpu_dynamic_frequencies();
        eprintln!("LIVE DYNAMIC CPU FREQUENCY RESULT: {cpu_res:?}");
        assert!(cpu_res.is_ok());
        let sample = cpu_res.unwrap();
        eprintln!("  Total dynamic MHz: {:?}", sample.total_frequency_mhz);
        eprintln!(
            "  Per core dynamic MHz: {:?}",
            sample.per_core_frequency_mhz
        );
        assert!(sample.total_frequency_mhz.is_some());
        assert!(!sample.per_core_frequency_mhz.is_empty());
    }
}
