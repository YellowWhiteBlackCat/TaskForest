//! Property tests for the systemd/OpenRC text parsers in [`super`].
//!
//! Properties under test:
//! * arbitrary text (control chars, lossy non-UTF-8 bytes) never panics any
//!   parser;
//! * typed relation edges only ever carry validated unit names and the last
//!   matching line wins — authority is never issued for patterns or paths;
//! * service rows only ever carry names the OpenRC name validator accepts;
//! * description parsing yields a substring of the input, never synthesized
//!   copy.

use proptest::prelude::*;
use proptest::string::string_regex;
use taskmanager_core::ServiceId;

use super::{ServiceRelationKind, ServiceStatus};
use super::{
    openrc_service_id, systemd_unit_id, valid_openrc_service_name, valid_systemd_unit_name,
};
use super::{
    parse_openrc_description, parse_openrc_status, parse_openrc_update, parse_systemctl_show_deps,
    strip_matching_quotes,
};

/// Any byte sequence up to `max_bytes`, lossily converted to UTF-8 — the
/// widest input shape a captured command's stdout can deliver.
fn utf8_lossy_bytes(max_bytes: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(any::<u8>(), 0..max_bytes)
        .prop_map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Arbitrary text with no line breaks: models one `systemctl show` line.
fn line_fragment(max_bytes: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(any::<u8>(), 0..max_bytes)
        .prop_map(|bytes| String::from_utf8_lossy(&bytes).replace(['\r', '\n'], " "))
}

fn valid_openrc_name() -> impl Strategy<Value = String> {
    string_regex("[a-zA-Z0-9._@:][a-zA-Z0-9._@:-]{0,63}").unwrap()
}

fn valid_unit_name() -> impl Strategy<Value = String> {
    string_regex(
        "[a-zA-Z0-9._@:-]{1,48}\\.(service|socket|target|device|mount|timer|path|slice|scope)",
    )
    .unwrap()
    .prop_filter(
        "systemd unit validator accepts the generated name",
        |name| valid_systemd_unit_name(name),
    )
}

/// `parse_systemctl_show_deps` maps exactly the eleven known relation keys and
/// never fabricates an edge: every edge kind is a known variant and every
/// target survived the unit-name validator.
fn assert_deps_grounded(deps: &super::ServiceDeps) {
    for edge in deps.relations().edges() {
        assert!(
            !matches!(edge.kind, ServiceRelationKind::Unknown(_)),
            "parser must never fabricate unknown relation kinds"
        );
        assert!(
            valid_systemd_unit_name(edge.target.as_str()),
            "typed edges must only carry validated unit names: {:?}",
            edge.target
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_text_never_panics_and_edges_stay_typed(
        text in utf8_lossy_bytes(64 * 1024),
    ) {
        let deps = parse_systemctl_show_deps(&text);
        assert_deps_grounded(&deps);
        for item in parse_openrc_status(&text) {
            assert!(valid_openrc_service_name(&item.name), "name {:?}", item.name);
            assert_eq!(item.id, openrc_service_id(&item.name));
        }
        for item in parse_openrc_update(&text) {
            assert!(valid_openrc_service_name(&item.name), "name {:?}", item.name);
            assert_eq!(item.id, openrc_service_id(&item.name));
        }
        let _ = parse_openrc_description(&text);
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
        let _ = parse_systemctl_show_deps(&text);
        let _ = parse_openrc_status(&text);
        let _ = parse_openrc_update(&text);
        let _ = parse_openrc_description(&text);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn show_deps_last_line_is_typed_and_validated(
        key in prop_oneof![
            Just("Requires"),
            Just("Wants"),
            Just("Requisite"),
            Just("BindsTo"),
            Just("PartOf"),
            Just("Conflicts"),
            Just("Before"),
            Just("After"),
            Just("WantedBy"),
            Just("RequiredBy"),
            Just("UpheldBy"),
            Just("Id"),
            Just("Description"),
        ],
        value in line_fragment(4096),
        lead in 0..16usize,
        trail in 0..16usize,
    ) {
        let line = format!("{}{}={}{}", " ".repeat(lead), key, value, " ".repeat(trail));
        let deps = parse_systemctl_show_deps(&line);
        assert_deps_grounded(&deps);
        let kind = match key {
            "Requires" => Some(ServiceRelationKind::Requires),
            "Wants" => Some(ServiceRelationKind::Wants),
            "Requisite" => Some(ServiceRelationKind::Requisite),
            "BindsTo" => Some(ServiceRelationKind::BindsTo),
            "PartOf" => Some(ServiceRelationKind::PartOf),
            "Conflicts" => Some(ServiceRelationKind::Conflicts),
            "Before" => Some(ServiceRelationKind::Before),
            "After" => Some(ServiceRelationKind::After),
            "WantedBy" => Some(ServiceRelationKind::WantedBy),
            "RequiredBy" => Some(ServiceRelationKind::RequiredBy),
            "UpheldBy" => Some(ServiceRelationKind::UpheldBy),
            _ => None,
        };
        let Some(kind) = kind else {
            assert!(deps.relations().is_empty(), "unknown key must add no edges");
            return Ok(());
        };
        let expected: Vec<ServiceId> = value
            .split_whitespace()
            .filter(|target| valid_systemd_unit_name(target))
            .map(systemd_unit_id)
            .collect();
        let actual: Vec<ServiceId> = deps.relation_targets(&kind).cloned().collect();
        let mut expected_dedup: Vec<ServiceId> = Vec::new();
        for target in expected {
            if !expected_dedup.contains(&target) {
                expected_dedup.push(target);
            }
        }
        assert_eq!(actual, expected_dedup, "line: {line:?}");
    }

    #[test]
    fn show_deps_last_line_wins_for_repeated_key(
        key in prop_oneof![
            Just("Requires"),
            Just("Wants"),
            Just("WantedBy"),
            Just("After"),
        ],
        first in valid_unit_name(),
        second in valid_unit_name(),
    ) {
        let text = format!("{key}={first}\n{key}={second}\n");
        let deps = parse_systemctl_show_deps(&text);
        let kind = match key {
            "Requires" => ServiceRelationKind::Requires,
            "Wants" => ServiceRelationKind::Wants,
            "WantedBy" => ServiceRelationKind::WantedBy,
            _ => ServiceRelationKind::After,
        };
        assert_eq!(
            deps.relation_targets(&kind).cloned().collect::<Vec<_>>(),
            [systemd_unit_id(&second)]
        );
    }

    #[test]
    fn show_deps_suffixed_keys_are_ignored(
        key in string_regex("[a-zA-Z]{1,12}").unwrap(),
        value in line_fragment(128),
    ) {
        let text = format!("{key}_override={value}\n{key}X={value}\n");
        let deps = parse_systemctl_show_deps(&text);
        assert!(deps.relations().is_empty(), "suffixed keys must not match");
    }

    #[test]
    fn openrc_status_rows_map_name_and_state(
        name in valid_openrc_name(),
        state in prop_oneof![
            Just("started"),
            Just("stopped"),
            Just("crashed"),
            Just("started 00:00:02 (0)"),
            Just("warmstarted"),
        ],
        lead in 0..16usize,
    ) {
        let text = format!("{}{} [ {} ]", " ".repeat(lead), name, state);
        let services = parse_openrc_status(&text);
        if matches!(name.as_str(), "." | "..") {
            assert_eq!(
                services.len(),
                0,
                "the reserved path names must never become services: {text:?}"
            );
            return Ok(());
        }
        assert_eq!(services.len(), 1, "text: {text:?}");
        assert_eq!(services[0].name, name);
        assert_eq!(services[0].id, openrc_service_id(&name));
        assert!(services[0].description.is_empty());
        let status = match state.split_whitespace().next().unwrap_or("") {
            "started" => ServiceStatus::Active,
            "stopped" => ServiceStatus::Inactive,
            "crashed" => ServiceStatus::Failed,
            _ => ServiceStatus::Unknown,
        };
        assert_eq!(services[0].status, status);
        assert_eq!(services[0].active_state, state.split_whitespace().next().unwrap_or(""));
    }

    #[test]
    fn openrc_status_never_issues_authority_for_pattern_names(
        name in utf8_lossy_bytes(64),
    ) {
        let text = format!("{name} [ started ]");
        for service in parse_openrc_status(&text) {
            assert!(valid_openrc_service_name(&service.name), "name {:?}", service.name);
        }
    }

    #[test]
    fn openrc_update_rows_dedupe_and_merge_runlevels(
        name in valid_openrc_name(),
        first in string_regex("[a-zA-Z0-9_-]{1,16}").unwrap(),
        second in string_regex("[a-zA-Z0-9_-]{1,16}").unwrap(),
    ) {
        let text = format!("{name} | {first}\n{name} | {second}\n");
        let services = parse_openrc_update(&text);
        if matches!(name.as_str(), "." | "..") {
            assert_eq!(
                services.len(),
                0,
                "the reserved path names must never become services: {text:?}"
            );
            return Ok(());
        }
        assert_eq!(services.len(), 1, "duplicate names must merge: {text:?}");
        assert_eq!(services[0].name, name);
        assert_eq!(services[0].id, openrc_service_id(&name));
        assert_eq!(services[0].status, ServiceStatus::Unknown);
        let expected = if first == second {
            first.clone()
        } else {
            format!("{first} {second}")
        };
        assert_eq!(services[0].description, expected);
    }

    #[test]
    fn description_parsers_yield_only_substrings(
        value in utf8_lossy_bytes(512),
    ) {
        for line in [
            format!("description={value}"),
            format!("description = {value}"),
            format!("description=\"{value}\""),
            format!("description = \"{value}\"  "),
            format!("description='{value}'"),
        ] {
            if let Some(parsed) = parse_openrc_description(&line) {
                assert!(
                    line.contains(parsed.as_str()),
                    "parsed {:?} is not a substring of {line:?}",
                    parsed
                );
            }
        }
    }

    #[test]
    fn quote_stripping_is_idempotent_and_strips_only_matching_pairs(
        s in utf8_lossy_bytes(256),
    ) {
        let once = strip_matching_quotes(&s);
        assert_eq!(strip_matching_quotes(once), once, "input {:?}", s);
        let quoted = format!("\"{s}\"");
        assert_eq!(strip_matching_quotes(&quoted), s);
        let single_quoted = format!("'{s}'");
        assert_eq!(strip_matching_quotes(&single_quoted), s);
        if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
            assert_eq!(once.len(), s.len() - 2);
        }
    }
}

/// Deterministic regression counterexamples: truncated lines, empty values,
/// header shapes, and oversized bodies for every services parser.
#[test]
fn damaged_inputs_never_panic() {
    let oversized = "9".repeat(80_000);
    let show_deps_cases = [
        "",
        "\n\n",
        "Requires",
        "Requires=",
        "=basic.target",
        "Requires =basic.target",
        "Requires= basic.target",
        "Req uires=x",
        "Requires=\u{0}basic.target\u{1f}",
        "After=network.target\t",
    ];
    let status_cases = [
        "",
        "Runlevel: default",
        "name []",
        "name [",
        "name]",
        "[ started ]",
        "name [ started]extra",
        "\u{0}\u{1f}name [ started ]",
        "name [ started 00:00:02 (0) ]",
    ];
    let update_cases = [
        "",
        "name |",
        "| default",
        "name||default",
        "name | default | extra",
        "\u{0}name\u{1f} | default",
        "name | \u{0}default\u{1f}",
    ];
    let description_cases = [
        "",
        "description",
        "description=",
        "description =",
        "description=\"\"",
        "description=''",
        "description = \"\"",
        "description_foo=x",
        "describer=x",
        "description = x y",
        "description=\"a\"b\"",
        "\u{0}description\u{1f}=x",
    ];
    for case in show_deps_cases {
        let _ = parse_systemctl_show_deps(case);
    }
    for case in status_cases {
        let _ = parse_openrc_status(case);
    }
    for case in update_cases {
        let _ = parse_openrc_update(case);
    }
    for case in description_cases {
        let _ = parse_openrc_description(case);
    }
    for oversized_case in [
        format!("Requires={oversized}"),
        format!("{oversized} [ started ]"),
        format!("{oversized} | default"),
        format!("description={oversized}"),
        "x".repeat(128 * 1024),
    ] {
        let _ = parse_systemctl_show_deps(&oversized_case);
        let _ = parse_openrc_status(&oversized_case);
        let _ = parse_openrc_update(&oversized_case);
        let _ = parse_openrc_description(&oversized_case);
    }
}
