//! Modal dialog (absorption §2): panel + focus trap + ESC/Enter + footer
//! button protocol, callback-style `on_ok`/`on_cancel`/`on_close` (task
//! requirement; absorption 2.5 notes gc had no typed DialogEvent).
//!
//! The dialog is *content* pushed into a `LayerStack` modal layer; the
//! layer owns the mask + focus handle, the dialog owns the panel + keys.
//! Title/content are stored as per-frame builder closures (AnyElement is not
//! Clone), so the Dialog builder itself stays cheap and re-renderable.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, BoxShadow, ElementId, InteractiveElement, IntoElement, KeyBinding,
    MouseDownEvent, ParentElement, Point, SharedString, Styled, Window, actions, div, px,
};
use taskmanager_theme::Palette;
use taskmanager_ui_contract::IconId;

use crate::focus::trap_modal;
use crate::overlays::layer_stack::{LayerBackfill, ModalSpec, PaletteScrim};
use crate::primitives::button::ButtonVariant;
use crate::primitives::icon_button::{IconButton, IconButtonState};
use crate::{BackfillBuilder, ElementBuilder, OptBoolCallback, OptCallback};
use taskmanager_theme::tokens;

/// The dialog key context (Escape/Enter bindings live under it).
pub const DIALOG_CONTEXT: &str = "TaskManagerDialog";

actions!(taskmanager_dialog, [CancelDialog, ConfirmDialog]);

/// Register the dialog Escape/Enter keymap.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CancelDialog, Some(DIALOG_CONTEXT)),
        KeyBinding::new("enter", ConfirmDialog, Some(DIALOG_CONTEXT)),
    ]);
}

/// Where an explicit cancel came from (typed payload).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelSource {
    /// The Cancel button.
    Button,
    /// A left click on the mask.
    Overlay,
    /// Escape.
    Escape,
    /// Closed programmatically.
    Programmatic,
}

/// Footer button protocol: the default footer renders a cancel + ok button
/// pair; both may be customized through the builder.
#[derive(Clone)]
pub struct DialogButtonProps {
    /// Label for the OK button (default localized by the caller).
    pub ok_text: SharedString,
    /// OK button variant.
    pub ok_variant: ButtonVariant,
    /// Label for the Cancel button.
    pub cancel_text: SharedString,
    /// Cancel button variant.
    pub cancel_variant: ButtonVariant,
}

impl Default for DialogButtonProps {
    fn default() -> Self {
        Self {
            ok_text: "OK".into(),
            ok_variant: ButtonVariant::Primary,
            cancel_text: "Cancel".into(),
            cancel_variant: ButtonVariant::Secondary,
        }
    }
}

/// Footer renderer: receives the default ok/cancel button builders and may
/// arrange them (or ignore them) in its own layout.
pub type FooterFn =
    Box<dyn Fn(RenderButtonFn, RenderButtonFn, &mut Window, &mut App) -> Vec<AnyElement>>;
type RenderButtonFn = Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>;

/// The dialog panel's drop shadow: the Mission Center two-layer pair — a
/// wide, low-opacity ambient blur plus a tight, high-opacity edge blur —
/// both painted in the palette's single `card_shadow` ink (the edge layer
/// carries the full ink alpha, the ambient layer 60% of it).
fn panel_shadow(palette: &Palette) -> Vec<BoxShadow> {
    let ink = palette.card_shadow;
    vec![
        BoxShadow {
            color: crate::theme_binding::hsla(ink.with_alpha(ink.a * 0.6)),
            offset: Point::new(px(0.0), px(4.0)),
            blur_radius: px(16.0),
            spread_radius: px(0.0),
        },
        BoxShadow {
            color: crate::theme_binding::hsla(ink),
            offset: Point::new(px(0.0), px(1.0)),
            blur_radius: px(4.0),
            spread_radius: px(0.0),
        },
    ]
}

/// Builder for one modal dialog.
pub struct Dialog {
    title: Option<ElementBuilder>,
    content: Vec<ElementBuilder>,
    footer: Option<FooterFn>,
    width: f32,
    max_width: Option<f32>,
    close_button: bool,
    mask: Option<PaletteScrim>,
    mask_closable: bool,
    keyboard: bool,
    on_ok: OptBoolCallback,
    on_cancel: OptBoolCallback,
    on_close: OptCallback,
    button_props: DialogButtonProps,
    palette: Palette,
}

impl Default for Dialog {
    fn default() -> Self {
        Self::new()
    }
}

impl Dialog {
    /// An empty dialog (customize via the builder).
    pub fn new() -> Self {
        Self {
            title: None,
            content: Vec::new(),
            footer: None,
            width: 480.0,
            max_width: None,
            close_button: true,
            mask: None,
            mask_closable: true,
            keyboard: true,
            on_ok: None,
            on_cancel: None,
            on_close: None,
            button_props: DialogButtonProps::default(),
            palette: taskmanager_theme::Theme::dark().palette(),
        }
    }

