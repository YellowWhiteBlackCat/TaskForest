use std::io;

use super::*;

fn plan(contents: &str) -> DiagnosticBundlePlan {
    DiagnosticBundlePlan::prepare(
        vec![DiagnosticSource {
            name: "facts.txt".into(),
            contents: contents.into(),
        }],
        [],
    )
    .expect("fixed diagnostic source must be valid")
}

#[test]
fn preview_and_encoded_bundle_contain_only_redacted_content() {
    let plan = DiagnosticBundlePlan::prepare(
        vec![DiagnosticSource {
            name: "snapshot.json".into(),
            contents: "user=alice home=/home/<user>/bin peer=192.168.7.9 v6=2001:db8::4 harmless=malice https://example.test/x".into(),
        }],
        ["alice".to_string()],
    )
    .unwrap();
    let preview = plan.preview();
    assert_eq!(preview.files.len(), 1);
    assert_eq!(preview.redactions.usernames, 1);
    assert_eq!(preview.redactions.paths, 1, "{}", preview.files[0].excerpt);
    assert_eq!(preview.redactions.ipv4_addresses, 1);
    assert_eq!(preview.redactions.ipv6_addresses, 1);
    let encoded = String::from_utf8(plan.encoded().unwrap()).unwrap();
    for secret in ["user=alice", "/home/<user>", "192.168.7.9", "2001:db8::4"] {
        assert!(!encoded.contains(secret));
    }
    assert!(encoded.contains("malice"));
    assert!(encoded.contains("https://example.test/x"));
}

#[test]
fn invalid_and_duplicate_sources_are_typed_without_losing_log_detail() {
    let invalid = DiagnosticBundlePlan::prepare(
        vec![DiagnosticSource {
            name: "../secret".into(),
            contents: String::new(),
        }],
        [],
    )
    .unwrap_err();
    assert_eq!(invalid.kind(), DiagnosticBundleErrorKind::InvalidSource);
    assert!(
        invalid
            .detail()
            .is_some_and(|detail| detail.contains("invalid"))
    );

    let duplicate = DiagnosticBundlePlan::prepare(
        vec![
            DiagnosticSource {
                name: "same.txt".into(),
                contents: "one".into(),
            },
            DiagnosticSource {
                name: "same.txt".into(),
                contents: "two".into(),
            },
        ],
        [],
    )
    .unwrap_err();
    assert_eq!(duplicate.kind(), DiagnosticBundleErrorKind::InvalidSource);
    assert!(
        duplicate
            .detail()
            .is_some_and(|detail| detail.contains("duplicate"))
    );
}

#[test]
fn encoder_failure_is_typed_and_retains_underlying_detail() {
    let error = plan("safe")
        .encoded_with(|_| Err(serde_json::Error::io(io::Error::other("encoder offline"))))
        .unwrap_err();
    assert_eq!(error.kind(), DiagnosticBundleErrorKind::Encode);
    assert_eq!(error.detail(), Some("encoder offline"));
}
