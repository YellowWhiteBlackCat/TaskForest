//! System-page unit tests (line split).

mod detail_rows;

fn format_uptime_compact(secs: u64) -> String {
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3_600;
    let m = (secs % 3_600) / 60;
    if d > 0 {
        format!("{d}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

#[cfg(test)]
mod tests_inner {
    // Import ONLY the helper under test — NOT `use super::*`. The parent module
    // has `use gpui::*;`, whose prelude shadows the built-in `#[test]` attribute
    // macro once re-globbed in here, which trips a recursion-limit error on the
    // attribute itself. Keeping this scope minimal resolves `#[test]` to the
    // std built-in.
    use super::super::{
        fmt_cache_kb, fmt_clock_ghz, fmt_observed_clock_ghz, joined_optional_text, kernel_display,
        optional_text, truncate_cmdline,
    };
    use super::detail_rows::{battery_detail_rows, thermal_control_rows};
    use super::format_uptime_compact;
    use crate::core::{
        BatteryInfo, BatteryScalarObservations, DeviceGeneration, DeviceState, ScalarObservation,
        SensorCenterSnapshot, ThermalControlSnapshot, ThermalCoolingActivity,
        ThermalCoolingDeviceStatus, ThermalThrottleSnapshot,
    };
    use crate::i18n;
    #[test]
    fn uptime_compact_formats_days_hours_minutes() {
        // Seconds are truncated (smallest unit is minutes, per the spec). So both
        // a 0s and a 59s uptime render "0m" (zero full minutes elapsed).
        assert_eq!(format_uptime_compact(0), "0m");
        assert_eq!(format_uptime_compact(59), "0m");
        // Sub-hour: minutes only.
        assert_eq!(format_uptime_compact(60), "1m");
        assert_eq!(format_uptime_compact(125), "2m"); // 2m5s → "2m"
        // Sub-day: hours + minutes.
        assert_eq!(format_uptime_compact(3_600), "1h 0m");
        assert_eq!(format_uptime_compact(3_661), "1h 1m"); // 1h1m1s → "1h 1m"
        // >= 1 day: days + hours + minutes.
        assert_eq!(format_uptime_compact(86_400), "1d 0h 0m");
        assert_eq!(format_uptime_compact(90_061), "1d 1h 1m"); // 1d1h1m1s
        assert_eq!(format_uptime_compact(86_400 * 3 + 3_661), "3d 1h 1m");
    }

    #[test]
    fn cache_readout_distinguishes_unknown_from_measured_zero() {
        assert_eq!(fmt_cache_kb(None), "—");
        assert_eq!(fmt_cache_kb(Some(0)), "0 KiB");
        assert_eq!(fmt_cache_kb(Some(2048)), "2 MiB");
    }

    #[test]
    fn optional_max_clock_distinguishes_missing_from_measured_zero() {
        assert_eq!(fmt_observed_clock_ghz(None), "—");
        assert_eq!(fmt_observed_clock_ghz(Some(0)), "0.00 GHz");
        assert_eq!(fmt_observed_clock_ghz(Some(4_400)), "4.40 GHz");
    }

    #[test]
    fn optional_base_clock_distinguishes_missing_from_measured_zero() {
        assert_eq!(fmt_clock_ghz(None), "—");
        assert_eq!(fmt_clock_ghz(Some(0)), "0.00 GHz");
        assert_eq!(fmt_clock_ghz(Some(2_400)), "2.40 GHz");
    }

    #[test]
    fn kernel_display_composes_platform_neutral_version_and_build_facts() {
        assert_eq!(
            kernel_display(Some("6.1.5-example"), Some("(builder@host) #1 SMP")),
            "6.1.5-example · (builder@host) #1 SMP"
        );
    }

    #[test]
    fn kernel_display_falls_back_to_version_when_build_is_unavailable() {
        assert_eq!(kernel_display(Some("6.1.0"), None), "6.1.0");
        assert_eq!(kernel_display(Some("6.1.0"), Some("   ")), "6.1.0");
        assert_eq!(kernel_display(None, Some("native build")), "native build");
        assert_eq!(kernel_display(None, None), "—");
        assert_eq!(joined_optional_text(None, None), "—");
    }

    #[test]
    fn unavailable_hardware_identity_renders_as_missing_not_a_platform_fallback() {
        assert_eq!(optional_text(None), "—");
        assert_eq!(optional_text(Some("   ")), "—");
        assert_eq!(
            joined_optional_text(None, Some("ExampleOS 1")),
            "ExampleOS 1"
        );
        assert_eq!(
            joined_optional_text(Some("ExampleOS"), Some("1")),
            "ExampleOS 1"
        );
    }

    #[test]
    fn truncate_cmdline_short_passes_through_unchanged() {
        assert_eq!(truncate_cmdline("quiet"), "quiet");
        assert_eq!(truncate_cmdline(""), "");
        // Exactly MAX (80) chars: boundary — no truncation.
        let exact: String = "a".repeat(80);
        assert_eq!(truncate_cmdline(&exact), exact);
    }

    #[test]
    fn truncate_cmdline_long_gets_ellipsis_suffix() {
        // 81 chars (>MAX): truncate to 79 chars + "…" = 80 chars total.
        let long: String = "a".repeat(81);
        let t = truncate_cmdline(&long);
        assert_eq!(t.chars().count(), 80);
        assert!(t.ends_with('…'));
        // The first 79 source chars are preserved.
        assert_eq!(t.chars().filter(|c| *c == 'a').count(), 79);
    }

    #[test]
    fn truncate_cmdline_counts_chars_not_bytes() {
        // Non-ASCII: 2 emoji (8 bytes) + 78 ASCII = 80 chars → unchanged.
        let s: String = "😀😀".to_string() + &"a".repeat(78);
        assert_eq!(truncate_cmdline(&s), s);
        // 3 emoji + 78 ASCII = 81 chars → truncate to 80 chars (79 source + "…"),
        // no codepoint split.
        let s: String = "😀😀😀".to_string() + &"a".repeat(78);
        let t = truncate_cmdline(&s);
        assert_eq!(t.chars().count(), 80);
        assert!(t.ends_with('…'));
        // The 2 leading emoji are intact (no split mid-codepoint).
        assert!(t.starts_with("😀😀"));
    }

    /// The row LABELS are now resolved through [`i18n::t`], whose active
    /// language is a process-wide global that the parallel `i18n::tests` flip
    /// between En and Zh. Text-equality on labels is therefore inherently racy,
    /// so these tests assert on the locale-independent contract instead: the row
    /// COUNT, the VALUE column (formatting + ordering), and that every emitted
    /// label is non-empty. The key→label mapping is trivially verifiable by code
    /// inspection of [`battery_detail_rows`].
    fn assert_rows(b: &BatteryInfo, expected_values: &[&str]) {
        let rows = battery_detail_rows(b);
        assert_eq!(
            rows.len(),
            expected_values.len(),
            "row count mismatch for {b:?}"
        );
        for ((label, value), want) in rows.iter().zip(expected_values.iter()) {
            assert!(!label.is_empty(), "empty label for value {value}");
            assert_eq!(value, *want);
        }
    }

    fn observed_battery(power_w: Option<f32>, cycle_count: Option<u32>) -> BatteryInfo {
        let mut battery = BatteryInfo::new("test-battery", DeviceState::healthy(10));
        battery.apply_scalar_observations(BatteryScalarObservations {
            power_w: power_w.map_or_else(ScalarObservation::default, |value| {
                ScalarObservation::available(value, 10)
            }),
            cycle_count: cycle_count.map_or_else(ScalarObservation::default, |value| {
                ScalarObservation::available(value, 10)
            }),
            ..Default::default()
        });
        battery
    }

    #[test]
    fn battery_detail_rows_all_fields_present() {
        // Every extended BatteryInfo field populated → all five rows, in order,
        // with the documented value formats (power to 1 dp; cycle count bare).
        let mut b = observed_battery(Some(8.34), Some(127));
        b.status = "Discharging".into();
        b.technology = "Li-ion".into();
        b.model_name = "BATT-001".into();
        b.manufacturer = "SMP".into();
        assert_rows(&b, &["8.3 W", "Li-ion", "127", "BATT-001", "SMP"]);
    }

    #[test]
    fn battery_detail_rows_absent_fields_omitted() {
        // Default BatteryInfo: every extended field is None / empty → no rows.
        assert!(battery_detail_rows(&BatteryInfo::default()).is_empty());
        // power_w present alone → exactly one row. Some(0.0) is a valid idle
        // reading and is kept (gated on Some, not on > 0).
        let b = observed_battery(Some(0.0), None);
        assert_rows(&b, &["0.0 W"]);
        // cycle_count = Some(0) is a valid fresh-pack reading → row kept.
        let b = observed_battery(None, Some(0));
        assert_rows(&b, &["0"]);
        // Empty-string fields (technology / model_name / manufacturer) are
        // omitted, never rendered as blank-value rows.
        let mut b = BatteryInfo::default();
        b.technology = "".into();
        b.model_name = "".into();
        b.manufacturer = "".into();
        assert!(battery_detail_rows(&b).is_empty());
    }

    #[test]
    fn battery_detail_rows_power_w_one_decimal_place() {
        // Power is rendered to one decimal place, matching the spec's "X.X W".
        // 12.5 is exactly representable in f32 → "12.5 W".
        let b = observed_battery(Some(12.5), None);
        assert_rows(&b, &["12.5 W"]);
    }

    #[test]
    fn thermal_control_rows_render_typed_states_and_measured_zero_throttle() {
        let sensors = SensorCenterSnapshot {
            thermal_control: ThermalControlSnapshot {
                cooling_devices: vec![ThermalCoolingDeviceStatus {
                    id: "cooling:acpi:channel:fan0".into(),
                    device_id: "cooling:acpi".into(),
                    device_generation: DeviceGeneration::default(),
                    kind: ScalarObservation::available(crate::core::ThermalCoolingKind::Fan, 10),
                    current_state: ScalarObservation::available(2, 10),
                    maximum_state: ScalarObservation::available(255, 10),
                    activity: ScalarObservation::available(ThermalCoolingActivity::Active, 10),
                }],
                throttle: ThermalThrottleSnapshot::from_observations(
                    10,
                    ScalarObservation::available(0, 10),
                    ScalarObservation::available(0, 10),
                ),
                ..Default::default()
            },
            ..Default::default()
        };

        let rows = thermal_control_rows(&sensors);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            (
                format!("{} — cooling:acpi:channel:fan0", i18n::t("system.cooling")),
                format!("2/255 · {}", i18n::t("system.cooling_active"))
            )
        );
        assert_eq!(
            rows[1],
            (
                i18n::t("system.throttle").to_string(),
                format!(
                    "{} 0 · {} 0",
                    i18n::t("system.throttle_core"),
                    i18n::t("system.throttle_package")
                )
            ),
            "measured zero counts stay visible"
        );
    }

    #[test]
    fn thermal_control_rows_render_typed_absence_as_dashes() {
        let sensors = SensorCenterSnapshot {
            thermal_control: ThermalControlSnapshot {
                cooling_devices: vec![ThermalCoolingDeviceStatus {
                    id: "cooling:acpi:channel:fan0".into(),
                    device_id: "cooling:acpi".into(),
                    device_generation: DeviceGeneration::default(),
                    kind: ScalarObservation::default(),
                    current_state: ScalarObservation::default(),
                    maximum_state: ScalarObservation::default(),
                    activity: ScalarObservation::default(),
                }],
                ..Default::default()
            },
            ..Default::default()
        };

        let rows = thermal_control_rows(&sensors);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0].1, "—",
            "unavailable cooling state must not fabricate a value"
        );
        assert_eq!(
            rows[1].1, "—",
            "unavailable throttle counts must not fabricate a zero"
        );
    }
}
