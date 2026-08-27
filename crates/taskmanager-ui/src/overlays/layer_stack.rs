//! Layer stack: typed modal/non-modal layer management (absorption §1.5).
//!
//! - `LayerId` is a typed identifier (never a bare `usize` index).
//! - `push_modal` records the pre-modal focus once (via `focus::begin_modal`)
//!   and focuses a fresh per-layer handle; `close` focuses the new top modal
//!   or restores the trigger via `focus::restore_modal`.
//! - Only the top-most modal paints a mask (absorption 1.6-3); the mask
//!   intercepts all mouse input and closes on left click when `mask_closable`.
//! - ESC routing is a pure query (`esc_target`); the focused layer's own
//!   key-context handles the actual Escape key.
//!
//! The stack is an `Entity<LayerStack>` (held by the host view and embedded
//! as a child view); `close`/`close_top` are driven by the host and by the
//! per-layer `LayerBackfill::close` closure.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, Context, ElementId, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Render, Styled, Window, div,
};
use taskmanager_theme::Palette;

use crate::focus::{ModalEscTarget, begin_modal, esc_chain_target, restore_modal};
use crate::styled::scrim;
use crate::{BackfillBuilder, Callback};

/// Typed layer identifier. Mints from a counter with wrap-around guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayerId(u64);

impl LayerId {
    fn mint(next: &mut u64) -> Self {
        let id = *next;
        *next = if *next == u64::MAX { 1 } else { *next + 1 };
        Self(id)
    }

    /// The raw counter value (for diagnostics/tests).
    pub fn value(self) -> u64 {
        self.0
    }
}

/// A palette-derived scrim (the palette snapshot owns the color; the alpha
/// is applied at render time).
#[derive(Clone, Copy, Debug)]
pub struct PaletteScrim {
    pub palette: Palette,
    pub alpha: f32,
}

impl PaletteScrim {
    /// Build a scrim from a palette at the given opacity.
    pub fn new(palette: Palette, alpha: f32) -> Self {
        Self {
            palette,
            alpha: alpha.clamp(0.0, 1.0),
        }
    }

    /// Resolve the scrim color.
    pub fn color(&self) -> taskmanager_theme::Color {
        scrim(&self.palette, self.alpha)
    }
}

/// Per-frame context handed to a layer's content builder.
#[derive(Clone)]
pub struct LayerBackfill {
    /// This layer's focus handle (modal trap uses it).
    pub focus_handle: FocusHandle,
    /// Stack index of this layer (0-based, bottom-up).
    pub layer_ix: usize,
    /// Whether this layer is the top-most modal (draws its mask).
    pub is_top_modal: bool,
    /// Close this layer (the stack removes it and refocuses).
    pub close: Callback,
}

/// Modal layer content: mask color + click behavior + content builder.
pub struct ModalSpec {
    /// Mask color over the window; `None` draws no mask for this layer.
    pub mask: Option<PaletteScrim>,
    /// Left-click on the mask closes the layer (cancel protocol).
    pub mask_closable: bool,
    /// Whether this layer owns the keyboard (ESC handled by its content).
    pub keyboard: bool,
    /// Content builder (rebuilt every frame with the layer backfill).
    pub content: BackfillBuilder,
}

/// Non-modal layer content (popups, menus, toasts): no mask, no focus trap.
pub struct NonModalSpec {
    /// Content builder (rebuilt every frame with the layer backfill).
    pub content: BackfillBuilder,
}

/// One stacked layer.
pub struct LayerEntry {
    /// Typed layer id.
    pub id: LayerId,
    /// Whether this layer is modal.
    pub is_modal: bool,
    /// Whether this layer draws a mask (only the top modal paints one).
    pub mask: Option<PaletteScrim>,
    /// Whether the mask is click-closable.
    pub mask_closable: bool,
    /// Per-layer focus handle (focused on push; refocused on close).
    pub focus_handle: FocusHandle,
    /// Content builder (modal or non-modal).
    pub content: BackfillBuilder,
}

/// The layer stack entity: held by the host view and embedded as a child
/// (so `push_modal`/`close`/`close_top` can be driven by the host's event
/// handlers and by the per-layer close closures).
pub struct LayerStack {
    layers: Vec<LayerEntry>,
    next_id: u64,
    /// Whether `focus::begin_modal` already recorded the trigger.
    restore_armed: bool,
}

impl Default for LayerStack {
    fn default() -> Self {
        Self::new()
    }
}

