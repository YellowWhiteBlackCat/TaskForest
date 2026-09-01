//! GPUI-product architecture gates, declared as a `[[test]]` target of the
//! `taskmanager-gpui` package (ADR-051: product-scoped tests belong to the
//! product). The sources live in the workspace's shared `tests/` tree and are
//! mounted here by `#[path]` — the same registration family every crate uses
//! for its headless suites. These gates import `taskmanager_gpui` directly,
//! which is exactly the product dependency closure they are gating.

#![allow(linker_messages)]

#[path = "../../../../tests/logic/columns_metadata_test.rs"]
mod columns_metadata_test;

#[path = "../../../../tests/logic/graph_helpers_test.rs"]
mod graph_helpers_test;

#[path = "../../../../tests/logic/processes_view_test.rs"]
mod processes_view_test;
