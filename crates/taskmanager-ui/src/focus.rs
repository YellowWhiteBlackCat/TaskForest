#![forbid(unsafe_code)]
//! GPUI adapter for the toolkit-neutral modal focus policy. The reusable
//! component boundary is defined by `docs/UI_COMPONENT_ARCHITECTURE.md`.
//!
//! Owns: the per-window modal scope + trigger registry, the bounded Tab
//! cycle inside a modal, and focus restoration with token validation. The
//! ESC chain routing (`esc_chain_target`) is a pure query over the active
//! layer kinds; the `LayerStack` (overlays/layer_stack.rs) drives it.

use std::collections::HashMap;

use gpui::{
    App, Div, FocusHandle, Global, InteractiveElement, KeyBinding, Stateful, Window, WindowId,
    actions,
};
use taskmanager_ui_contract::{FocusCycleStep, FocusRestoreToken, FocusTarget, ModalFocusPolicy};

/// Key context that marks an element as inside a modal scope. `trap_modal`
/// attaches it; key bindings for the modal Tab cycle live under it.
pub const MODAL_CONTEXT: &str = "TaskManagerModal";

/// Bounded scan budget for one forward/reverse traversal attempt.
const MODAL_FOCUS_POLICY: ModalFocusPolicy = ModalFocusPolicy::contained(512);

actions!(taskmanager_modal, [ModalTab, ModalTabPrev]);

/// Which layer kind should receive an Escape first (top-most first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalEscTarget {
    /// The top-most layer is a non-modal popup (menu / dropdown / toast).
    Popup,
    /// The top-most layer is a modal dialog.
    Dialog,
    /// No layer is open; Escape belongs to the window.
    Window,
}

#[derive(Clone, Copy)]
enum TraversalDirection {
    Next,
    Previous,
}

struct DialogFocusRegistry {
    by_window: HashMap<WindowId, DialogFocusState>,
    next_restore_token: u64,
}

impl Default for DialogFocusRegistry {
    fn default() -> Self {
        Self {
            by_window: HashMap::new(),
            next_restore_token: 1,
        }
    }
}

impl DialogFocusRegistry {
    fn mint_restore_token(&mut self) -> FocusRestoreToken {
        let token = FocusRestoreToken::new(self.next_restore_token);
        self.next_restore_token = if self.next_restore_token == u64::MAX {
            1
        } else {
            self.next_restore_token + 1
        };
        token
    }
}

impl Global for DialogFocusRegistry {}

#[derive(Clone)]
struct DialogFocusState {
    trigger: Option<(FocusRestoreToken, FocusHandle)>,
    scope: FocusHandle,
}

/// Bind the modal Tab/Shift-Tab actions and ensure the registry exists.
pub fn ensure_support(cx: &mut App) {
    if cx.has_global::<DialogFocusRegistry>() {
        return;
    }
    cx.set_global(DialogFocusRegistry::default());
    cx.bind_keys([
        KeyBinding::new("tab", ModalTab, Some(MODAL_CONTEXT)),
        KeyBinding::new("shift-tab", ModalTabPrev, Some(MODAL_CONTEXT)),
    ]);
}

/// Whether the window's focus path currently contains the modal context.
pub fn modal_context_focused(window: &Window) -> bool {
    window
        .context_stack()
        .iter()
        .any(|context| context.contains(MODAL_CONTEXT))
}

fn apply_target(target: FocusTarget, state: Option<&DialogFocusState>, window: &mut Window) {
    match target {
        FocusTarget::ModalScope => {
            if let Some(state) = state {
                state.scope.focus(window);
            } else {
                window.blur();
            }
        }
        FocusTarget::Restore(expected_token) => {
            if let Some((_, trigger)) = state
                .and_then(|state| state.trigger.as_ref())
                .filter(|(token, _)| *token == expected_token)
            {
                trigger.focus(window);
            } else {
                window.blur();
            }
        }
        FocusTarget::Clear => window.blur(),
    }
}

fn move_focus(direction: TraversalDirection, window: &mut Window, cx: &mut App) {
    let mut cycle = MODAL_FOCUS_POLICY.begin_cycle();
    loop {
        match direction {
            TraversalDirection::Next => window.focus_next(),
            TraversalDirection::Previous => window.focus_prev(),
        }

        match cycle.observe(modal_context_focused(window)) {
            FocusCycleStep::Settled => return,
            FocusCycleStep::Continue => {}
            FocusCycleStep::Focus(target) => {
                let window_id = window.window_handle().window_id();
                let state = cx
                    .try_global::<DialogFocusRegistry>()
                    .and_then(|registry| registry.by_window.get(&window_id))
                    .cloned();
                apply_target(target, state.as_ref(), window);
                return;
            }
        }
    }
}

/// Register and focus the stable per-window modal scope. Safe to call for
/// every nested layer; the first call records the pre-modal trigger, later
/// calls reuse the same scope (the `LayerStack` layers keep their own
/// per-entry handles on top).
pub fn begin_modal(window: &mut Window, cx: &mut App) -> FocusHandle {
    ensure_support(cx);
    let window_id = window.window_handle().window_id();
    if let Some(scope) = cx
        .global::<DialogFocusRegistry>()
        .by_window
        .get(&window_id)
        .map(|state| state.scope.clone())
    {
        return scope;
    }

    let trigger_handle = window.focused(cx);
    let scope = cx.focus_handle();
    let trigger = trigger_handle.map(|handle| {
        let token = cx.global_mut::<DialogFocusRegistry>().mint_restore_token();
        (token, handle)
    });
    let state = DialogFocusState {
        trigger,
        scope: scope.clone(),
    };
    cx.global_mut::<DialogFocusRegistry>()
        .by_window
        .insert(window_id, state.clone());
    apply_target(MODAL_FOCUS_POLICY.initial_target(), Some(&state), window);
    scope
}

/// Add GPUI context tracking and typed Tab actions to a modal scope element.
pub fn trap_modal(element: Stateful<Div>, focus_scope: &FocusHandle) -> Stateful<Div> {
    element
        .key_context(MODAL_CONTEXT)
        .track_focus(focus_scope)
        .on_action(|_: &ModalTab, window, cx| {
            move_focus(TraversalDirection::Next, window, cx);
        })
        .on_action(|_: &ModalTabPrev, window, cx| {
            move_focus(TraversalDirection::Previous, window, cx);
        })
}

/// Restore the exact pre-modal focus handle, or clear focus if none was safely
/// recorded. Every close path calls this same adapter boundary.
pub fn restore_modal(window: &mut Window, cx: &mut App) {
    if !cx.has_global::<DialogFocusRegistry>() {
        return;
    }
    let window_id = window.window_handle().window_id();
    let Some(state) = cx
        .global_mut::<DialogFocusRegistry>()
        .by_window
        .remove(&window_id)
    else {
        return;
    };
    let restore_token = state.trigger.as_ref().map(|(token, _)| *token);
    apply_target(
        MODAL_FOCUS_POLICY.restore_target(restore_token),
        Some(&state),
        window,
    );
}

/// Pure routing query for the ESC chain: the top-most layer kind wins, then
/// the window. Popup first, then Dialog, then Window (absorption §1.5).
pub fn esc_chain_target(top_layer_is_modal: Option<bool>) -> ModalEscTarget {
    match top_layer_is_modal {
        Some(true) => ModalEscTarget::Dialog,
        Some(false) => ModalEscTarget::Popup,
        None => ModalEscTarget::Window,
    }
}

#[cfg(test)]
#[path = "../tests/gui/ui_focus_tests.rs"]
mod tests;
