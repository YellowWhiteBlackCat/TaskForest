//! Toolkit-neutral semantic icon registry (ADR-017).
//!
//! The toolkit-neutral semantic identity ([`taskmanager_ui_contract::IconId`])
//! stays in `taskmanager-ui-contract`; the embedded SVG assets stay in
//! `taskmanager-assets`. This crate owns the mapping between them:
//!
//! - [`path`] — resolve an [`IconId`](taskmanager_ui_contract::IconId) to the
//!   embedded SVG asset path.
//! - [`asset_bytes`] — retrieve the same embedded SVG bytes for any
//!   frontend's native vector widget.
//!
//! Toolkit rendering adapters are frontend-owned (ADR-051): the GPUI SVG/
//! image builders live in `taskmanager-ui::icons_binding`, the Bevy raster
//! fallback in `taskmanager-bevy-ui`, and the iced frontend resolves bytes
//! directly. This crate compiles zero toolkit code on every target.

#![forbid(unsafe_code)]
#![deny(clippy::wildcard_imports)]

mod path;

pub use path::{asset_bytes, path};
