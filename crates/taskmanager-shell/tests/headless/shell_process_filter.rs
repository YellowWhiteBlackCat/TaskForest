use super::*;
use taskmanager_application::i18n::{Language, set_language};

#[test]
fn status_classifier_covers_every_bucket_and_preserves_stopped_precedence() {
    set_language(Language::En);
    let cases = [
        ("Running", ProcessStatusFilter::Running),
        ("S", ProcessStatusFilter::Sleeping),
        ("Stopped", ProcessStatusFilter::Stopped),
        ("T", ProcessStatusFilter::Stopped),
        ("Zombie", ProcessStatusFilter::Zombie),
        ("disk sleep", ProcessStatusFilter::Other),
        ("", ProcessStatusFilter::Other),
    ];
    for (status, expected) in cases {
        assert_eq!(
            ProcessStatusFilter::classify(status),
            expected,
            "{status:?}"
        );
    }
    assert!(!ProcessStatusFilter::Sleeping.matches("Stopped"));
    assert!(ProcessStatusFilter::Stopped.matches("stopped"));
    assert!(ProcessStatusFilter::All.matches("anything"));
}

#[test]
fn filter_catalog_is_complete_localized_and_keyed() {
    set_language(Language::En);
    let keys: Vec<_> = ProcessStatusFilter::ALL
        .into_iter()
        .map(ProcessStatusFilter::key)
        .collect();
    assert_eq!(
        keys,
        ["all", "running", "sleeping", "stopped", "zombie", "other"]
    );
    assert!(
        ProcessStatusFilter::ALL
            .into_iter()
            .all(|filter| !filter.label().is_empty())
    );
}
