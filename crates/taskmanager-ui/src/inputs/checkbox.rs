#![forbid(unsafe_code)]
//! Checkbox: typed state + builder + element, keyboard Space/Enter, focus
//! ring from `palette.ring`.

use std::rc::Rc;

use crate::OptCallback1;
use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClickEvent, Context, ElementId, Entity, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled,
    Window, div, px,
};
use taskmanager_theme::Palette;
use taskmanager_theme::tokens;

/// Typed checkbox event payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckboxEvent {
    /// The checkbox toggled to a new state.
    Toggled { checked: bool },
}

/// Checkbox state: the canonical `checked` flag lives here.
pub struct CheckboxState {
    focus_handle: FocusHandle,
    checked: bool,
}

impl CheckboxState {
    /// Create a checkbox state, initially unchecked.
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            checked: false,
        }
    }

    /// The focus handle backing this checkbox.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Current checked state.
    pub fn is_checked(&self) -> bool {
        self.checked
    }

    /// Set the checked state directly and notify.
    pub fn set_checked(&mut self, checked: bool, cx: &mut Context<Self>) {
        if self.checked != checked {
            self.checked = checked;
            cx.notify();
        }
    }

    /// Flip and notify.
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.checked = !self.checked;
        cx.notify();
    }
}

/// Builder for one rendered checkbox (box + optional label).
#[derive(IntoElement)]
pub struct Checkbox {
    state: Entity<CheckboxState>,
    palette: Palette,
    label: Option<SharedString>,
    disabled: bool,
    on_change: OptCallback1<bool>,
}

impl Checkbox {
    /// Build a checkbox bound to `state`.
    pub fn new(state: Entity<CheckboxState>, palette: Palette) -> Self {
        Self {
            state,
            palette,
            label: None,
            disabled: false,
            on_change: None,
        }
    }

    /// Optional label to the right of the box.
    #[must_use]
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Disable pointer + keyboard interaction.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Change handler: receives the NEW checked state.
    #[must_use]
    pub fn on_change(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Checkbox {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (checked, focus_handle) = self
            .state
            .read_with(cx, |state, _| (state.checked, state.focus_handle.clone()));
        let palette = self.palette;
        let disabled = self.disabled;
        let on_change = self.on_change.clone();
        let state = self.state.clone();
        let id = ElementId::named_usize(
            "tm-checkbox",
            self.state.entity_id().as_non_zero_u64().get() as usize,
        );

        let box_ = div()
            .size(px(18.0))
            .rounded(crate::theme_binding::absolute(palette.small_radius))
            .border_1()
            .border_color(crate::theme_binding::hsla(if checked {
                palette.accent
            } else {
                palette.border
            }))
            .bg(crate::theme_binding::fill(if checked {
                palette.accent
            } else {
                palette.surface
            }))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer();

        // Check mark: an accent-on-accent tick (colors stay palette-derived).
        let mark = div()
            .size(px(10.0))
            .rounded(crate::theme_binding::absolute(palette.xsmall_radius))
            .bg(crate::theme_binding::fill(if checked {
                taskmanager_theme::color::on_accent(palette.accent)
            } else {
                palette.surface
            }));

        let focus_handle = focus_handle.clone().tab_stop(!disabled);

        let mut element = div()
            .id(id)
            .debug_selector(|| "tm-checkbox".into())
            .track_focus(&focus_handle)
            .flex()
            .flex_row()
            .items_center()
            .gap(crate::theme_binding::definite_length(tokens::SPACE_8))
            .cursor_pointer()
            .when(disabled, |el| el.opacity(0.5))
            .focus(|style| style.border_color(crate::theme_binding::hsla(palette.ring)))
            .border_1()
            .rounded(px(4.0))
            .child(box_.child(mark))
            .on_key_down({
                let state = state.clone();
                let on_change = on_change.clone();
                move |event: &KeyDownEvent, window, cx| {
                    if disabled {
                        return;
                    }
                    let stroke = &event.keystroke;
                    let unmodified = !stroke.modifiers.modified();
                    if unmodified && matches!(stroke.key.as_str(), "enter" | "space") {
                        cx.stop_propagation();
                        let new_checked = !checked;
                        state.update(cx, |state, cx| {
                            state.set_checked(new_checked, cx);
                        });
                        if let Some(on_change) = &on_change {
                            on_change(new_checked, window, cx);
                        }
                    }
                }
            })
            .on_click(move |_event: &ClickEvent, window, cx| {
                if disabled {
                    return;
                }
                let new_checked = !checked;
                state.update(cx, |state, cx| {
                    state.set_checked(new_checked, cx);
                });
                if let Some(on_change) = &on_change {
                    on_change(new_checked, window, cx);
                }
            });

        if let Some(label) = self.label {
            element = element.child(
                div()
                    .text_sm()
                    .text_color(crate::theme_binding::hsla(palette.fg))
                    .child(label),
            );
        }
        element
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_inputs_checkbox_tests.rs"]
mod tests;
