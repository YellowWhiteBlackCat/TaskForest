//! test-intent: behavior
//! ServiceAction owns both its stable transport token and its catalog key, so
//! every frontend and provider resolves the same product vocabulary.

use crate::core::services::ServiceAction;

#[test]
fn every_action_has_one_stable_token_and_one_catalog_key() {
    let actions = [
        ServiceAction::Start,
        ServiceAction::Stop,
        ServiceAction::Restart,
        ServiceAction::Enable,
        ServiceAction::Disable,
        ServiceAction::ReloadDaemon,
    ];
    let tokens: Vec<_> = actions.iter().map(|action| action.stable_token()).collect();
    let keys: Vec<_> = actions.iter().map(|action| action.i18n_key()).collect();
    assert_eq!(
        tokens,
        [
            "start",
            "stop",
            "restart",
            "enable",
            "disable",
            "reload_daemon"
        ]
    );
    assert_eq!(
        keys,
        [
            "svc.start",
            "svc.stop",
            "svc.restart",
            "svc.enable",
            "svc.disable",
            "svc.reload_daemon",
        ]
    );
    assert_eq!(
        tokens
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        actions.len()
    );
    assert_eq!(
        keys.iter().collect::<std::collections::HashSet<_>>().len(),
        actions.len()
    );
}