    /// The dialog title (per-frame element builder).
    #[must_use]
    pub fn title(mut self, title: ElementBuilder) -> Self {
        self.title = Some(title);
        self
    }

    /// Append content (per-frame element builder).
    #[must_use]
    pub fn content(mut self, content: ElementBuilder) -> Self {
        self.content.push(content);
        self
    }

    /// Custom footer renderer. Defaults to the cancel/ok button pair.
    #[must_use]
    pub fn footer(mut self, footer: FooterFn) -> Self {
        self.footer = Some(footer);
        self
    }

    /// The confirm() preset: cancel + ok footer, mask not click-closable, no
    /// close button (absorption 2.2).
    #[must_use]
    pub fn confirm(mut self) -> Self {
        self.mask_closable = false;
        self.close_button = false;
        self
    }

    /// The alert() preset: ok-only footer, mask not click-closable, no close
    /// button.
    #[must_use]
    pub fn alert(mut self) -> Self {
        self.mask_closable = false;
        self.close_button = false;
        self.footer = Some(Box::new(|ok, _cancel, window, cx| vec![ok(window, cx)]));
        self
    }

    /// OK handler: return `false` to keep the dialog open.
    #[must_use]
    pub fn on_ok(mut self, handler: impl Fn(&mut Window, &mut App) -> bool + 'static) -> Self {
        self.on_ok = Some(Rc::new(handler));
        self
    }

    /// Cancel handler: return `false` to keep the dialog open.
    #[must_use]
    pub fn on_cancel(mut self, handler: impl Fn(&mut Window, &mut App) -> bool + 'static) -> Self {
        self.on_cancel = Some(Rc::new(handler));
        self
    }

    /// Close handler (runs after ok/cancel succeed).
    #[must_use]
    pub fn on_close(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }

    /// Mask scrim; `None` draws no mask.
    #[must_use]
    pub fn mask(mut self, mask: Option<PaletteScrim>) -> Self {
        self.mask = mask;
        self
    }

    /// Whether a left click on the mask cancels.
    #[must_use]
    pub fn mask_closable(mut self, mask_closable: bool) -> Self {
        self.mask_closable = mask_closable;
        self
    }

    /// Whether ESC/Enter are active.
    #[must_use]
    pub fn keyboard(mut self, keyboard: bool) -> Self {
        self.keyboard = keyboard;
        self
    }

    /// Whether the header close (X) button renders.
    #[must_use]
    pub fn close_button(mut self, close_button: bool) -> Self {
        self.close_button = close_button;
        self
    }

    /// Dialog width (default 480).
    #[must_use]
    pub fn w(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Maximum width.
    #[must_use]
    pub fn max_w(mut self, max_width: f32) -> Self {
        self.max_width = Some(max_width);
        self
    }

    /// Footer button labels/variants.
    #[must_use]
    pub fn button_props(mut self, props: DialogButtonProps) -> Self {
        self.button_props = props;
        self
    }

    /// Set the palette snapshot (colors come exclusively from it).
    #[must_use]
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = palette;
        self
    }

    /// Convert into a [`ModalSpec`] for `LayerStack::push_modal`. The layer
    /// owns the mask + focus handle; the dialog wires its keys + footer.
    pub fn into_modal_spec(mut self) -> ModalSpec {
        let mask = self
            .mask
            .take()
            .or_else(|| Some(PaletteScrim::new(self.palette, 0.5)));
        let mask_closable = self.mask_closable;
        let keyboard = self.keyboard;
        let content: BackfillBuilder = Rc::new(
            move |backfill: LayerBackfill, window: &mut Window, cx: &mut App| {
                self.render(&backfill, window, cx)
            },
        );
        ModalSpec {
            mask,
            mask_closable,
            keyboard,
            content,
        }
    }

