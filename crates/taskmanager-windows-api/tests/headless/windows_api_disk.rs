use super::*;

#[test]
fn live_disk_performance_query() {
    let result = query_disk_performance("C");
    #[cfg(windows)]
    {
        if let Ok(perf) = result {
            eprintln!("LIVE C: DISK PERFORMANCE: {perf:?}");
            assert!(perf.query_time_100ns > 0);
        }
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}

#[test]
fn live_disk_device_info_query() {
    let result = query_disk_device_info("C");
    #[cfg(windows)]
    {
        if let Ok(info) = result {
            eprintln!("LIVE C: DISK DEVICE INFO: {info:?}");
            assert_ne!(info.media_type, WindowsDiskMediaType::Unknown);
        }
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}

#[test]
fn live_disk_smart_info_query() {
    let result = query_disk_smart_info("C");
    #[cfg(windows)]
    {
        if let Ok(smart) = result {
            eprintln!("LIVE C: DISK SMART INFO: {smart:?}");
            if let Some(t) = smart.temperature_c {
                assert!(t > 0.0 && t < 120.0);
            }
        }
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}
