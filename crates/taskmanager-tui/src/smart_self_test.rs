//! TUI guards for the shared SMART self-test confirmation.

use taskmanager_core::core::smart::self_test::SmartSelfTestPhase;
use taskmanager_core::core::system_health::SmartSelfTestIntent;

use crate::TuiApp;

/// Whether the canonical report says this exact frozen disk lifecycle is
/// already running a self-test. The provider locator is deliberately not
/// identity: only `device_id + device_generation` can match the target.
#[must_use]
pub(crate) fn target_is_running(app: &TuiApp, target: &SmartSelfTestIntent) -> bool {
    let (projection, _) = app.projection().smart_projection();
    projection.observations().iter().any(|observation| {
        observation.device_id.as_str() == target.device_id.as_str()
            && observation.device_generation == target.device_generation
            && observation.report.phase == SmartSelfTestPhase::Running
    })
}

/// Whether the currently frozen confirmation target became Running after the
/// gate was armed.
#[must_use]
pub(crate) fn pending_target_is_running(app: &TuiApp) -> bool {
    app.shell
        .pending_smart_self_test()
        .is_some_and(|target| target_is_running(app, target))
}
