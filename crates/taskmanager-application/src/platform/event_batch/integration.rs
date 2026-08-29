//! Correlated shell and desktop-appearance events appended to the bounded `PlatformEventBatch`.

use taskmanager_core::core::setup::SetupScriptEvent;

use super::super::{DesktopAppearanceEvent, ShellEvent};
use super::{CorrelatedEvent, PlatformEventBatch, PlatformEventContext};

pub type CorrelatedShellEvent = CorrelatedEvent<ShellEvent>;
pub type CorrelatedSetupScriptEvent = CorrelatedEvent<SetupScriptEvent>;
pub type CorrelatedDesktopAppearanceEvent = CorrelatedEvent<DesktopAppearanceEvent>;

pub(super) fn push_shell(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: ShellEvent,
) {
    batch
        .shell_events
        .push(CorrelatedEvent::new(context, event));
}

pub(super) fn push_desktop_appearance(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: DesktopAppearanceEvent,
) {
    batch
        .desktop_appearance_events
        .push(CorrelatedEvent::new(context, event));
}

pub(super) fn push_setup_script(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: SetupScriptEvent,
) {
    batch
        .setup_script_events
        .push(CorrelatedEvent::new(context, event));
}

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_event_batch_integration_tests.rs"]
mod tests;
