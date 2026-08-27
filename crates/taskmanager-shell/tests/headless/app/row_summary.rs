//! `ShellApp::selected_row_summary` — the single Ctrl+C copy payload seam.

use super::*;
use taskmanager_application::{
    AppPage, ServiceItem, ServiceStatus, StartupControlPolicy, StartupEntry, StartupEntryId,
    StartupEntryLocator, StartupImpact, StartupImpactEvidence, StartupImpactUnknownReason,
    StartupScope, StartupSource,
};

#[test]
fn applications_summary_is_pid_tab_name() {
    let mut shell = ShellApp::default();
    shell.data.processes = Some(vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(4242)
            .name("my_daemon".into())
            .build(),
    ]);
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    shell.selected = 0;
    assert_eq!(
        shell.selected_row_summary().as_deref(),
        Some("4242\tmy_daemon")
    );
}

#[test]
fn services_and_startup_summaries_carry_typed_state() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut shell = ShellApp::default();
    shell.data.services = Some(vec![ServiceItem::from_inventory(
        "nm.service",
        "NetworkManager.service",
        ServiceStatus::Active,
        "",
        "",
        "",
        "",
    )]);
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Services));
    shell.selected = 0;
    assert_eq!(
        shell.selected_row_summary().as_deref(),
        Some("NetworkManager.service\tActive")
    );

    shell.data.services = None;
    shell.data.startup_entries = Some(vec![StartupEntry {
        id: StartupEntryId::new("appimage"),
        name: "AppImage.desktop".into(),
        exec: "/opt/AppImage".into(),
        enabled: true,
        source: StartupSource::DesktopEntry,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: StartupEntryLocator::new("appimage"),
        impact: StartupImpact::None,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented,
        },
    }]);
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Startup));
    shell.selected = 0;
    assert_eq!(
        shell.selected_row_summary().as_deref(),
        Some("AppImage.desktop\tEnabled")
    );
}

#[test]
fn pages_without_a_row_selection_return_none() {
    let mut shell = ShellApp::default();
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Performance));
    assert_eq!(shell.selected_row_summary(), None);
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::System));
    assert_eq!(shell.selected_row_summary(), None);
}
