//! Shared CLI composition harness for the four frontend products
//! (ADR-051): argv parsing, the UI-neutral modes (`--json`,
//! `--suggest-thresholds`, `--gpu-engines`, `--memory-smbios`,
//! `--package-power`, `--msr`), help, tracing initialization, and the
//! capability-value seam every product `[[bin]]` hands its shape handlers
//! to. The crate knows the products by their injected handler values, never
//! by `cfg`.

#![forbid(unsafe_code)]
#![deny(clippy::wildcard_imports)]

pub mod cli;
pub mod cli_gpu_engines;
pub mod cli_memory_smbios;
pub mod cli_msr;
pub mod cli_package_power;
pub mod cli_process_gpu;
pub mod run;

pub use run::{FrontendHandlers, run};

// Mounted for the Linux-only /proc fixture tests that the cli_process_gpu
// module pulls in through `#[path]` (see cli_process_gpu_tests.rs). The cfg
// mirrors the one consumer: mounting it on Windows/macOS leaves the scratch
// helper unused under -D warnings because no lib-side caller compiles there.
#[cfg(all(test, target_os = "linux"))]
#[path = "../../../tests/common/test_support.rs"]
pub(crate) mod test_support;
