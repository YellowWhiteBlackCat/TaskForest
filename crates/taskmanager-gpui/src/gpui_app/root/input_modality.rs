//! Per-window input modality and GPUI capture adapters.
//!
//! GPUI 0.2.2 does not attach an input origin to `FocusHandle`, so focus-visible
//! cannot be derived from the focused element itself. `RootView` instead owns one
//! small state machine per window. Capture listeners update it before descendant
//! controls handle the same event, which means a Tab focus move renders its ring
//! immediately while a pointer-driven focus change does not.

use super::RootView;
use gpui::{Context, KeyDownEvent, MouseDownEvent, Window};
/// The most recent origin capable of changing focus in this window.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InputModality {
    /// Initial state and focus changes initiated directly by application code.
    #[default]
    Programmatic,
    /// A keyboard event was observed at the root capture boundary.
    Keyboard,
    /// A pointer button was pressed inside the root surface.
    Pointer,
}

impl InputModality {
    /// Strict focus-visible policy: only keyboard modality paints an outset ring.
    pub const fn shows_focus_ring(self) -> bool {
        matches!(self, Self::Keyboard)
    }

    fn replace(&mut self, next: Self) -> bool {
        if *self == next {
            return false;
        }
        *self = next;
        true
    }

    pub(super) fn observe_keyboard(&mut self) -> bool {
        self.replace(Self::Keyboard)
    }

    pub(super) fn observe_pointer(&mut self) -> bool {
        self.replace(Self::Pointer)
    }

    /// Clear a modality decision when its window or focused target leaves the
    /// active render lifecycle.
    pub(super) fn clear_after_focus_lifecycle(&mut self) -> bool {
        self.replace(Self::Programmatic)
    }
}

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
                        if view.input_modality.observe_keyboard() {
                            cx.notify();
                        }
                    });
                }));
        }

        if self.input_modality_activation_subscription.is_none() {
            self.input_modality_activation_subscription =
                Some(cx.observe_window_activation(window, |view, window, cx| {
                    if !window.is_window_active()
                        && view.input_modality.clear_after_focus_lifecycle()
                    {
                        cx.notify();
                    }
                }));
        }

        if self.input_modality_focus_lost_subscription.is_none() {
            self.input_modality_focus_lost_subscription =
                Some(cx.on_focus_lost(window, |view, _window, cx| {
                    if view.input_modality.clear_after_focus_lifecycle() {
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
        if view.input_modality.observe_keyboard() {
            cx.notify();
        }
    }

    pub(super) fn capture_input_modality_mouse_down(
        view: &mut Self,
        _event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if view.input_modality.observe_pointer() {
            cx.notify();
        }
    }
}
