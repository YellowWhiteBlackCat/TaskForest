use super::{
    DiagnosticBundleErrorKind, DiagnosticBundlePlan, DiagnosticSource, RedactionSummary,
    redact_paths, validate_source_name,
};

#[test]
fn redaction_total_is_the_sum_of_all_counts() {
    let summary = RedactionSummary {
        usernames: 2,
        paths: 3,
        ipv4_addresses: 4,
        ipv6_addresses: 5,
    };
    assert_eq!(summary.total(), 14);
    assert_eq!(RedactionSummary::default().total(), 0);
}

#[test]
fn prepare_truncates_the_excerpt_at_the_preview_chars_boundary() {
    // Exactly at the boundary: no ellipsis. One past it: ellipsis (a
    // `>`→`>=` mutation flips the boundary case).
    let exact = DiagnosticBundlePlan::prepare(
        vec![DiagnosticSource {
            name: "exact.txt".into(),
            contents: "x".repeat(800),
        }],
        [],
    )
    .expect("plan");
    assert_eq!(exact.preview.files[0].excerpt.chars().count(), 800);
    assert!(!exact.preview.files[0].excerpt.contains('…'));

    let over = DiagnosticBundlePlan::prepare(
        vec![DiagnosticSource {
            name: "over.txt".into(),
            contents: "y".repeat(801),
        }],
        [],
    )
    .expect("plan");
    assert!(
        over.preview.files[0].excerpt.ends_with('…'),
        "excerpt past the boundary must carry the ellipsis"
    );
}

#[test]
fn source_names_are_rejected_for_every_rule_independently() {
    // Each rule is a separate OR arm: an input violating exactly ONE of
    // them must be rejected (a `||`→`&&` mutation would let it through).
    for (name, what) in [
        ("", "empty"),
        (&"x".repeat(97), "too long"),
        (".", "dot"),
        ("..", "dotdot"),
        ("a/b", "slash"),
        ("a\\b", "backslash"),
        ("a\u{7}b", "control char"),
    ] {
        let error = validate_source_name(name).expect_err("must reject");
        assert_eq!(
            error.kind(),
            DiagnosticBundleErrorKind::InvalidSource,
            "{what}: {name:?} must be an invalid-source rejection"
        );
    }
    validate_source_name("valid-name.txt").expect("valid names pass");
}

#[test]
fn redact_paths_counts_every_redaction_and_skips_urls() {
    let (output, count) = redact_paths("/home/<user>/a b /etc/hosts https://x/y");
    assert_eq!(count, 2, "two real paths redacted, the URL untouched");
    assert!(!output.contains("/home/<user>"));
    assert!(!output.contains("/etc/hosts"));
    assert!(output.contains("https://x/y"), "URLs are not paths");
    assert_eq!(redact_paths("no paths here").1, 0);
}
