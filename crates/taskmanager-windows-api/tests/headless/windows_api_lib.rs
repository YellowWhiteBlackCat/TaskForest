use super::*;

#[test]
fn locale_decoder_rejects_invalid_external_lengths_without_panicking() {
    let buffer = [b'e' as u16, b'n' as u16, 0];
    assert_eq!(
        decode_locale_name(&buffer, -1),
        Err(WindowsApiError::QueryFailed)
    );
    assert_eq!(
        decode_locale_name(&buffer, 0),
        Err(WindowsApiError::QueryFailed)
    );
    assert_eq!(
        decode_locale_name(&buffer, 4),
        Err(WindowsApiError::QueryFailed)
    );
    assert_eq!(decode_locale_name(&buffer, 3), Ok("en".to_owned()));
}

#[cfg(not(windows))]
#[test]
fn native_queries_are_typed_unsupported_off_windows() {
    assert_eq!(system_performance(), Err(WindowsApiError::Unsupported));
    assert_eq!(user_locale_name(), Err(WindowsApiError::Unsupported));
    assert_eq!(
        known_folder_path(KnownFolder::RoamingAppData),
        Err(WindowsApiError::Unsupported)
    );
    assert_eq!(
        process_creation_time_100ns(1),
        Err(WindowsApiError::Unsupported)
    );
    assert_eq!(enumerate_sessions(), Err(WindowsApiError::Unsupported));
    assert_eq!(logoff_session(1), Err(WindowsApiError::Unsupported));
    assert_eq!(processor_topology(), Err(WindowsApiError::Unsupported));
    assert_eq!(
        enumerate_network_adapters(),
        Err(WindowsApiError::Unsupported)
    );
    assert_eq!(
        set_service_start_mode("Spooler", ServiceStartMode::Automatic),
        Err(WindowsApiError::Unsupported)
    );
    assert_eq!(
        query_gpu_engine_utilization(),
        Err(WindowsApiError::Unsupported)
    );
    assert_eq!(
        query_cpu_dynamic_frequencies(),
        Err(WindowsApiError::Unsupported)
    );
    assert_eq!(
        extract_process_icon_bmp(""),
        Err(WindowsApiError::Unsupported)
    );
}

#[cfg(windows)]
#[test]
fn service_name_encoding_is_bounded_and_nul_terminated() {
    assert_eq!(
        encode_service_name("Spooler")
            .expect("short service names fit")
            .last(),
        Some(&0)
    );
    assert_eq!(
        encode_service_name("").unwrap_err(),
        WindowsApiError::InvalidInput
    );
    assert_eq!(
        encode_service_name(&format!("bad{}name", char::from(0_u8))).unwrap_err(),
        WindowsApiError::InvalidInput
    );
    assert_eq!(
        encode_service_name(&"x".repeat(MAX_SERVICE_NAME_UTF16 + 1)).unwrap_err(),
        WindowsApiError::InvalidInput
    );
}
