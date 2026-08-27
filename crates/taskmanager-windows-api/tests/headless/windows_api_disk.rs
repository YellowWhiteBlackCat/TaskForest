use super::*;

#[cfg(windows)]
fn current_system_drive() -> String {
    std::env::var("SystemDrive").expect("Windows must expose the SystemDrive environment variable")
}

#[test]
fn live_disk_performance_query() {
    #[cfg(windows)]
    let drive = current_system_drive();
    #[cfg(not(windows))]
    let drive = String::from("C");
    let result = query_disk_performance(&drive);
    #[cfg(windows)]
    {
        if let Ok(perf) = result {
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
    #[cfg(windows)]
    let drive = current_system_drive();
    #[cfg(not(windows))]
    let drive = String::from("C");
    let result = query_disk_device_info(&drive);
    #[cfg(windows)]
    {
        match result {
            Ok(info) => {
                // Virtual disks and storage providers are allowed to omit the
                // seek-penalty fact. Unknown is an honest typed result, not a
                // reason to require a particular runner's hardware.
                assert!(
                    info.vendor_id
                        .as_deref()
                        .is_none_or(|value| !value.trim().is_empty())
                );
                assert!(
                    info.product_id
                        .as_deref()
                        .is_none_or(|value| !value.trim().is_empty())
                );
                assert!(
                    info.product_revision
                        .as_deref()
                        .is_none_or(|value| !value.trim().is_empty())
                );
                assert!(
                    info.serial_number
                        .as_deref()
                        .is_none_or(|value| !value.trim().is_empty())
                );
            }
            // A Windows host may expose a drive that accepts the query but
            // cannot provide device metadata; the API reports that honestly.
            Err(WindowsApiError::PermissionDenied | WindowsApiError::QueryFailed) => {}
            Err(error) => panic!("unexpected disk device query result: {error:?}"),
        }
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}

#[test]
fn live_disk_smart_info_query() {
    #[cfg(windows)]
    let drive = current_system_drive();
    #[cfg(not(windows))]
    let drive = String::from("C");
    let result = query_disk_smart_info(&drive);
    #[cfg(windows)]
    {
        if let Ok(smart) = result
            && let Some(t) = smart.temperature_c
        {
            assert!(t > 0.0 && t < 120.0);
        }
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}