    /// Render the panel with the layer backfill (focus trap + close path).
    /// Takes `&self`: the content builder is re-invoked every frame.
    pub fn render(
        &self,
        backfill: &LayerBackfill,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let palette = self.palette;
        let focus_handle = backfill.focus_handle.clone();
        let close = backfill.close.clone();

        // Button states are element-level (use_keyed_state): the layer's
        // content builder re-invokes this render every frame, and creating
        // entities here would churn focus handles per frame (absorption
        // 3.6-7 — no entity creation in render paths). The key includes the
        // layer index so simultaneous dialogs keep distinct button state.
        let close_x_state = window.use_keyed_state(
            ElementId::Name(format!("tm-dialog:{}-close-x", backfill.layer_ix).into()),
            cx,
            |_window, cx| IconButtonState::new(cx),
        );

        // The header X button runs the same cancel protocol as Escape (gc
        // dialog behavior, absorption §2.2): on_cancel → on_close → close.
        // The X is therefore indistinguishable from an explicit cancel for
        // hosts that flip their "open" flag in on_close.
        let close_x = {
            let on_cancel = self.on_cancel.clone();
            let on_close = self.on_close.clone();
            let close = close.clone();
            move |window: &mut Window, cx: &mut App| {
                let proceed = on_cancel
                    .as_ref()
                    .map(|cancel| cancel(window, cx))
                    .unwrap_or(true);
                if proceed {
                    if let Some(on_close) = &on_close {
                        on_close(window, cx);
                    }
                    (close)(window, cx);
                }
            }
        };

        let mut panel = div()
            .id(ElementId::NamedInteger(
                "tm-dialog".into(),
                backfill.layer_ix as u64,
            ))
            .w(px(self.width))
            .when_some(self.max_width, |el, max| el.max_w(px(max)))
            .rounded(crate::theme_binding::absolute(palette.panel_radius))
            .bg(crate::theme_binding::fill(palette.surface))
            .border_1()
            .border_color(crate::theme_binding::hsla(palette.border))
            .shadow(panel_shadow(&palette))
            .flex()
            .flex_col()
            .overflow_hidden()
            // The layer mask wraps this panel and closes on any left mouse-down
            // (absorption 1.6-3). Clicks INSIDE the panel must never reach the
            // mask: stop propagation here so only genuine outside clicks cancel
            // through the mask, while the panel's own controls keep working.
            .on_any_mouse_down(|_event: &MouseDownEvent, _window, cx| {
                cx.stop_propagation();
            });

        // Header.
        if self.title.is_some() || self.close_button {
            panel = panel.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px(crate::theme_binding::definite_length(tokens::SPACE_16))
                    .py(crate::theme_binding::definite_length(tokens::SPACE_12))
                    .child(
                        div()
                            .font_weight(crate::theme_binding::font_weight(
                                tokens::FONT_WEIGHT_HEADER,
                            ))
                            .text_color(crate::theme_binding::hsla(palette.fg))
                            .child(match &self.title {
                                Some(title) => title(window, cx),
                                None => div().into_any_element(),
                            }),
                    )
                    .when(self.close_button, |el| {
                        let close_x = close_x.clone();
                        el.child(
                            IconButton::new(close_x_state, IconId::Close, palette).on_activate(
                                move |_, window, cx| {
                                    (close_x)(window, cx);
                                },
                            ),
                        )
                    }),
            );
        }

        // Content.
        if !self.content.is_empty() {
            let content_elements: Vec<AnyElement> = self
                .content
                .iter()
                .map(|builder| builder(window, cx))
                .collect();
            panel = panel.child(
                div()
                    .px(crate::theme_binding::definite_length(tokens::SPACE_16))
                    .py(crate::theme_binding::definite_length(tokens::SPACE_12))
                    .flex_col()
                    .gap(crate::theme_binding::definite_length(tokens::SPACE_8))
                    .children(content_elements),
            );
        }

        let on_ok = self.on_ok.clone();
        let on_cancel = self.on_cancel.clone();
        let on_close = self.on_close.clone();

        // Keyboard: Escape cancels, Enter confirms (Dialog context).
        let keyboard = self.keyboard;
        let panel = if keyboard {
            panel
                .key_context(DIALOG_CONTEXT)
                .on_action({
                    let on_cancel = on_cancel.clone();
                    let on_close = on_close.clone();
                    let close = close.clone();
                    move |_: &CancelDialog, window, cx| {
                        let proceed = on_cancel
                            .as_ref()
                            .map(|cancel| cancel(window, cx))
                            .unwrap_or(true);
                        if proceed {
                            if let Some(on_close) = &on_close {
                                on_close(window, cx);
                            }
                            (close)(window, cx);
                        }
                    }
                })
                .on_action({
                    let on_ok = on_ok.clone();
                    let on_close = on_close.clone();
                    let close = close.clone();
                    move |_: &ConfirmDialog, window, cx| {
                        let proceed = on_ok.as_ref().map(|ok| ok(window, cx)).unwrap_or(true);
                        if proceed {
                            if let Some(on_close) = &on_close {
                                on_close(window, cx);
                            }
                            (close)(window, cx);
                        }
                    }
                })
        } else {
            panel
        };

        // Modal focus trap on the panel (Tab stays inside; absorption §2.5).
        let panel = trap_modal(panel, &focus_handle);

        panel.into_any_element()
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_overlays_dialog_tests.rs"]
mod tests;
