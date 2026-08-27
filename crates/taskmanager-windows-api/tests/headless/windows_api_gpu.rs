use super::*;

#[test]
fn gpu_and_npu_adapter_enumeration() {
    let result = enumerate_gpu_adapters();
    #[cfg(windows)]
    {
        let inventory = result.expect("DXGI adapter enumeration should be queryable");
        assert!(inventory.adapters.len() <= MAX_GPU_ADAPTERS as usize);
        for adapter in &inventory.adapters {
            // The runner may expose a physical GPU, a virtual adapter, or
            // only the Microsoft Basic Render Driver. Hardware vendor and NPU
            // presence are runtime capabilities, not test preconditions.
            assert!(!adapter.name.trim().is_empty());
        }
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}
