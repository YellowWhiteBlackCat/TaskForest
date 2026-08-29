use super::super::components::banner_title_key;
use super::super::tests::filtered_services;
use super::*;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};

/// Unsorted/unfiltered rows carry their provider-order position as
/// `source_index` (the identity the action messages resolve).
#[test]
fn service_rows_carry_provider_order_indices_by_default() {
    let shell = taskmanager_shell::demo_app();
    let provider = shell.projection().services.as_deref().unwrap_or(&[]);
    for (position, row) in service_rows(&shell).into_iter().enumerate() {
        assert_eq!(row.source_index, position);
        assert_eq!(provider[row.source_index].name, row.name);
    }
}

/// The visible-row → service-identity seam behind the action buttons:
/// `Message::RequestServiceAction` resolves its index against
/// `data.services` (provider order), so a sorted + filtered view must
/// still map each visible row back to its own service — the visual
/// position would otherwise authorize Start/Stop on a neighbor.
#[test]
fn service_action_identity_survives_sort_and_filter() {
    let mut shell = taskmanager_shell::demo_app();
    // Name sort toggled to Desc reverses the fixture order; the
    // "network" filter then drops every row whose name AND description
    // both miss — the exact reorder + shrink scenario from the audit.
    shell.set_info_sort(InfoTable::Services, InfoSortCol::Name);
    shell.set_info_sort(InfoTable::Services, InfoSortCol::Name);
    let provider = shell.projection().services.as_deref().unwrap_or(&[]);
    let rows = service_rows(&shell);
    let visible = filtered_services(&rows, "network");

    let names: Vec<&str> = visible.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(
        names,
        ["systemd-timesyncd.service", "NetworkManager.service"]
    );
    // Visual row 0 is provider row 3, NOT provider row 0 — the audit's
    // wrong-target case when a visual index reached the effect arm.
    assert_eq!(visible[0].source_index, 3);
    assert_eq!(visible[1].source_index, 0);
    for row in &visible {
        assert_eq!(provider[row.source_index].name, row.name);
    }
}

/// The Services name-cells highlight by the SAME page-local query that
/// filters the rows — never the shared Processes-page `shell.query`. With
/// both queries set to different values, every row the services box keeps
/// must carry a name match under that query, while the Processes query
/// finds nothing in it (the old wiring highlighted by `shell.query`, so
/// the services box filtered the rows but highlighted none of them).
#[test]
fn services_name_highlight_follows_the_page_local_query_not_the_shared_one() {
    let shell = taskmanager_shell::demo_app();
    let rows = service_rows(&shell);
    let services_query = "timesync";
    let shared_query = "NetworkManager";

    let visible = filtered_services(&rows, services_query);
    assert_eq!(
        visible.len(),
        1,
        "the demo fixture keeps exactly the timesync service under this query"
    );
    for row in &visible {
        let matched_by_page_query = highlight::highlight_segments(&row.name, services_query)
            .into_iter()
            .any(|(_, matched)| matched);
        assert!(
            matched_by_page_query,
            "name {} must highlight by the services query",
            row.name
        );
        let matched_by_shared_query = highlight::highlight_segments(&row.name, shared_query)
            .into_iter()
            .any(|(_, matched)| matched);
        assert!(
            !matched_by_shared_query,
            "the Processes-page query {} must not highlight services rows",
            shared_query
        );
    }

    // The full page composes with both queries live and differing.
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::ServicesSearchChanged(services_query.into()));
    app.shell.query = shared_query.into();
    app.shell.open_search();
    let _ = services_page(&app);
}

/// Parity: the shared page-top banner (Services / Startup / Users pages)
/// folds its sources through the neutral `merge_source_lines` — the
/// headline kind drives the title family exactly like GPUI's
/// `banner_title_key`, the merged notice keeps the typed reason and retry
/// policy, and healthy-only input renders no banner at all.
#[test]
fn page_top_banner_agrees_with_the_neutral_merge_for_every_list_page() {
    use taskmanager_application::{SourceNotice, SourceStateKind, merge_source_lines};
    use taskmanager_core::core::failure::FailureKind;
    use taskmanager_core::core::identity::ProviderId;

    fn page_source(provider: &'static str, outcome: SourceOutcome, rows: usize) -> SourceStatus {
        SourceStatus {
            provider: ProviderId::borrowed(provider),
            outcome,
            item_count: rows,
        }
    }

    // Mixed fixture per page family: services degrade (partial stats next
    // to a healthy unit list), startup goes stale (desktop entries gone,
    // rows still visible), users hard-fail (loginctl missing, no rows).
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

    let app = crate::IcedApp::demo();
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
        // Both shared surfaces — the empty-state panel and the page-top
        // banner above usable rows — render from that same fold.
        assert!(
            source_state_panel(
                app.theme(),
                Some(fixture.as_slice()),
                RefreshRequest::Services
            )
            .is_some(),
            "{page} empty-state panel must render"
        );
        assert!(
            source_notice_banner(
                app.theme(),
                Some(fixture.as_slice()),
                RefreshRequest::Services
            )
            .is_some(),
            "{page} page-top banner must render"
        );
    }

    // A healthy source never headlines a banner or an empty-state panel.
    for provider in ["systemd.units", "xdg.autostart", "loginctl"] {
        let healthy = vec![page_source(provider, SourceOutcome::Available, 3)];
        assert_eq!(
            merge_source_lines(&healthy),
            None,
            "{provider} answered; nothing to headline"
        );
        assert!(
            source_state_panel(app.theme(), Some(&healthy), RefreshRequest::Services).is_none()
        );
        assert!(
            source_notice_banner(app.theme(), Some(&healthy), RefreshRequest::Services).is_none()
        );
    }
}
