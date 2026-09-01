//! The cross-crate conformance host (ADR-051).
//!
//! This crate ships no binary and no public API. It exists so the workspace
//! structural gate suites (`tests/logic`, `tests/performance`) have a stable
//! home with their fixtures, and so workspace-level `[patch]`/`[profile]`
//! sections live beside the workspace root. The four frontend products are
//! independent crates under `crates/`; the shared CLI harness is
//! `taskmanager-cli`; product-scoped tests live in their product crates.

#![forbid(unsafe_code)]
