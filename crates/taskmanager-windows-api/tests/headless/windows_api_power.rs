use super::*;

#[test]
fn live_power_scheme_name_query() {
    let result = active_power_scheme_name();
    #[cfg(windows)]
    {
        let name = result.expect("active power scheme name");
        eprintln!("LIVE POWER SCHEME: {name}");
        assert!(!name.is_empty());
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}

#[test]
fn live_processor_power_info_query() {
    let result = query_processor_power_information();
    #[cfg(windows)]
    {
        let infos = result.expect("processor power information");
        eprintln!("LIVE PROCESSOR POWER INFOS: (count = {})", infos.len());
        if let Some(first) = infos.first() {
            eprintln!("SAMPLE CORE POWER: {first:?}");
        }
        assert!(!infos.is_empty());
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}

#[test]
fn live_system_power_status_query() {
    let result = query_system_power_status();
    #[cfg(windows)]
    {
        let status = result.expect("system power status");
        eprintln!("LIVE SYSTEM POWER STATUS: {status:?}");
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}

#[test]
fn overlay_guids_map_to_stable_slider_labels_only_when_documented() {
    // The documented performance-slider GUIDs (learn.microsoft.com,
    // "Customize the Windows performance power slider").
    assert_eq!(
        power_overlay_label(
            0x961c_c777,
            0x2547,
            0x4f9d,
            [0x81, 0x74, 0x7d, 0x86, 0x18, 0x1b, 0x8a, 0x7a]
        ),
        Some("better battery")
    );
    assert_eq!(
        power_overlay_label(
            0x3af9_b8d9,
            0x7c97,
            0x431d,
            [0xad, 0x78, 0x34, 0xa8, 0xbf, 0xea, 0x43, 0x9f]
        ),
        Some("better performance")
    );
    // Balanced personality and the "no overlay" GUID are the same default
    // slider position.
    assert_eq!(
        power_overlay_label(
            0x381b_4222,
            0xf694,
            0x41f0,
            [0x96, 0x85, 0xff, 0x5b, 0xb2, 0x60, 0xdf, 0x2e]
        ),
        Some("better performance")
    );
    assert_eq!(
        power_overlay_label(0, 0, 0, [0, 0, 0, 0, 0, 0, 0, 0]),
        Some("better performance")
    );
    assert_eq!(
        power_overlay_label(
            0xded5_74b5,
            0x45a0,
            0x4f42,
            [0x87, 0x37, 0x46, 0x34, 0x5c, 0x09, 0xc2, 0x38]
        ),
        Some("best performance")
    );
    // Unknown/OEM overlays stay typed absence, never a guess.
    assert_eq!(
        power_overlay_label(0x1234_5678, 0x9abc, 0xdef0, [1, 2, 3, 4, 5, 6, 7, 8]),
        None
    );
}

#[test]
fn effective_power_overlay_query_is_dormant_off_windows() {
    let result = effective_power_overlay_name();
    #[cfg(windows)]
    {
        match result.expect("effective power overlay query") {
            Some(label) => {
                eprintln!("LIVE POWER OVERLAY: {label}");
                assert!(
                    ["better battery", "better performance", "best performance"]
                        .contains(&label.as_str())
                );
            }
            // An OEM/unknown overlay is honest absence, not a failure.
            None => eprintln!("LIVE POWER OVERLAY: unmapped GUID"),
        }
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}
