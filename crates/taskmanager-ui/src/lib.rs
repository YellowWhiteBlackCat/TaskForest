//! TaskManager's own GPUI component layer (ADR-017 Phases 3/4).
//!
//! Everything here builds directly on gpui, `taskmanager-theme` (`Palette`),
//! `taskmanager-icons` and `taskmanager-ui-contract`; `gpui_component` is
//! intentionally absent (firewall: new files must not import it).
//!
//! Modules: `focus` (modal focus trap/restore), `primitives` (button, label,
//! badge, divider, spinner, progress, tooltip, scrollbar, pill),
//! `inputs` (switch, slider, checkbox, text_input, search_input),
//! `overlays` (layer_stack, dialog, popup, context_menu, dropdown_menu,
//! toast), and `data` (table, virtual_list, tree, highlighter).
//!
//! Architecture (docs/UI_COMPONENT_ARCHITECTURE.md §2.3, M2):
//! every interactive component is a `XxxState` entity (owning its
//! [`gpui::FocusHandle`]) + a typed builder + an `Element` implementation;
//! rendering is a pure consumer of state. Colors come exclusively from
//! `Palette` snapshots; focus rings read `Palette::ring` (its alpha
//! already encodes the per-frame focus-visible decision).

#![forbid(unsafe_code)]
#![deny(clippy::wildcard_imports)]
#![recursion_limit = "4096"]
// The callback-builder pattern (Rc<dyn Fn(&mut Window, &mut App)>) is inherent
// to the design; typed event enums are used wherever behavior must be asserted.
// The aliases below keep those callback field types readable instead of
// sprinkling `#[allow(clippy::type_complexity)]` over every component.

use std::rc::Rc;

use gpui::{AnyElement, App, Window};

/// Callback with no component payload.
pub type Callback = Rc<dyn Fn(&mut Window, &mut App)>;
/// Callback with one component payload.
pub type Callback1<T> = Rc<dyn Fn(T, &mut Window, &mut App)>;
/// Callback with two component payloads.
pub type Callback2<T, U> = Rc<dyn Fn(T, U, &mut Window, &mut App)>;
/// Optional callback with no component payload.
pub type OptCallback = Option<Callback>;
/// Optional callback with one component payload.
pub type OptCallback1<T> = Option<Callback1<T>>;
/// Optional callback with two component payloads.
pub type OptCallback2<T, U> = Option<Callback2<T, U>>;
/// Callback that builds an element.
pub type ElementBuilder = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
/// Callback that approves/rejects a modal outcome.
pub type BoolCallback = Rc<dyn Fn(&mut Window, &mut App) -> bool>;
/// Optional approval callback.
pub type OptBoolCallback = Option<BoolCallback>;
/// Callback receiving a shared typed event payload.
pub type EventCallback<E> = Rc<dyn Fn(&E, &mut Window, &mut App)>;
/// Optional typed-event callback.
pub type OptEventCallback<E> = Option<EventCallback<E>>;
/// Field validator (`None` accepts every input).
pub type Validator = Option<Rc<dyn Fn(&str) -> bool>>;
/// Popup-menu builder (mutates and returns the popup state).
pub type MenuBuilder = Rc<dyn Fn(PopupMenuState, &mut App) -> PopupMenuState>;
/// Content builder for a modal layer (receives the layer backfill).
pub type BackfillBuilder = Rc<dyn Fn(LayerBackfill, &mut Window, &mut App) -> AnyElement>;

pub mod data;
pub mod focus;
pub mod icons_binding;
pub mod inputs;
pub mod layout;
pub mod overlays;
pub mod primitives;
pub mod styled;
pub mod theme_binding;

use overlays::layer_stack::LayerBackfill;
use overlays::popup::PopupMenuState;

pub use data::{highlighter, key_value_row, row, table, tree, virtual_list};
pub use focus::{
    ModalEscTarget, begin_modal, esc_chain_target, modal_context_focused, restore_modal, trap_modal,
};
pub use inputs::{checkbox, search_input, slider, switch, text_input};
pub use layout::{
    BoundedScrollRailSpec, PageFrame, PageScaffold, auto_scroll_region,
    bounded_scroll_column_with_fixed_header, bounded_scroll_region,
    bounded_scroll_region_with_handle, bounded_scroll_region_with_rail, page_frame, page_viewport,
    pinned_scroll_region, scroll_region, scroll_region_with_overlay_rail, scroll_region_with_rail,
};
pub use overlays::{context_menu, dialog, dropdown_menu, layer_stack, popup, toast};
pub use primitives::{
    badge, button, card_surface, divider, icon_button, label, motion, pill, progress, scrollbar,
    section_header, selectable_text, spinner, state_panel, toolbar, tooltip,
};

/// Register every keymap this layer owns, scoped to its own contexts.
///
/// Idempotent: the host (startup path and tests constructing `RootView`
/// directly) may call it any number of times per process — keymap binds are
/// overwrites, and `focus::ensure_support` is global-guarded. Replaces the old
/// `gpui_component::init` call site (P6).
pub fn init(cx: &mut gpui::App) {
    focus::ensure_support(cx);
    inputs::text_input::init(cx);
    overlays::dialog::init(cx);
    overlays::popup::init(cx);
    primitives::selectable_text::init(cx);
    data::table::init(cx);
    data::tree::init(cx);
}
