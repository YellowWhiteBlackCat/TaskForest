use super::*;

#[test]
fn gpu_and_npu_adapter_enumeration() {
    let result = enumerate_gpu_adapters();
    #[cfg(windows)]
    {
        if let Ok(inventory) = result {
            for adapter in &inventory.adapters {
                eprintln!("LIVE GPU/NPU ADAPTER: {adapter:?}");
                assert!(!adapter.name.is_empty());
            }
            let has_intel_arc = inventory
                .adapters
                .iter()
                .any(|a| a.name.contains("Intel(R) Arc"));
            let has_npu = inventory.adapters.iter().any(|a| a.is_npu);
            eprintln!("Has Intel Arc: {has_intel_arc}, Has NPU: {has_npu}");
            assert!(has_intel_arc);
            assert!(has_npu);
        }
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}
