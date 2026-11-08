//! Finite Vi-style chord sequences for safe, already-owned navigation actions.
//!
//! The layer is deliberately contextual: it only arms a pending chord when the
//! first key is not already claimed by the shared shell, the TUI command
//! registry, or the Applications prefix jump. It never invents a destructive
//! process action. `gg` and `G` reuse the table-page Home/End owner; `/`
//! reuses the shared search command; `dd` clears batch marks. Every other key
//! cancels a stale pending chord and continues through the normal precedence
//! chain.
//!
//! [`advance`] is the pure state transition, while [`handle`] supplies the
//! only execution adapter. State is retained on `TuiApp` and is scoped to the
//! current page and Performance device, so context changes cannot leak a chord
//! into another surface. Replaying a completed sequence is idempotent because
//! it returns to `Idle`.

use ratatui::crossterm::event::{self, KeyEvent};
use taskmanager_application::{AppAction, AppPage, Modifiers};
use taskmanager_shell::InputDispatch;

use super::direct::direct_scope_armed;
use super::navigation::{self, is_table_page};
use crate::command_palette::TUI_LOCAL_COMMANDS;
use crate::surface::TuiInputScope;
use crate::{PerfDevice, TuiApp};

/// A pending two-key Vim chord. The captured context is part of the state,
/// rather than mutable global input state: a page or device change makes the
/// pending chord stale before it can act.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum VimPending {
    /// No sequence is waiting for its second key.
    #[default]
    Idle,
    /// `g` was accepted and `g` may request the first row.
    FirstRow { context: VimContext },
    /// `d` was accepted and `d` may clear batch marks.
    ClearMarks { context: VimContext },
}

/// The context under which a pending chord was armed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VimContext {
    pub(crate) page: AppPage,
    pub(crate) perf_device: PerfDevice,
}

impl VimContext {
    fn current(app: &TuiApp) -> Self {
        Self {
            page: app.page(),
            perf_device: app.perf_device,
        }
    }
}

/// Normalized sequence input. Modifier-bearing keys other than the uppercase
/// `G` form are intentionally classified as `Other`, which cancels a chord
/// without hiding the normal input owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VimKey {
    G,
    ShiftG,
    D,
    Slash,
    Other,
}

/// Contextual availability supplied by the host of the pure state machine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct VimAvailability {
    /// The first `g` may arm, and a second `g` may move to the first row.
    pub(crate) first_row: bool,
    /// `G` may move to the last row.
    pub(crate) last_row: bool,
    /// The first `d` may arm, and a second `d` may clear marks.
    pub(crate) clear_marks: bool,
    /// `/` may focus the current page's search field.
    pub(crate) search: bool,
}

/// The action a completed sequence asks the adapter to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VimAction {
    FirstRow,
    LastRow,
    ClearMarks,
    Search,
}

/// The result of one pure transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VimTransition {
    pub(crate) pending: VimPending,
    pub(crate) action: Option<VimAction>,
    pub(crate) consumed: bool,
}

impl VimTransition {
    const fn ignored() -> Self {
        Self {
            pending: VimPending::Idle,
            action: None,
            consumed: false,
        }
    }

    const fn waiting(pending: VimPending) -> Self {
        Self {
            pending,
            action: None,
            consumed: true,
        }
    }

    const fn action(action: VimAction) -> Self {
        Self {
            pending: VimPending::Idle,
            action: Some(action),
            consumed: true,
        }
    }
}

/// Advance the finite chord state machine. The pending context is discarded if
/// the current context changed, preventing a stale first key from completing
/// against a different table or performance device.
#[must_use]
pub(crate) fn advance(
    pending: VimPending,
    key: VimKey,
    context: VimContext,
    available: VimAvailability,
) -> VimTransition {
    let pending = match pending {
        VimPending::Idle => VimPending::Idle,
        VimPending::FirstRow {
            context: pending_context,
        } if pending_context == context => pending,
        VimPending::ClearMarks {
            context: pending_context,
        } if pending_context == context => pending,
        _ => VimPending::Idle,
    };

    match (pending, key) {
        (VimPending::FirstRow { .. }, VimKey::G) if available.first_row => {
            VimTransition::action(VimAction::FirstRow)
        }
        (VimPending::ClearMarks { .. }, VimKey::D) if available.clear_marks => {
            VimTransition::action(VimAction::ClearMarks)
        }
        (VimPending::Idle, VimKey::G) if available.first_row => {
            VimTransition::waiting(VimPending::FirstRow { context })
        }
        (VimPending::Idle, VimKey::D) if available.clear_marks => {
            VimTransition::waiting(VimPending::ClearMarks { context })
        }
        (VimPending::Idle, VimKey::ShiftG) if available.last_row => {
            VimTransition::action(VimAction::LastRow)
        }
        (VimPending::Idle, VimKey::Slash) if available.search => {
            VimTransition::action(VimAction::Search)
        }
        _ => VimTransition::ignored(),
    }
}

