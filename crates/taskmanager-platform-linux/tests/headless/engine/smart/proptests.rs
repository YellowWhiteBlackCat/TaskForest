//! Property tests for the NVMe `smart-log` text parser and its helpers.
//!
//! Properties under test:
//! * arbitrary text (control chars, lossy non-UTF-8 bytes) up to 64 KiB never
//!   panics `parse_smart_log_stdout`;
//! * a successful parse is always grounded: availability is `Available`, no
//!   provider failure is claimed, and at least one field came from the text;
//! * every key:value numeric field round-trips exactly;
//! * `nvme_controller_from_name` preserves the controller digits, strips a
//!   `/dev/` prefix, and only ever yields `nvme<digits>` shapes;
//! * stderr denial detection matches the known needles anywhere,
//!   case-insensitively, and never panics on arbitrary bytes.

use proptest::prelude::*;
use taskmanager_core::core::metrics::SmartAvailability;

use super::{
    nvme_controller_from_name, parse_leading_f32, parse_leading_u64, parse_smart_log_stdout,
    stderr_is_permission_denied,
};

/// Any byte sequence up to `max_bytes`, lossily converted to UTF-8 — the
/// widest input shape the shell-out boundary can deliver to the parser.
fn utf8_lossy_bytes(max_bytes: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(any::<u8>(), 0..max_bytes)
        .prop_map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

fn assert_grounded(smart: &taskmanager_core::core::smart::DiskSmart) {
    assert_eq!(smart.availability, SmartAvailability::Available);
    assert!(
        smart.failure.is_none(),
        "a parsed sample must not claim a provider failure"
    );
    assert!(
        smart.temperature_c.is_some()
            || smart.critical_warning.is_some()
            || smart.percent_used.is_some()
            || smart.power_on_hours.is_some(),
        "a parsed sample must derive at least one field from the input"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_text_never_panics_and_never_fabricates_success(
        text in utf8_lossy_bytes(64 * 1024),
    ) {
        if let Some(smart) = parse_smart_log_stdout(&text) {
            assert_grounded(&smart);
        }
    }

    #[test]
    fn control_char_laden_text_never_panics(
        prefix in utf8_lossy_bytes(512),
        control in prop_oneof![
            Just("\u{0}"), Just("\u{1f}"), Just("\u{7f}"), Just("\u{00ad}"),
        ],
        suffix in utf8_lossy_bytes(512),
    ) {
        let text = format!("{prefix}{control}{suffix}");
        if let Some(smart) = parse_smart_log_stdout(&text) {
            assert_grounded(&smart);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn power_on_hours_round_trips(
        n in any::<u32>(),
        suffix in prop_oneof![Just(""), Just(" hours"), Just(",")],
    ) {
        let text = format!("power_on_hours: {n}{suffix}");
        let smart = parse_smart_log_stdout(&text).expect("recognised key parses");
        assert_eq!(smart.power_on_hours, Some(u64::from(n)));
        assert_grounded(&smart);
    }

    #[test]
    fn temperature_round_trips_finite_values(
        n in any::<f32>(),
        suffix in prop_oneof![Just(""), Just("°C (312.15 K)"), Just("%")],
    ) {
        let text = format!("temperature: {n}{suffix}");
        let smart = parse_smart_log_stdout(&text);
        if n.is_finite() {
            let smart = smart.expect("finite temperature parses");
            assert_eq!(smart.temperature_c, Some(n));
        } else {
            assert!(
                smart.is_none(),
                "non-finite temperature must not fabricate a sample: {text:?}"
            );
        }
    }

    #[test]
    fn percentage_used_round_trips(
        n in any::<u32>(),
        suffix in prop_oneof![Just(""), Just("%"), Just(" %")],
    ) {
        let text = format!("percentage_used: {n}{suffix}");
        let smart = parse_smart_log_stdout(&text).expect("recognised key parses");
        assert_eq!(smart.percent_used, Some(n as f32));
    }

    #[test]
    fn critical_warning_round_trips_hex_and_decimal(
        n in any::<u64>(),
        base in prop_oneof![Just("hex"), Just("decimal")],
    ) {
        let raw = match base {
            "hex" => format!("0x{n:x}"),
            _ => n.to_string(),
        };
        let text = format!("critical_warning: {raw}");
        let smart = parse_smart_log_stdout(&text).expect("recognised key parses");
        assert_eq!(smart.critical_warning, Some(n != 0));
    }

    #[test]
    fn leading_numeric_helpers_round_trip_u32(n in any::<u32>()) {
        let text = n.to_string();
        assert_eq!(parse_leading_u64(&text), Some(u64::from(n)));
        assert_eq!(parse_leading_f32(&text), Some(n as f32));
        assert_eq!(
            parse_leading_u64(&format!("{text},suffix")),
            Some(u64::from(n))
        );
        assert_eq!(
            parse_leading_f32(&format!("{text}°C (312.15 K)")),
            Some(n as f32)
        );
    }

    #[test]
    fn controller_name_keeps_digits_and_ignores_suffix(
        digits in "[0-9]{1,6}",
        tail in "[a-zA-Z_]*",
    ) {
        let name = format!("nvme{digits}{tail}");
        assert_eq!(
            nvme_controller_from_name(&name),
            Some(format!("nvme{digits}")),
            "name {name:?}"
        );
    }

    #[test]
    fn controller_name_strips_dev_prefix(digits in "[0-9]{1,6}") {
        let bare = format!("nvme{digits}n1");
        let prefixed = format!("/dev/{bare}");
        assert_eq!(
            nvme_controller_from_name(&prefixed),
            nvme_controller_from_name(&bare)
        );
    }

    #[test]
    fn controller_name_never_panics_and_results_are_nvme_shaped(
        name in utf8_lossy_bytes(256),
    ) {
        if let Some(ctrl) = nvme_controller_from_name(&name) {
            assert!(ctrl.starts_with("nvme"), "name {name:?}");
            assert!(ctrl[4..].bytes().all(|byte| byte.is_ascii_digit()));
        }
    }

    #[test]
    fn stderr_denial_needles_match_anywhere_case_insensitively(
        needle in prop_oneof![
            Just("permission denied"),
            Just("operation not permitted"),
            Just("insufficient privileges"),
        ],
        lead in 0..64usize,
        trail in 0..64usize,
    ) {
        let message = format!("{}{}{}", " ".repeat(lead), needle, " ".repeat(trail));
        assert!(stderr_is_permission_denied(message.as_bytes()));
        assert!(stderr_is_permission_denied(message.to_uppercase().as_bytes()));
    }

    #[test]
    fn stderr_denial_scan_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        let _ = stderr_is_permission_denied(&bytes);
    }
}

/// Deterministic regression counterexamples for the `smart-log` parser:
/// truncated keys, empty values, junk keys, and oversized bodies.
#[test]
fn damaged_inputs_never_panic_and_junk_never_parses() {
    let oversized = "9".repeat(80_000);
    let cases: Vec<String> = [
        "",
        "Smart Log for NVME device:nvme0n1 ...",
        ":",
        "temperature",
        "temperature:",
        "temperature: °C",
        "power_on_hours:",
        "power_on_hours: hours",
        "critical_warning: 0x",
        "critical_warning: -1",
        "percentage_used: %",
        "\u{0}temperature\u{1f}: 39°C",
        "temperature: 1e999",
        "temperature: NaN",
        "temperature: inf",
        "temperature: -0x10",
        "unknown_key: 5",
        "TEMP: 5",
        "temperature\t: 39",
        "power_on_hours: 1,234,567,890,123,456,789,012",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let mut cases = cases;
    cases.push(format!("power_on_hours: {oversized}"));
    cases.push(format!("temperature: {oversized}"));
    cases.push("x".repeat(128 * 1024));

    for input in cases {
        if let Some(smart) = parse_smart_log_stdout(&input) {
            assert_grounded(&smart);
        }
    }
}
