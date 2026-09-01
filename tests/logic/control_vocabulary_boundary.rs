//! source-inspection: static-policy
//!
//! Negative gate for the control-vocabulary boundary (ARCH.md §8.1 语义完备律,
//! 2026-08-19).
//!
//! Two platform-native control vocabularies are banned from the UI shells:
//!
//! * **Raw nice values** — `ProcessBatchAction::SetPriority` carries the typed
//!   [`PriorityTier`] vocabulary; the platform adapters own the tier→native
//!   mapping (nice on Unix, priority class on Windows). A UI shell must
//!   therefore never construct `SetPriority` from a raw number nor keep a
//!   private nice-value fold (`priority_nice` & friends). The single
//!   tier→label fold lives in
//!   `taskmanager_shell::presentation::priority_tier_label`.
//!
//! * **POSIX stop/continue signals** — suspend/resume are the neutral
//!   `ProcessControlRequest::Suspend`/`Resume` concepts; the stop/continue
//!   signals are adapter mapping details (SIGSTOP/SIGCONT on Unix, typed
//!   `Unsupported` on Windows). A UI tree must therefore never name
//!   `ProcessSignal::Stop` / `ProcessSignal::Continue` — a frontend that does
//!   is projecting the adapter's primitive instead of the user concept. Other
//!   signal-menu vocabulary (Hangup, Interrupt, User1/2, Terminate) stays
//!   legal.
//!
//! Detection is deliberately structural, mirroring
//! [`super::renderer_fold_boundary`]: scan every `.rs` under the four UI
//! roots with line comments stripped, and flag the observable signatures of a
//! raw-nice control construction:
//!   * `SetPriority(-` / `SetPriority(0` / `SetPriority(1` — a numeric
//!     literal payload (negative, zero, or positive digit);
//!   * `SetPriority(nice` — a legacy nice-named variable payload;
//!   * `priority_nice` — a private nice-fold helper (definition or call).
//!
//! After the PriorityTier migration there are ZERO occurrences anywhere in
//! the UI roots — production and tests alike — so no allowlist is needed.
//! The same holds for the stop/continue signal names after the
//! Suspend/Resume semantic migration (2026-08-19): the neutral requests
//! complete as signal events inside the adapters, which the UI roots never
//! name.

use std::fs;
use std::path::{Path, PathBuf};

const SCAN_ROOTS: [&str; 4] = [
    "crates/taskmanager-gpui/src",
    "crates/taskmanager-tui/src",
    "crates/taskmanager-iced/src",
    "crates/taskmanager-bevy-ui/src",
];

/// The observable signatures of a raw-nice control construction in a UI
/// shell. Each entry is (pattern, what it means).
const RAW_NICE_SIGNATURES: [(&str, &str); 5] = [
    ("SetPriority(-", "negative numeric nice literal"),
    ("SetPriority(0", "zero numeric nice literal"),
    ("SetPriority(1", "positive numeric nice literal"),
    ("SetPriority(nice", "legacy nice-named variable payload"),
    ("priority_nice", "private nice-value fold helper"),
];

/// The observable signatures of POSIX stop/continue signal vocabulary in a UI
/// shell: suspend/resume must flow through the neutral
/// `ProcessControlRequest::Suspend`/`Resume` request, never through the
/// adapter's signal mapping.
const STOP_CONTINUE_SIGNAL_SIGNATURES: [(&str, &str); 2] = [
    (
        "ProcessSignal::Stop",
        "POSIX stop signal named in a UI tree (suspend must use the neutral request)",
    ),
    (
        "ProcessSignal::Continue",
        "POSIX continue signal named in a UI tree (resume must use the neutral request)",
    ),
];

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

/// Collect the offending `relative-path: meaning (pattern)` entries for one
/// signature table across the four UI roots.
fn scan_for_signatures(signatures: &[(&str, &str)]) -> Vec<String> {
    let repo = repository();
    let mut offenders = Vec::new();
    for scan_root in SCAN_ROOTS {
        let root = repo.join(scan_root);
        let mut files = Vec::new();
        collect_rs_files(&root, &mut files);
        assert!(!files.is_empty(), "scan root missing: {scan_root}");

        for path in &files {
            let Ok(source) = fs::read_to_string(path) else {
                continue;
            };
            let code = strip_line_comments(&source);
            for (pattern, meaning) in signatures {
                if code.contains(pattern) {
                    let relative = path
                        .strip_prefix(&repo)
                        .expect("scanned path is inside the repository")
                        .to_string_lossy()
                        .replace('\\', "/");
                    offenders.push(format!("{relative}: {meaning} ({pattern})"));
                }
            }
        }
    }
    offenders
}

#[test]
fn ui_shells_construct_no_raw_nice_priority_control() {
    let offenders = scan_for_signatures(&RAW_NICE_SIGNATURES);

    assert!(
        offenders.is_empty(),
        "raw nice-value priority control in UI shell(s): {offenders:?} — a frontend must \
         construct ProcessBatchAction::SetPriority(PriorityTier) and read labels through \
         taskmanager_shell::presentation::priority_tier_label (ARCH.md §8.1 语义完备律: a \
         raw nice number never crosses into a UI; the adapters own the tier→native mapping)."
    );
}

#[test]
fn ui_shells_never_name_the_stop_or_continue_signals() {
    let offenders = scan_for_signatures(&STOP_CONTINUE_SIGNAL_SIGNATURES);

    assert!(
        offenders.is_empty(),
        "POSIX stop/continue signal vocabulary in UI shell(s): {offenders:?} — suspend and \
         resume are the neutral ProcessControlRequest::Suspend/Resume concepts; SIGSTOP/SIGCONT \
         are adapter mapping details that must never cross into a UI (ARCH.md §8.1 语义完备律)."
    );
}

/// The negative scan must be able to fail: the three UI roots collectively
/// reference the typed `PriorityTier` vocabulary, proving the scan roots (and
/// this gate's pattern table) are not silently matching nothing.
#[test]
fn ui_shells_reference_the_typed_priority_tier_vocabulary() {
    let repo = repository();
    let mut references = 0;
    for scan_root in SCAN_ROOTS {
        let root = repo.join(scan_root);
        let mut files = Vec::new();
        collect_rs_files(&root, &mut files);
        assert!(!files.is_empty(), "scan root missing: {scan_root}");
        for path in &files {
            let Ok(source) = fs::read_to_string(path) else {
                continue;
            };
            if strip_line_comments(&source).contains("PriorityTier") {
                references += 1;
            }
        }
    }
    assert!(
        references > 0,
        "no UI file references PriorityTier — the negative gate's scan roots are stale"
    );
}
