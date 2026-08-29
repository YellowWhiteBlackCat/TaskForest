// Narrow imports (mirroring root/termination.rs): a `use super::*` glob in a
// test module pulls gpui's deeply generic prelude into the test scope and
// pushes the `#[test]` attribute macro past its recursion limit.
use super::{ServiceControlConfirmation, requires_service_confirmation};
use taskmanager_application::i18n;
use taskmanager_core::core::services::ServiceAction;
use taskmanager_core::core::startup::{
    StartupControlPolicy, StartupEntry, StartupImpact, StartupImpactEvidence,
    StartupImpactUnknownReason, StartupScope, StartupSource,
};
use taskmanager_core::core::target::ServiceId;

#[test]
fn only_destructive_service_actions_require_confirmation() {
    // Start / Enable are constructive and stay immediate.
    assert!(!requires_service_confirmation(ServiceAction::Start));
    assert!(!requires_service_confirmation(ServiceAction::Enable));
    // Stop / Restart / Disable can lock the session → gated.
    assert!(requires_service_confirmation(ServiceAction::Stop));
    assert!(requires_service_confirmation(ServiceAction::Restart));
    assert!(requires_service_confirmation(ServiceAction::Disable));
}

#[test]
fn service_intent_freezes_display_name_and_id() {
    let intent = ServiceControlConfirmation::Service {
        service_id: ServiceId::new("NetworkManager.service"),
        display_name: "NetworkManager".into(),
        action: ServiceAction::Stop,
    };
    assert!(intent.is_high_risk());
    assert_eq!(intent.confirm_label(), i18n::t("svc.stop"));
    // The message template substitutes the frozen display name.
    assert!(intent.dialog_message().contains("NetworkManager"));
}

#[test]
fn startup_disable_is_not_high_risk_but_is_gated() {
    let entry = StartupEntry {
        id: "desktop:helper.desktop".into(),
        name: "Desktop helper".into(),
        exec: "capture-desktop-helper --background".into(),
        enabled: true,
        source: StartupSource::DesktopEntry,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: "/home/<user>/.config/autostart/helper.desktop".into(),
        impact: StartupImpact::None,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented,
        },
    };
    let intent = ServiceControlConfirmation::Startup {
        entry,
        enabled: false,
    };
    // Startup toggles are gated but do not paint in the destructive accent.
    assert!(!intent.is_high_risk());
    assert_eq!(intent.confirm_label(), i18n::t("common.disable"));
    assert!(intent.dialog_message().contains("Desktop helper"));
}
