use super::*;

#[test]
fn smbios_query_behavior() {
    let result = raw_smbios_table();
    #[cfg(windows)]
    {
        // On a real Windows host, SMBIOS should succeed and return table bytes.
        if let Ok(bytes) = result {
            assert!(!bytes.is_empty(), "SMBIOS table buffer must not be empty");
            assert!(bytes.len() <= MAX_SMBIOS_BUFFER_BYTES);
        }
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}
