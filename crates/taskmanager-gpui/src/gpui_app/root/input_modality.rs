//! Per-window input modality and GPUI capture adapters.
//!
//! `FocusHandle` carries no input origin, so TaskForest's keyboard-only
//! focus-visible policy is derived from a `RootView`-owned state machine. Each
//! window owns one small state machine. Capture listeners update it before descendant
//! controls handle the same event, which means a Tab focus move renders its ring
//! immediately while a pointer-driven focus change does not.
//!
//! The modality vocabulary and the focus-visible rule belong to the shared
//! contract (`taskmanager_ui_contract::InputModality`); this module owns only
//! the GPUI capture wiring.

use super::RootView;
use gpui::{Context, KeyDownEvent, MouseDownEvent, Window};
use taskmanager_ui_contract::InputModality;

impl RootView {
    /// Install the input and focus-lifecycle observers for this exact window.
    /// The pre-action keyboard interceptor complements element capture for keys
    /// (notably Tab) that GPUI resolves to actions first. The activation and
    /// focus-lost observers clear stale rings when the window blurs or its
    /// focused target is unmounted. All subscriptions and modality remain
    /// Root-owned; the WindowId filter prevents cross-window keyboard updates.
    pub(super) fn ensure_input_modality_observers(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.input_modality_key_subscription.is_none() {
            let window_id = window.window_handle().window_id();
            let weak = cx.entity().downgrade();
            self.input_modality_key_subscription =
                Some(cx.intercept_keystrokes(move |_event, event_window, cx| {
                    if event_window.window_handle().window_id() != window_id {
                        return;
                    }
                    let _ = weak.update(cx, |view, cx| {
                        if view.input_modality.transition(InputModality::Keyboard) {
                            cx.notify();
                        }
                    });
                }));
        }

        if self.input_modality_activation_subscription.is_none() {
            self.input_modality_activation_subscription =
                Some(cx.observe_window_activation(window, |view, window, cx| {
                    if !window.is_window_active()
                        && view.input_modality.transition(InputModality::Programmatic)
                    {
                        cx.notify();
                    }
                }));
        }

        if self.input_modality_focus_lost_subscription.is_none() {
            self.input_modality_focus_lost_subscription =
                Some(cx.on_focus_lost(window, |view, _window, cx| {
                    if view.input_modality.transition(InputModality::Programmatic) {
                        cx.notify();
                    }
                }));
        }
    }

    pub(super) fn capture_input_modality_key_down(
        view: &mut Self,
        _event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if view.input_modality.transition(InputModality::Keyboard) {
            cx.notify();
        }
    }

    pub(super) fn capture_input_modality_mouse_down(
        view: &mut Self,
        _event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if view.input_modality.transition(InputModality::Pointer) {
            cx.notify();
        }
    }
}
