//! The shared gate-confirmation vocabulary (2026-08-17 uplift): every armed
//! destructive-action gate — service control, process batch, session
//! control, startup control — owns 'y'/'n' directly in the shell, so
//! frontends route armed-gate keys through the shared state machine instead
//! of carrying per-frontend copies (the TUI's four local blocks were
//! collapsed into this vocabulary). y emits the frozen request through the
//! gate's own confirm path; n dismisses without submitting; every other
//! character is swallowed — a confirmation dialog owns the keyboard.

use taskmanager_application::{AppAction, ConfirmationKind, PlatformEffect};

use super::ShellApp;

/// The outcome of routing one character against the armed gates.
#[derive(Debug)]
pub(super) enum GateRouting {
    /// No gate is armed; the caller continues with its own precedence.
    NotArmed,
    /// A gate consumed the character; the payload is the emitted effect, if
    /// any ('y' emits, 'n' and swallows produce `None`).
    Consumed(Option<PlatformEffect>),
}

/// Route one character while the sole confirmation gate is armed. The enum
/// match is exhaustive, so adding a branch cannot silently inherit another
/// branch's confirm action.
pub(super) fn route_armed_gate(app: &mut ShellApp, character: char) -> GateRouting {
    let Some(kind) = app.confirmation_kind() else {
        return GateRouting::NotArmed;
    };
    GateRouting::Consumed(match character {
        'y' | 'Y' => match kind {
            ConfirmationKind::EndTask => app.confirm_end_task(),
            ConfirmationKind::ProcessBatch => app.confirm_process_batch(),
            ConfirmationKind::ServiceControl => app.apply_action(AppAction::ConfirmServiceControl),
            ConfirmationKind::StartupControl => app.confirm_startup_control(),
            ConfirmationKind::SessionControl => app.confirm_session_control(),
            ConfirmationKind::SmartSelfTest => app.confirm_confirmation(kind),
        },
        'n' | 'N' => {
            app.dismiss_overlay();
            None
        }
        _ => None,
    })
}
