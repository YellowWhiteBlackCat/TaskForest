//! The TaskForest binary library (ADR-029): one binary, three UI shapes.
//!
//! Exactly one `ui-*` feature is enabled per build (enforced by build.rs):
//! `ui-gpui` (default) compiles the GPUI desktop frontend, `ui-tui` the
//! ratatui frontend, `ui-iced` the iced frontend. The UI-neutral CLI
//! (`cli`, `--json`, `--suggest-thresholds`, `--gpu-engines`) is compiled in
//! every shape.

#![forbid(unsafe_code)]
#![deny(clippy::wildcard_imports)]

// A release artifact is a platform variant, never a hardware-vendor variant.
// Developers may use reduced debug builds to exercise fallback paths, but a
// distributable binary must carry every backend in the standard hardware set.
#[cfg(all(not(debug_assertions), not(feature = "hardware-all")))]
compile_error!(
    "release builds require the default `hardware-all` feature; \
     vendor-specific TaskManager artifacts are not supported"
);

pub mod cli;
pub mod cli_gpu_engines;
pub mod cli_process_gpu;
pub mod frontend;
// Mounted for the Linux-only /proc fixture tests that src/*.rs modules pull
// in through `#[path]` (see cli_process_gpu.rs). The cfg mirrors the one
// consumer: mounting it on Windows/macOS leaves the scratch helper unused
// under -D warnings because no lib-side caller compiles there.
#[cfg(all(test, target_os = "linux"))]
#[path = "../tests/common/test_support.rs"]
pub(crate) mod test_support;
