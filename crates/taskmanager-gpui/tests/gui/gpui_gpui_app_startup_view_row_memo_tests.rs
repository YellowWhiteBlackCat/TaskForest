use super::*;
use crate::gpui_app::root::RootView;
use gpui::TestAppContext;
use std::rc::Rc;
use taskmanager_application::i18n;
use taskmanager_core::core::startup::StartupEntry;
use taskmanager_shell::{InfoSortCol, InfoTable, SortDir};
use taskmanager_theme::Theme;

fn entry(name: &str, enabled: bool) -> StartupEntry {
    use taskmanager_core::core::startup::{
        StartupControlPolicy, StartupImpact, StartupScope, StartupSource,
    };

    StartupEntry {
        id: format!("desktop:{name}.desktop").into(),
        name: name.to_owned(),
        exec: format!("/usr/bin/{name}"),
        enabled,
        source: StartupSource::DesktopEntry,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: format!("{name}.desktop").into(),
        impact: StartupImpact::Low,
        impact_evidence: taskmanager_core::core::startup::StartupImpactEvidence::Measured {
            duration_ms: 10,
        },
    }
}

#[gpui::test]
fn startup_rows_reuse_the_projection_until_an_input_changes(cx: &mut TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    root.update(cx, |view, _cx| {
        view.replace_startup_for_test(
            vec![
                entry("Zeta", true),
                entry("alpha", false),
                entry("Beta", true),
            ],
            Vec::new(),
        );
        let first = view.startup_rows();
        // No column picked yet: the shell sort slot is `None`, so the
        // memoized order is the provider order (single source: the shell
        // `InventorySorts` slot, replacing the old local enabled-first
        // fixed sort).
        let names: Vec<&str> = first.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Zeta", "alpha", "Beta"]);
        let second = view.startup_rows();
        assert!(
            Rc::ptr_eq(&first, &second),
            "unchanged inputs must reuse the cached projection"
        );

        view.advance_startup_generation_for_test();
        let rebuilt = view.startup_rows();
        assert!(!Rc::ptr_eq(&first, &rebuilt));

        // A header-click sort change invalidates the memo without any
        // generation bump and orders the rows through the shell slot.
        view.apply_table_sort(
            InfoTable::Startup,
            Some(InfoSortCol::Status),
            taskmanager_ui::data::table::SortState::Ascending,
        );
        let ordered = view.startup_rows();
        assert!(!Rc::ptr_eq(&rebuilt, &ordered));
        let names: Vec<&str> = ordered.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Zeta", "Beta", "alpha"],
            "ascending status is enabled-first (stable within equals)"
        );
    });
}

/// The startup header-sort chain, end to end: the widget's `perform_sort`
/// (exactly what the header icon invokes) emits `SortChanged`, the
/// subscriber applies the post-cycle state verbatim onto the shell-owned
/// `InventorySorts` slot, and the memo — keyed on that slot — rebuilds
/// with the new row order.
#[gpui::test]
fn header_sort_click_flows_through_the_shell_slot_and_reorders_the_memo(cx: &mut TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    let table = root.update(cx, |view, cx| {
        view.replace_startup_for_test(vec![entry("Zeta", true), entry("alpha", false)], Vec::new());
        init_table_entity(Theme::dark(), cx)
    });
    // First click on the Name column (ix 1): widget cycles to Descending.
    table.update(cx, |table, cx| table.perform_sort(1, cx));
    root.update(cx, |view, _| {
        assert_eq!(
            view.inventory_sort(InfoTable::Startup),
            Some((InfoSortCol::Name, SortDir::Desc))
        );
        let rows = view.startup_rows();
        let names: Vec<&str> = rows.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "Zeta"]);
    });
}

#[test]
fn startup_source_detail_names_the_missing_blame_facet() {
    let detail = startup_source_detail(&[taskmanager_core::core::SourceStatus {
        provider: taskmanager_core::core::ProviderId::borrowed("linux.startup.systemd-blame"),
        outcome: taskmanager_core::core::SourceOutcome::Partial(
            taskmanager_core::core::FailureKind::ProviderFault,
        ),
        item_count: 3,
    }])
    .expect("failed startup source should explain its scope");
    assert!(detail.contains(i18n::t("startup.source_field_blame")));
    assert!(detail.contains("linux.startup.systemd-blame"));
    assert!(detail.contains(i18n::t("startup.source_inventory_available")));
}

#[test]
fn startup_source_detail_does_not_claim_complete_rows_for_inventory_failure() {
    let detail = startup_source_detail(&[taskmanager_core::core::SourceStatus {
        provider: taskmanager_core::core::ProviderId::borrowed("linux.startup.systemd-user"),
        outcome: taskmanager_core::core::SourceOutcome::Unavailable(
            taskmanager_core::core::FailureKind::Rejected,
        ),
        item_count: 0,
    }])
    .expect("failed startup inventory should explain its scope");
    assert!(detail.contains(i18n::t("startup.source_field_systemd")));
    assert!(detail.contains(i18n::t("startup.source_inventory_degraded")));
    assert!(!detail.contains(i18n::t("startup.source_inventory_available")));
}