impl LayerStack {
    /// An empty stack.
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            next_id: 1,
            restore_armed: false,
        }
    }

    /// Push a modal layer; focuses its fresh handle and records the
    /// pre-modal trigger on the first push. Returns the layer id.
    pub fn push_modal(&mut self, spec: ModalSpec, window: &mut Window, cx: &mut App) -> LayerId {
        if !self.restore_armed {
            begin_modal(window, cx);
            self.restore_armed = true;
        }
        let id = LayerId::mint(&mut self.next_id);
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);
        self.layers.push(LayerEntry {
            id,
            is_modal: true,
            mask: spec.mask,
            mask_closable: spec.mask_closable,
            focus_handle,
            content: spec.content,
        });
        let _ = spec.keyboard;
        id
    }

    /// Push a non-modal layer (popup/menu/toast). Does not touch modal
    /// focus; the popup content manages its own focus.
    pub fn push_popup(&mut self, spec: NonModalSpec, window: &mut Window, cx: &mut App) -> LayerId {
        let id = LayerId::mint(&mut self.next_id);
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);
        self.layers.push(LayerEntry {
            id,
            is_modal: false,
            mask: None,
            mask_closable: false,
            focus_handle,
            content: spec.content,
        });
        id
    }

    /// Close any layer by id (not just the top). After the last modal
    /// closes, restores the pre-modal trigger.
    pub fn close(&mut self, id: LayerId, window: &mut Window, cx: &mut App) {
        let Some(ix) = self.layers.iter().position(|layer| layer.id == id) else {
            return;
        };
        self.layers.remove(ix);
        self.refocus_after_close(window, cx);
    }

    /// Close the top-most layer (the default ESC path).
    pub fn close_top(&mut self, window: &mut Window, cx: &mut App) {
        if self.layers.pop().is_some() {
            self.refocus_after_close(window, cx);
        }
    }

    /// Close every layer.
    pub fn close_all(&mut self, window: &mut Window, cx: &mut App) {
        self.layers.clear();
        self.refocus_after_close(window, cx);
    }

    fn refocus_after_close(&mut self, window: &mut Window, cx: &mut App) {
        // Focus the new top modal, or restore the trigger when the last
        // modal closed.
        if let Some(top_modal) = self.top_modal() {
            top_modal.focus_handle.focus(window);
        } else if self.restore_armed {
            self.restore_armed = false;
            restore_modal(window, cx);
        }
    }

    /// The top-most modal layer (mask/event routing only consults this).
    pub fn top_modal(&self) -> Option<&LayerEntry> {
        self.layers.iter().rev().find(|layer| layer.is_modal)
    }

    /// The top-most layer of any kind.
    pub fn top(&self) -> Option<&LayerEntry> {
        self.layers.last()
    }

    /// The ESC routing decision: the top-most layer kind wins, else window.
    pub fn esc_target(&self) -> ModalEscTarget {
        esc_chain_target(self.top().map(|layer| layer.is_modal))
    }

    /// The stack index of a layer (0-based, bottom-up); `None` when absent.
    pub fn layer_ix(&self, id: LayerId) -> Option<usize> {
        self.layers.iter().position(|layer| layer.id == id)
    }

    /// Whether the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// The number of open layers.
    pub fn len(&self) -> usize {
        self.layers.len()
    }
}

impl Render for LayerStack {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let window_handle = window.window_handle();
        let top_modal_ix = self.layers.iter().rposition(|layer| layer.is_modal);
        let mut children: Vec<AnyElement> = Vec::with_capacity(self.layers.len());

        for (ix, layer) in self.layers.iter().enumerate() {
            let is_top_modal = Some(ix) == top_modal_ix;
            let layer_id = layer.id;
            let close = {
                let this = this.clone();
                Rc::new(move |_window: &mut Window, cx: &mut App| {
                    let _ = window_handle.update(cx, |_, window, cx| {
                        let _ = this.update(cx, |stack, cx| {
                            stack.close(layer_id, window, cx);
                        });
                    });
                })
            };
            let backfill = LayerBackfill {
                focus_handle: layer.focus_handle.clone(),
                layer_ix: ix,
                is_top_modal,
                close,
            };
            let content = (layer.content)(backfill, window, cx);

            if layer.is_modal && is_top_modal {
                let mask = layer.mask.map(|scrim| scrim.color());
                let mask_closable = layer.mask_closable;
                let layer_id = layer.id;
                let stack = this.clone();
                let mask_layer =
                    div().id(ElementId::NamedInteger("tm-layer-mask".into(), ix as u64));
                #[cfg(any(test, feature = "test-support"))]
                let mask_layer = mask_layer.debug_selector(move || format!("tm-layer-mask:{ix}"));
                let mask_layer = mask_layer
                    .absolute()
                    .size_full()
                    .occlude()
                    .when_some(mask, |el, color| el.bg(color))
                    .on_any_mouse_down(move |event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        if mask_closable && event.button == MouseButton::Left {
                            let _ = stack.update(cx, |stack, cx| stack.close(layer_id, window, cx));
                        }
                    });
                children.push(mask_layer.child(content).into_any_element());
            } else {
                children.push(content);
            }
        }

        // The stack root must cover the whole window: gpui elements are
        // `position: relative` by default, so a plain auto-height root would
        // make every `.absolute()` layer child resolve against a ~0-height
        // box at the stack's flex position — dialogs rendered there collapse
        // and never paint (capture evidence: no scrim, panel off-window).
        // Absolute + full-size pins the stack to the window origin so layer
        // masks and panels get the window as their containing block.
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .children(children)
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_overlays_layer_stack_tests.rs"]
mod tests;
