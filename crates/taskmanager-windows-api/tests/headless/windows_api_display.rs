use super::*;

#[test]
fn display_monitor_enumeration_stays_bounded_and_typed() {
    let result = enumerate_display_monitors();
    #[cfg(windows)]
    {
        if let Ok(monitors) = result {
            assert!(
                monitors.len() <= 16 * 8,
                "enumeration must respect its bounds"
            );
            for monitor in &monitors {
                assert!(!monitor.device_name.is_empty());
                if let Some(edid) = &monitor.edid {
                    assert!(edid.len() >= 128, "returned EDID must be plausibly sized");
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}
