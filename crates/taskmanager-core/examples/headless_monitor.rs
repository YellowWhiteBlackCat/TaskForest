//! Minimal headless system-monitoring example powered by `taskmanager-core`.
//!
//! Demonstrates how downstream tools consume core domain models:
//! unit formatting, stable identity, and alert rule evaluation.

use std::time::Duration;

use taskmanager_core::alerts::{AlertEngine, AlertMetric, AlertRule, AlertSeverity};
use taskmanager_core::identity::{DeviceId, ProviderId};
use taskmanager_core::units::{UnitPreferences, format_byte_rate, format_memory};

fn main() {
    println!("=== TaskForest Core Headless Example ===");

    // 1. Stable Identity
    let provider = ProviderId::borrowed("linux.procfs");
    let device = DeviceId::new("nvme0n1");
    println!("Provider : {provider}");
    println!("Device   : {}", device.as_str());

    // 2. Canonical Unit Formatting
    let prefs = UnitPreferences::default();
    let total_ram_bytes = 34_359_738_368; // 32 GiB
    let tx_rate_bytes_per_sec = 125_000_000; // 1 Gb/s
    println!("Memory   : {}", format_memory(total_ram_bytes, &prefs));
    println!(
        "Network  : {}",
        format_byte_rate(tx_rate_bytes_per_sec, &prefs)
    );

    // 3. In-Memory Alert Engine Configuration
    let rule = AlertRule::new(
        "high-memory-pressure",
        AlertMetric::MemoryUsagePercent,
        AlertSeverity::Warning,
        90.0,
        Duration::from_secs(3),
        2.0, // Hysteresis 2%
    );

    let alerts = AlertEngine::new([rule]);
    println!(
        "Alerts   : {} active rule(s) configured",
        alerts.rules().len()
    );

    println!("Core models evaluated deterministically with zero I/O.");
}
