use super::*;

#[test]
fn live_acpi_thermal_zones_query() {
    let result = query_acpi_thermal_zones();
    #[cfg(windows)]
    {
        if let Ok(zones) = result {
            eprintln!("LIVE ACPI THERMAL ZONES: {zones:?}");
            for zone in zones {
                assert!(zone.temperature_c > 0.0 && zone.temperature_c < 120.0);
            }
        }
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}
