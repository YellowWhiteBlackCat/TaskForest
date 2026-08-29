use super::{empty_state_failure, empty_state_icon};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::core::{FailureKind, ProviderId};
use taskmanager_ui_contract::IconId;

fn source(outcome: SourceOutcome) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed("test.source"),
        outcome,
        item_count: 0,
    }
}

#[test]
fn unavailable_and_partial_sources_report_their_failure() {
    for outcome in [
        SourceOutcome::Unavailable(FailureKind::MissingDependency),
        SourceOutcome::Partial(FailureKind::MissingDependency),
        SourceOutcome::Unavailable(FailureKind::PermissionDenied),
        SourceOutcome::Unavailable(FailureKind::TimedOut),
    ] {
        assert_eq!(
            empty_state_failure(&[source(outcome)], false),
            Some(match outcome {
                SourceOutcome::Unavailable(kind) | SourceOutcome::Partial(kind) => kind,
                _ => unreachable!(),
            }),
            "an empty list from a failed source must expose the typed failure"
        );
    }
}

#[test]
fn available_or_empty_sources_stay_a_plain_empty_list() {
    assert_eq!(
        empty_state_failure(&[source(SourceOutcome::Available)], false),
        None
    );
    assert_eq!(
        empty_state_failure(&[source(SourceOutcome::Empty)], false),
        None
    );
    assert_eq!(empty_state_failure(&[], false), None);
    // A healthy source alongside a failed one still exposes the failure.
    assert_eq!(
        empty_state_failure(
            &[
                source(SourceOutcome::Available),
                source(SourceOutcome::Unavailable(FailureKind::MissingDependency)),
            ],
            false,
        ),
        Some(FailureKind::MissingDependency)
    );
}

#[test]
fn a_search_without_matches_never_masquerades_as_source_failure() {
    assert_eq!(
        empty_state_failure(
            &[source(SourceOutcome::Unavailable(
                FailureKind::MissingDependency
            ))],
            true,
        ),
        None,
        "an active query must keep the 'no match' answer"
    );
}

#[test]
fn empty_state_icon_preserves_no_rows_vs_no_match_semantics() {
    assert_eq!(empty_state_icon(""), IconId::Applications);
    assert_eq!(empty_state_icon("kernel"), IconId::Search);
}

/// Parity: the banner's render input is exactly the neutral VM's merged
/// kind — partial answers read as "data may be missing", unanswered
/// sources (failed OR stale-but-visible) read as "unavailable".
#[test]
fn banner_title_follows_the_neutral_merged_kind() {
    use super::banner_title_key;
    use taskmanager_application::{SourceStateKind, merge_source_lines};

    let partial = source(SourceOutcome::Partial(FailureKind::TimedOut));
    let failed = source(SourceOutcome::Unavailable(FailureKind::Unsupported));
    let mut with_rows = failed.clone();
    with_rows.item_count = 5;

    let partial_kind = merge_source_lines(&[partial]).map(|merged| merged.kind);
    assert_eq!(partial_kind, Some(SourceStateKind::Degraded));
    assert_eq!(
        partial_kind.map(banner_title_key),
        Some("source.partial_title")
    );

    for fixture in [failed, with_rows] {
        let expected = if fixture.item_count > 0 {
            SourceStateKind::Stale
        } else {
            SourceStateKind::Failed
        };
        let kind = merge_source_lines(&[fixture]).map(|merged| merged.kind);
        assert_eq!(kind, Some(expected));
        assert_eq!(
            kind.map(banner_title_key),
            Some("source.unavailable_title"),
            "unanswered sources keep the unavailable title"
        );
    }

    assert_eq!(merge_source_lines(&[]), None);
}

/// Parity for the three list pages (Services / Startup / Users): every
/// page renders its page-top notice through [`super::source_notice`],
/// whose title comes from [`super::banner_title_key`] over the neutral
/// `merge_source_lines` kind — the same shared fold for all three. Given
/// each page's mixed fixture, the banner decision must agree with the
/// neutral VM entry by entry: headline kind, title family, typed notice,
/// and retry policy. Healthy-only input must produce no banner at all.
#[test]
fn page_top_notice_agrees_with_the_neutral_merge_for_every_list_page() {
    use super::banner_title_key;
    use taskmanager_application::{SourceNotice, SourceStateKind, merge_source_lines};

    fn page_source(provider: &'static str, outcome: SourceOutcome, rows: usize) -> SourceStatus {
        SourceStatus {
            provider: ProviderId::borrowed(provider),
            outcome,
            item_count: rows,
        }
    }

    // One mixed fixture per page family. Services: a healthy unit list
    // beside a timed-out manager-stats provider (partial answer, retryable
    // now). Startup: desktop entries gone while their rows stay visible
    // next to a healthy systemd user-unit source (stale, retry later).
    // Users: loginctl missing entirely (hard failure, no rows, needs a
    // capability change instead of a retry loop).
    let cases = [
        (
            "services",
            vec![
                page_source("systemd.units", SourceOutcome::Available, 5),
                page_source(
                    "systemd.manager",
                    SourceOutcome::Partial(FailureKind::TimedOut),
                    0,
                ),
            ],
            SourceStateKind::Degraded,
            SourceNotice::Partial(FailureKind::TimedOut),
            "source.partial_title",
            true,
        ),
        (
            "startup",
            vec![
                page_source(
                    "xdg.autostart",
                    SourceOutcome::Unavailable(FailureKind::TemporarilyUnavailable),
                    4,
                ),
                page_source("systemd.user", SourceOutcome::Available, 2),
            ],
            SourceStateKind::Stale,
            SourceNotice::Unavailable(FailureKind::TemporarilyUnavailable),
            "source.unavailable_title",
            true,
        ),
        (
            "users",
            vec![page_source(
                "loginctl",
                SourceOutcome::Unavailable(FailureKind::MissingDependency),
                0,
            )],
            SourceStateKind::Failed,
            SourceNotice::Unavailable(FailureKind::MissingDependency),
            "source.unavailable_title",
            false,
        ),
    ];

    for (page, fixture, expected_kind, expected_notice, expected_key, expected_retry) in cases {
        let merged = merge_source_lines(&fixture).unwrap_or_else(|| panic!("{page} must headline"));
        assert_eq!(merged.kind, expected_kind, "{page} headline kind");
        assert_eq!(merged.notice, expected_notice, "{page} typed notice");
        assert_eq!(banner_title_key(merged.kind), expected_key, "{page} title");
        assert_eq!(
            merged.notice.is_retryable(),
            expected_retry,
            "{page} retry affordance"
        );
    }

    // A healthy source never headlines a page-top notice on any page.
    for provider in ["systemd.units", "xdg.autostart", "loginctl"] {
        assert_eq!(
            merge_source_lines(&[page_source(provider, SourceOutcome::Available, 3)]),
            None,
            "{provider} answered; no banner"
        );
    }
}
