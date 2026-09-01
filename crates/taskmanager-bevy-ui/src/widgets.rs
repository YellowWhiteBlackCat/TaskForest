//! Owned product component layer for the bevy frontend.
//!
//! The official two-piece base (`bevy_ui` + `bevy_ui_widgets`) provides the
//! widget *primitives*; product component semantics — the process table,
//! charts, and action menus — are owned here per the charter
//! (`docs/BEVY_UI_FRONTEND.md`: no third-party component semantic layer,
//! Feathers not adopted). Every component splits the same way:
//!
//! - a **pure core**: plain data functions (window math, path projection,
//!   style models) with zero bevy deps — the headless-test surface and the
//!   piece future frontends could share verbatim;
//! - a **render adapter**: a minimal `bsn!` scene builder consuming the
//!   core's output, themed exclusively through [`crate::palette`] tokens.
//!
//! `foo.rs` module shape per STANDARDS §1; menus are the frontend-local action
//! surface and confirmation panels are owned by `crate::confirmation`.

pub mod chart;
pub mod controls;
pub mod layout;
pub mod menu;
pub mod table;

#[cfg(test)]
#[path = "../tests/headless/widgets.rs"]
mod tests;