/// The TUI input-system adapter inserted before the shared character router.
/// It yields to surfaces that own input, and otherwise only consumes a Vim key
/// whose first chord is not claimed by an existing binding owner.
pub(crate) fn handle(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    if app.input_scope() != TuiInputScope::Content {
        app.vim_pending = VimPending::default();
        return InputDispatch::Unhandled;
    }

    let context = VimContext::current(app);
    let available = availability(app, key.modifiers);
    let transition = advance(app.vim_pending, classify(key), context, available);
    app.vim_pending = transition.pending;

    if let Some(action) = transition.action {
        return execute(app, action);
    }
    if transition.consumed {
        return InputDispatch::Consumed;
    }
    InputDispatch::Unhandled
}

fn classify(key: &KeyEvent) -> VimKey {
    if key
        .modifiers
        .intersects(event::KeyModifiers::CONTROL | event::KeyModifiers::ALT)
    {
        return VimKey::Other;
    }
    match key.code {
        event::KeyCode::Char('g') if key.modifiers.contains(event::KeyModifiers::SHIFT) => {
            VimKey::ShiftG
        }
        event::KeyCode::Char('g') => VimKey::G,
        event::KeyCode::Char('G') => VimKey::ShiftG,
        event::KeyCode::Char('d') if !key.modifiers.contains(event::KeyModifiers::SHIFT) => {
            VimKey::D
        }
        event::KeyCode::Char('/') => VimKey::Slash,
        _ => VimKey::Other,
    }
}

fn availability(app: &TuiApp, key_modifiers: event::KeyModifiers) -> VimAvailability {
    let page = app.page();
    let can_jump = is_table_page(page);
    VimAvailability {
        first_row: can_jump && !character_is_claimed(app, 'g', key_modifiers),
        last_row: can_jump && !character_is_claimed(app, 'G', key_modifiers),
        clear_marks: !app.selected_identities().is_empty()
            && !character_is_claimed(app, 'd', key_modifiers),
        search: matches!(page, AppPage::Applications | AppPage::Services)
            && !character_is_claimed(app, '/', key_modifiers),
    }
}

/// Whether one bare character is already a live binding in this context.
/// This keeps the Vim layer from racing with an existing command owner.
fn character_is_claimed(app: &TuiApp, character: char, key_modifiers: event::KeyModifiers) -> bool {
    if app.page() == AppPage::Applications
        && character.is_ascii_alphanumeric()
        && !matches!(character, 'q' | '?' | 's' | 'S' | 'T')
        && !key_modifiers.intersects(event::KeyModifiers::CONTROL | event::KeyModifiers::ALT)
    {
        return true;
    }

    let mut buffer = [0_u8; 4];
    let pressed = character.encode_utf8(&mut buffer);
    let modifiers = modifiers(key_modifiers);
    TUI_LOCAL_COMMANDS.iter().any(|command| {
        command.binding.shortcut == pressed
            && command
                .direct
                .iter()
                .any(|arm| direct_scope_armed(app, arm.scope, modifiers))
    })
}

fn execute(app: &mut TuiApp, action: VimAction) -> InputDispatch {
    match action {
        VimAction::FirstRow => navigation::handle_table_navigation(
            app,
            &KeyEvent::new(event::KeyCode::Home, event::KeyModifiers::NONE),
        ),
        VimAction::LastRow => navigation::handle_table_navigation(
            app,
            &KeyEvent::new(event::KeyCode::End, event::KeyModifiers::NONE),
        ),
        VimAction::ClearMarks => {
            app.clear_selected_rows();
            InputDispatch::Consumed
        }
        VimAction::Search => InputDispatch::consumed(app.apply_action(AppAction::FocusSearch)),
    }
}

fn modifiers(key_modifiers: event::KeyModifiers) -> Modifiers {
    Modifiers::new(
        key_modifiers.contains(event::KeyModifiers::CONTROL),
        key_modifiers.contains(event::KeyModifiers::ALT),
        key_modifiers.contains(event::KeyModifiers::SHIFT),
        key_modifiers.contains(event::KeyModifiers::SUPER),
    )
}

#[cfg(test)]
#[path = "../../../tests/headless/runtime/vim_state_tests.rs"]
mod tests;
