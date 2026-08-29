//! Frontend-shared shell state machine (ADR-027).
//!
//! The renderer-independent frontend layer every TaskForest frontend runs:
//! page routing, selection, search, process-table sort, platform-batch
//! application, the rolling metric window, and the shared conflict-checked
//! command router. The TUI renders it with ratatui, the iced frontend with
//! iced — the state machine itself is toolkit-neutral and never names a
//! toolkit type.
//!
//! Modules:
//!
//! - [`app`] — the shell state ([`ShellApp`]/[`SystemProjectionStore`]) and the shared
//!   key-dispatch adapter.
//! - [`keys`] — the renderer-neutral key event ([`ShellKeyEvent`]) and the
//!   conflict-checked routing through the shared command table.
//! - [`fixture`] — deterministic demo data for headless tests and captures.
//! - [`history`] — the rolling per-metric sample window behind the alert
//!   threshold suggestions.
//! - [`presentation`] — shared formatting and icon-glyph mapping.
//! - [`memory`] — the shared memory-composition segment breakdown every
//!   frontend renders as the composition bar.
//! - [`viewmodel`] — the typed stat-panel ViewModel contract (one fold,
//!   three renderers; ARCH.md §8.1).

#![forbid(unsafe_code)]

pub mod app;
pub mod fixture;
pub mod history;
mod input_dispatch;
pub mod keys;
pub mod memory;
pub mod presentation;
pub mod process_filter;
pub mod viewmodel;

pub use app::process_rows::{
    ProcessProjectionGeneration, ProcessRowAnchor, ProcessRowId, ProcessRowIdentity,
};
pub use app::{
    BatchFoldChanges, BatchFoldOutput, DirectTrackState, FeedbackBatchLifetime, FeedbackLifecycle,
    FeedbackNotice, FeedbackSeverity, FeedbackSource, FeedbackState, FrameCommit, InfoSortCol,
    InfoTable, PAGE_STEP, ProcessControlFeedback, ProcessControlKind, ProcessSelection,
    ProcessViewing, QuitReason, QuitRequestOutcome, QuitState, ShellApp, ShellInputMode, SortCol,
    SortDir, SystemProjectionStore, TelemetryFrameState, aggregate_sort_key, gpu_chart_metric_gate,
    identity_range, order_service_rows, order_session_rows, order_startup_rows,
    process_control_notice_text, queue_effect, queue_effect_result, selected_rows_range, sort_axis,
};
pub use fixture::demo_app;
pub use input_dispatch::InputDispatch;
pub use keys::{LocalBinding, ShellKeyEvent, route_key, shell_local_bindings};
pub use memory::{MemSegment, MemSegmentKind, SwapBreakdown, memory_segments, swap_breakdown};
pub use presentation::{
    CommandHelp, GraphSummary, PageHelp, command_help, graph_summary, page_help,
};
pub use process_filter::{ProcessStatusFilter, matches_process_query};
