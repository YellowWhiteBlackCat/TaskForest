//! source-inspection: static-policy
//!
//! Long-term renderer/data dependency boundary (ARCH.md §8.1).
//!
//! The folding-layer contract ("一次折叠，三端渲染") says render modules may
//! not fold typed observations into display semantics themselves — that fold
//! belongs to a pure data-layer module (page `*_vm`/`*_stats`/`projection`
//! builders or `taskmanager-shell`). The 2026-08-24 migration removed every
//! known exception in the same change as its replacement. This policy test has
//! no migration allowlist and is not evidence that a migration is complete.
//!
//! Since 2026-08-20 the gate covers all three product frontends, each with
//! its own scan root and paint signature:
//!   * GPUI — `crates/taskmanager-gpui/src/gpui_app`;
//!   * Iced — `crates/taskmanager-iced/src`;
//!   * TUI  — `crates/taskmanager-tui/src`.
//!
//! Detection is deliberately fail-closed and structural — it flags the
//! observable signature of an inline fold in a render module:
//!   * a `current_*` observation READ against a typed metrics/observation
//!     accessor inside the same file that also paints (`fn render`,
//!     `impl TableDelegate`, or a gpui `div()` builder — per-frontend paint
//!     idioms below).
//!
//! The rule matches the `current_*` accessor family by PREFIX, not by an
//! exhaustive suffix whitelist. A suffix list fails OPEN: accessors outside
//! the enumerated suffixes (e.g. `current_link_speed_mbps()`,
//! `current_slots_used()`, `current_used_rate_mib_per_sec()`,
//! `current_compressed_swap_cache_enabled()`) silently escape the gate,
//! which is exactly the drift the gate exists to catch. Prefix matching
//! fails CLOSED: an unknown `current_*` call in a paint module trips the
//! gate, and a reviewer decides whether it is a real fold (move the read to
//! the page's data-layer module) or a surveyed non-observation idiom (add
//! it to the documented denylist in `has_observation_read`).
//!
//! The denylist is small and every entry names WHY it is not a scalar
//! observation read:
//!   * `.current_value(` / `.current_number(` — sensor/fan/measurement
//!     reads on the core `Observation` type (GPUI parity: the GPUI signature
//!     does not count the sensor read either; its sensor modules are caught
//!     via co-located `_pct()` reads).
//!   * `.current_user(` / `.current_start_token(` / `.current_exe_path(` —
//!     process/session identity accessors, not scalar observations.
//!   * `"alerts.current_value"` — an i18n key literal whose dotted name
//!     coincidentally starts with `.current_`.
//!
//! Direct `*Availability::` pattern matches stay out by design: they fold
//! availability STATE, not scalar observation values, and matching enum
//! variants by prefix would be a different (and noisier) gate.
//!
//! A finding is fixed by moving the read into the page's data-layer module and
//! passing the folded string/ViewModel down. Adding an exception is not a fix.

use std::fs;
use std::path::{Path, PathBuf};

const GPUI_SCAN_ROOT: &str = "crates/taskmanager-gpui/src/gpui_app";
const ICED_SCAN_ROOT: &str = "crates/taskmanager-iced/src";
const TUI_SCAN_ROOT: &str = "crates/taskmanager-tui/src";

type PaintDetector = fn(&str) -> bool;
type FrontendScan = (&'static str, PaintDetector);

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `current_*` call sites surveyed as NOT typed scalar observation reads.
///
/// Kept on a documented denylist (module docs) so the prefix rule fails
/// closed: anything matching `.current_` and NOT listed here is treated as
/// an observation read and trips the gate in a paint module.
const NON_OBSERVATION_READ_IDS: &[&str] = &[
    // Sensor/fan/measurement reads on the core `Observation` type (GPUI
    // parity) — see module docs.
    ".current_value(",
    ".current_number(",
    // Identity/token accessors, not scalar observations.
    ".current_user(",
    ".current_start_token(",
    ".current_exe_path(",
    // i18n key literal with a coincidental ".current_" prefix.
    "\"alerts.current_value\"",
];

/// The observable signature of an inline fact fold: a typed observation
/// read. Matches the `current_*` accessor family by PREFIX and fails
/// closed against the surveyed non-observation denylist above.
fn has_observation_read(code: &str) -> bool {
    code.contains(".current_")
        && !NON_OBSERVATION_READ_IDS
            .iter()
            .any(|id| code.contains(id))
}

/// The observable signature of a render module: it paints. Per frontend:
///   * GPUI — `fn render*`, `impl TableDelegate`, or a `div()` builder.
fn paints_gpui(code: &str) -> bool {
    code.contains("fn render") || code.contains("impl TableDelegate") || code.contains("div()")
}

///   * Iced — a page/view entry (`fn render` / `fn view(`), a custom widget
///     (`impl Widget`, e.g. `focus/widget.rs`), or any `-> Element` builder
///     fn (the repo's helper builders all return
///     `Element<'a, Message, iced::Theme, iced::Renderer>`, e.g.
///     `icons.rs`/`focus.rs`/`saved_views.rs`). `fn view(` is
///     paren-anchored so the app-logic `fn viewport_*` accessor family
///     (`app/motion.rs`, `app/scroll.rs`, `app/prefs_accessors.rs`) is not
///     mistaken for painting.
fn paints_iced(code: &str) -> bool {
    code.contains("fn view(")
        || code.contains("fn render")
        || code.contains("impl Widget")
        || code.contains("-> Element")
}

///   * TUI (Ratatui) — the repo paints via `fn render*` helpers taking
///     `frame: &mut Frame<'_>` (`Frame<` anchors on that signature type, not
///     the bare import). `impl Widget for` and `fn draw` are the other
///     standard Ratatui paint idioms; zero occurrences exist today, they are
///     kept for forward coverage at no false-positive cost.
fn paints_tui(code: &str) -> bool {
    code.contains("fn render")
        || code.contains("impl Widget for")
        || code.contains("Frame<")
        || code.contains("fn draw")
}

fn collect_rs_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn render_modules_gain_no_new_inline_observation_folds() {
    let repo = repository();
    let frontends: [FrontendScan; 3] = [
        (GPUI_SCAN_ROOT, paints_gpui),
        (ICED_SCAN_ROOT, paints_iced),
        (TUI_SCAN_ROOT, paints_tui),
    ];

    let mut offenders = Vec::new();
    for (scan_root, paints) in frontends {
        // A moved/renamed frontend crate must fail the gate, not skip it.
        let root_path = repo.join(scan_root);
        assert!(
            root_path.is_dir(),
            "scan root missing: {scan_root} — update the gate's frontend roots"
        );
        let mut files = Vec::new();
        collect_rs_files(&root_path, &mut files);
        assert!(!files.is_empty(), "scan root empty: {scan_root}");

        for path in &files {
            let Ok(source) = fs::read_to_string(path) else {
                continue;
            };
            let code = strip_line_comments(&source);
            if has_observation_read(&code) && paints(&code) {
                let relative = path
                    .strip_prefix(&root_path)
                    .expect("scanned path is inside the scan root")
                    .to_string_lossy()
                    .replace('\\', "/");
                offenders.push(format!("{scan_root}/{relative}"));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "new inline observation fold in render module(s): {offenders:?} — move the \
         read into the page's data-layer module (ARCH.md §8.1) and pass the folded \
         ViewModel down; renderer exceptions are not accepted."
    );
}
