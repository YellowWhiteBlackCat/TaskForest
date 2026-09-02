//! App-adaptation helpers over the owned component layer (ADR-017). This module
//! is NOT a component library: every reusable primitive lives in
//! `taskmanager-ui` (primitives/inputs/overlays/data), and this file only hosts
//! the business-flavored compositions the app views share — the focus-ring /
//! pill / tool_btn / dialog / graph_card / status_bar family, all parameterized
//! by a `&Theme` and caller-owned state via closures. New primitives must land
//! in `taskmanager-ui`, never here; the UI component architecture record
//! (`docs/UI_COMPONENT_ARCHITECTURE.md` §2.2 "业务组件是资产") treats these as
//! app assets, not generic controls.
//!
//! Overlay/dismiss pattern: the full-size content container carries the
//! `on_mouse_down` close handler; the inner panel calls `cx.stop_propagation()` so
//! clicks on it don't bubble up and dismiss. (Mirrors gpui's own `window/prompts.rs`.)

mod actions;
mod graphs;
mod overlays;
mod visual;

pub use actions::{Pill, ToolBtn, pill, tool_btn};
pub(crate) use graphs::mini_graph_cell;
pub(crate) const CARD_SHADOW_AMBIENT_ALPHA: f32 = 0.35;
pub(crate) const CARD_SHADOW_AMBIENT_DROP: f32 = 2.0;
pub(crate) const CARD_SHADOW_AMBIENT_BLUR: f32 = 6.0;
pub use graphs::{
    GraphLegendEntry, card_shadow, graph_card, graph_card_with_dual_state, graph_card_with_state,
    graph_legend,
};
pub use overlays::{
    dialog_overlay, dialog_overlay_width, more_rows_hint, more_rows_label, tooltip,
    tooltip_overlay, truncated_text,
};
pub(crate) use visual::sparkline;
pub use visual::{
    focus_ring, focus_ring_shadow, highlighted_text, highlighted_text_with_ranges, status_bar,
    titlebar_border,
};

// ── stateful entity factory (Input box wiring) ─────────────────────────────
// The search/run Input boxes scattered across the views each carry a verbatim copy of
// the same wiring: root.rs `init_search_entity` (Apps) + `init_run_entity` (Run dialog),
// services_view.rs `init_search_entity`, startup_view.rs `init_search_entity`. Each one
// (1) builds an `Entity<InputState>` with a placeholder via `cx.new(...)`, and (2)
// subscribes `InputEvent::Change` once to pipe the new value into some view-specific
// sink (`RootView.search_query` / `UiState.query`). Run-command submission reads its
// own input entity directly and keeps no parallel string mirror. The old shared
// `make_search_entity` factory was removed once every view adopted its own copy.

#[cfg(test)]
#[path = "../../tests/gui/gpui_app/elements/tests.rs"]
mod tests;
