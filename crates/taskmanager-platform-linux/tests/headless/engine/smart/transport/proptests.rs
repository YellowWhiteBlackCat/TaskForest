//! Property tests for the smartctl JSON / device-path parsers.
//!
//! Properties under test:
//! * arbitrary text (including control chars and lossy non-UTF-8 bytes) up to
//!   64 KiB never panics the pure parsers;
//! * a successful parse always derives at least one recognised field from the
//!   input — success is never fabricated from garbage;
//! * ATA attribute rows keep typed semantics: an `id` beyond `u16::MAX` is
//!   dropped instead of truncated, and failure signals map exactly;
//! * the device path accepts exactly the documented alphabet/length contract
//!   and is idempotent;
//! * smartctl exit-status data eligibility is exactly the low three bits.

use proptest::prelude::*;
use proptest::string::string_regex;
use taskmanager_core::core::metrics::SmartAvailability;

use super::{parse_ata_attributes, parse_smartctl_json, smartctl_device_path};

/// Any byte sequence up to `max_bytes`, lossily converted to UTF-8. Models the
/// shell-out boundary: corrupt stdout either becomes a replacement char or
/// (`String::from_utf8` failure) an empty output, so this is the widest input
/// shape the parser can ever receive.
fn utf8_lossy_bytes(max_bytes: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(any::<u8>(), 0..max_bytes)
        .prop_map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Arbitrarily nested JSON values so the parser sees every shape a hostile or
/// corrupt `smartctl --json=c` stdout could present.
fn arbitrary_json_value(depth: usize) -> BoxedStrategy<serde_json::Value> {
    let leaf = prop_oneof![
        any::<bool>().prop_map(serde_json::Value::Bool),
        any::<f64>().prop_map(serde_json::Value::from),
        any::<u64>().prop_map(serde_json::Value::from),
        any::<String>().prop_map(serde_json::Value::String),
        Just(serde_json::Value::Null),
    ];
    if depth == 0 {
        return leaf.boxed();
    }
    prop_oneof![
        leaf,
        prop::collection::vec(arbitrary_json_value(depth - 1), 0..8)
            .prop_map(serde_json::Value::Array),
        prop::collection::hash_map(any::<String>(), arbitrary_json_value(depth - 1), 0..8)
            .prop_map(|map| serde_json::Value::Object(map.into_iter().collect())),
    ]
    .boxed()
}

/// The observed sample must never claim availability that the input did not
/// support: every field is either derived from the text or absent.
fn assert_parsed_sample_is_grounded(smart: &taskmanager_core::core::smart::DiskSmart) {
    assert_eq!(smart.availability, SmartAvailability::Available);
    assert!(
        smart.temperature_c.is_some()
            || smart.critical_warning.is_some()
            || smart.percent_used.is_some()
            || smart.power_on_hours.is_some()
            || smart.ata_attributes.is_some(),
        "a parsed sample must derive at least one recognised field from the input"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_text_never_panics_and_never_fabricates_success(
        text in utf8_lossy_bytes(64 * 1024),
    ) {
        if let Some(smart) = parse_smartctl_json(&text) {
            assert_parsed_sample_is_grounded(&smart);
        }
    }

    #[test]
    fn truncated_and_control_char_text_never_panics(
        prefix in utf8_lossy_bytes(512),
        control in prop_oneof![
            Just("\u{0}"), Just("\u{1f}"), Just("\u{7f}"), Just("\u{00ad}"),
        ],
        suffix in utf8_lossy_bytes(512),
    ) {
        let text = format!("{prefix}{control}{suffix}");
        if let Some(smart) = parse_smartctl_json(&text) {
            assert_parsed_sample_is_grounded(&smart);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn arbitrary_json_never_panics_and_success_is_grounded(value in arbitrary_json_value(4)) {
        let text = serde_json::to_string(&value).expect("generated JSON serializes");
        if let Some(smart) = parse_smartctl_json(&text) {
            assert_parsed_sample_is_grounded(&smart);
        }
    }

    #[test]
    fn ata_attribute_rows_keep_typed_semantics(
        id in any::<u64>(),
        raw_value in any::<u64>(),
        signal in prop_oneof![
            Just(""),
            Just("\"failing_now\":true,"),
            Just("\"failing_now\":false,"),
            Just("\"failed\":true,"),
            Just("\"failed\":false,"),
            Just("\"when_failed\":\"now\","),
            Just("\"when_failed\":\"past\","),
            Just("\"when_failed\":\"\","),
        ],
    ) {
        let text = format!(
            r#"{{"ata_smart_attributes":{{"table":[{{"id":{id},{signal}"raw":{{"value":{raw_value}}}}}]}}}}"#
        );
        if id > u64::from(u16::MAX) {
            assert!(
                parse_smartctl_json(&text).is_none(),
                "attribute id {id} beyond u16 must be dropped, not truncated"
            );
            return Ok(());
        }
        let smart = parse_smartctl_json(&text).expect("bounded attribute id parses");
        let table = smart
            .ata_attributes
            .as_ref()
            .expect("parsed attribute table is present");
        assert_eq!(table.len(), 1);
        assert_eq!(table[0].id, u16::try_from(id).expect("bounded attribute id"));
        assert_eq!(table[0].raw_value, raw_value);
        let failing = matches!(
            signal,
            "\"failing_now\":true," | "\"failed\":true," | "\"when_failed\":\"now\","
        );
        assert_eq!(table[0].failing_now, failing, "input: {text}");
    }

    #[test]
    fn attribute_rows_are_skipped_when_fields_are_not_typed(
        row in prop_oneof![
            Just(r#"{"id":true,"raw":{"value":1}}"#),
            Just(r#"{"id":"one","raw":{"value":1}}"#),
            Just(r#"{"id":-1,"raw":{"value":1}}"#),
            Just(r#"{"id":5,"raw":{"value":true}}"#),
            Just(r#"{"id":5,"raw":{"value":-1}}"#),
            Just(r#"{"id":5,"raw":{"value":18446744073709551616}}"#),
        ],
    ) {
        let text = format!(
            r#"{{"ata_smart_attributes":{{"table":[{row}]}}}}"#
        );
        let smart = parse_smartctl_json(&text);
        assert!(
            smart.is_none(),
            "malformed rows must be dropped, not defaulted: {text}"
        );
    }

    #[test]
    fn temperature_keeps_the_typed_range_semantics(value in any::<f64>()) {
        let text = format!(r#"{{"temperature":{{"current":{value}}}}}"#);
        let smart = parse_smartctl_json(&text);
        if value.is_finite() && (-273.15..=1000.0).contains(&value) {
            let smart = smart.expect("in-range temperature parses");
            assert_eq!(smart.temperature_c, Some(value as f32));
        } else {
            assert!(
                smart.is_none(),
                "out-of-range or non-finite temperature must not fabricate a sample"
            );
        }
    }

    #[test]
    fn device_path_accepts_only_bounded_alphabet_names(
        name in string_regex("[a-zA-Z0-9._-]{0,300}").unwrap(),
    ) {
        let accepted = !name.is_empty()
            && !name.starts_with('-')
            && name.len() <= 255
            && !matches!(name.as_str(), "." | "..");
        let path = smartctl_device_path(&name);
        if accepted {
            let expected = format!("/dev/{name}");
            assert_eq!(path.as_deref(), Some(expected.as_str()), "name {name:?}");
        } else {
            assert_eq!(path, None, "name {name:?} must be rejected");
        }
    }

    #[test]
    fn device_path_never_panics_and_results_are_well_formed(name in utf8_lossy_bytes(1024)) {
        let path = smartctl_device_path(&name);
        if let Some(path) = path {
            assert!(path.starts_with("/dev/"), "name {name:?}");
            assert!(path.len() <= "/dev/".len() + 255);
            assert_eq!(
                smartctl_device_path(&path),
                Some(path),
                "device paths must round-trip"
            );
        }
    }

    #[test]
    fn device_path_is_case_sensitive_and_slash_aware(
        name in string_regex("[a-zA-Z0-9._-]{1,64}").unwrap(),
    ) {
        let bare = smartctl_device_path(&name);
        let prefixed = smartctl_device_path(&format!("/dev/{name}"));
        assert_eq!(bare, prefixed, "/dev/ prefix must be stripped");
        let doubled = smartctl_device_path(
            &format!("/dev//dev/{name}"),
        );
        assert_eq!(
            doubled, None,
            "only the leading /dev/ is stripped; a slash inside the name must be rejected"
        );
    }
}

#[cfg(target_os = "linux")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn exit_status_accepts_data_iff_low_three_bits_are_clear(code in any::<i32>()) {
        assert_eq!(super::smartctl_exit_allows_data(code), code & 0b111 == 0);
    }
}

/// Deterministic regression counterexamples: truncated, oversized, control
/// chars, and non-UTF-8 byte streams. Proptest covers the search space; these
/// pin the exact shapes that must never panic and never fabricate data.
#[test]
fn damaged_inputs_never_panic_and_garbage_never_parses() {
    let oversized_digits = "9".repeat(80_000);
    let control_chars = "\u{0}\u{1f}\u{7f}";
    let cases: Vec<(String, bool)> = [
        ("", false),
        ("not json", false),
        ("{", false),
        ("{[]}", false),
        ("{\"temperature\":", false),
        ("{\"temperature\":{\"current\":}}", false),
        ("{\"temperature\":{\"current\":1e999}}", false),
        ("{\"temperature\":{\"current\":-1e999}}", false),
        ("{\"temperature\":{\"current\":\"hot\"}}", false),
        ("{\"smart_status\":{\"passed\":\"true\"}}", false),
        ("{\"smart_status\":{\"passed\":true}}", true),
        ("{\"smart_status\":{\"passed\":false}}", true),
        (
            "{\"ata_smart_attributes\":{\"table\":[{\"id\":65536,\"raw\":{\"value\":0}}]}}",
            false,
        ),
        (
            "{\"ata_smart_attributes\":{\"table\":[{\"id\":18446744073709551616}]}}",
            false,
        ),
        (
            "{\"ata_smart_attributes\":{\"table\":[{\"id\":-1,\"raw\":{\"value\":0}}]}}",
            false,
        ),
        (
            "{\"ata_smart_attributes\":{\"table\":[{\"id\":1,\"raw\":{\"value\":7}}]}}",
            true,
        ),
        (
            "{\"smart_status\":{\"passed\":false},\"temperature\":{\"current\":1000.5}}",
            true,
        ),
        (
            "{\"power_on_time\":{\"hours\":18446744073709551616}}",
            false,
        ),
        ("{\"temperature\":{\"current\":3.14e9999}}", false),
        (
            "{\"temperature\":{\"current\":3.141592653589793238462643383279502884197169}}",
            true,
        ),
    ]
    .into_iter()
    .map(|(input, expected)| (input.to_owned(), expected))
    .collect();
    let mut cases = cases;
    cases.push((format!("x{control_chars}"), false));
    cases.push((format!("{{\"temperature\":{control_chars}"), false));
    cases.push((
        format!("{{\"power_on_time\":{{\"hours\":{oversized_digits}}}"),
        false,
    ));
    cases.push(("x".repeat(128 * 1024), false));

    for (input, expected_some) in cases {
        let smart = parse_smartctl_json(&input);
        assert_eq!(
            smart.is_some(),
            expected_some,
            "input prefix {:?}",
            &input[..input.len().min(64)]
        );
        if let Some(smart) = smart {
            assert_parsed_sample_is_grounded(&smart);
        }
    }
}

#[test]
fn non_utf8_stdout_boundary_collapses_to_empty_output() {
    // The call site maps `String::from_utf8` failure to an empty stdout, so a
    // non-UTF-8 byte stream reaches the parser as "". That mapping must not
    // panic and must not fabricate a sample.
    assert!(String::from_utf8(vec![0xff, 0xfe, b'{', 0x80]).is_err());
    assert!(parse_smartctl_json("").is_none());
}

#[test]
fn parse_ata_attributes_drops_rows_without_id_or_raw_value() {
    let attributes = [
        serde_json::json!({"name": "Reallocated_Sector_Ct", "raw": {"value": 3}}),
        serde_json::json!({"id": 5, "raw": {"value": 3}}),
        serde_json::json!({"id": 5}),
        serde_json::json!({"id": 1, "raw": {"value": 2}, "when_failed": "now"}),
    ];
    let parsed = parse_ata_attributes(Some(&attributes)).expect("valid rows parse");
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].id, 5);
    assert!(!parsed[0].failing_now);
    assert_eq!(parsed[1].id, 1);
    assert!(parsed[1].failing_now);
}
