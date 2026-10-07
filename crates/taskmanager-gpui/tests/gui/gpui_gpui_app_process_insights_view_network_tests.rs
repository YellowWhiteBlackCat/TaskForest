//! Headless formatter tests for the process insights network-throughput card.

use super::formatting::format_rate;
use taskmanager_core::core::units::UnitPreferences;

#[test]
fn network_throughput_formats_observed_rates_and_keeps_absence_honest() {
    let units = UnitPreferences::default();
    assert_eq!(format_rate(units, Some(1024), "—"), "8.2 Kb/s");
    assert_eq!(format_rate(units, Some(2048), "—"), "16.4 Kb/s");
    assert_eq!(
        format_rate(units, None, "—"),
        "—",
        "an unobserved rate stays the shared dash, never a fabricated zero"
    );
}
