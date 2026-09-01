//! source-inspection: static-policy
//!
//! Negative gate for the single-source contracts the four product frontends
//! consume but must not re-implement (ARCH.md §8.1 同一律, the ADR-020
//! folding family, and ADR-027's reducer-style state writes):
//!
//! 1. **Shell selection writes** — the multi-select set, the semantic main
//!    row, and the application detail identity are shell state with one legal
//!    write path each (`ShellApp::set_row_selection`,
//!    `add_selected_identity`, `move_selection_to`,
//!    `clear_selected_rows`). A frontend that assigns the fields directly
//!    re-implements the reducer: the page gate, the frozen-tree rule, and the
//!    invalidation bookkeeping live behind those methods, so a direct write
//!    silently skips them.
//! 2. **The dBm→signal-quality fold** — the same display fact is folded
//!    exactly once, in `taskmanager_shell::presentation::
//!    wifi_signal_quality_percent`. A private copy drifts the first time the
//!    clamp window or the percentage scaling is tuned.
//! 3. **The saved-view transfer protocol** — format tag, version, and limits
//!    are core-owned (`taskmanager-core::core::config`). A frontend imports
//!    them; a second declaration lets two protocols claim the same clipboard
//!    document.
//!
//! Detection mirrors [`super::frontend_submission_ownership_test`]: read the
//! production sources under the four frontend roots, match the observable
//! signatures, assert the finding list is empty. Two refinements keep the
//! scan honest:
//!
//! * Matching is whitespace-insensitive (see [`flatten`]), so reformatting a
//!   direct write across a method chain is not an escape hatch.
//! * Patterns are anchored on the shell receiver (`shell.` /
//!   `application.`), so a renderer-local field that shares a name does not
//!   trip the gate — GPUI's Performance device selector legitimately keeps
//!   its own `selected`, and the TUI's deref alias
//!   (`TuiApp: DerefMut<Target = ShellApp>`) is covered by a TUI-scoped
//!   pattern instead of a bare `.selected =` that would flag that selector.
//!
//! Only `src/` is scanned: a frontend's own tests are the behavior layer and
//! may exercise edge states the shell does not sanction in production.
//!
//! ## Waiting-fix exemptions
//!
//! Each invariant carries an explicit, file-scoped allowlist of the copies
//! that predate the single-source API. Every entry names its file plus the
//! `TODO(fix-frontend)` change that removes it, and an entry that no longer
//! absorbs a live violation FAILS its test: the owning frontend deletes its
//! own line in the same change that consumes the shared source. The lists
//! must only ever shrink.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const FRONTEND_SCAN_ROOTS: [&str; 4] = [
    "crates/taskmanager-gpui/src",
    "crates/taskmanager-iced/src",
    "crates/taskmanager-tui/src",
    "crates/taskmanager-bevy-ui/src",
];

const TUI_SCAN_ROOT: &str = "crates/taskmanager-tui/src";
const SHELL_PRESENTATION: &str = "crates/taskmanager-shell/src/presentation.rs";
const CORE_SAVED_VIEW_TRANSFER: &str =
    "crates/taskmanager-core/src/core/config/saved_view_transfer.rs";

/// Direct writes to the shell's selection state, anchored on the shell
/// receiver (ADR-027).
const SHELL_SELECTION_WRITE_SIGNATURES: &[(&str, &str)] = &[
    (
        "shell.selected=",
        "positional anchor written directly (ShellApp::move_selection_to)",
    ),
    (
        "shell.selected_row=",
        "semantic main row written directly (ShellApp::set_row_selection)",
    ),
    (
        "selected_rows.insert(",
        "multi-select set mutated directly (ShellApp::add_selected_identity)",
    ),
    (
        "selected_rows.extend(",
        "multi-select set mutated directly (ShellApp::add_selected_identity)",
    ),
    (
        "selected_rows.clear()",
        "multi-select set emptied directly (ShellApp::clear_selected_rows)",
    ),
    (
        "selected_rows.remove(",
        "multi-select set mutated directly (ShellApp::add_selected_identity)",
    ),
    (
        "selected_rows.retain(",
        "multi-select set mutated directly (ShellApp::add_selected_identity)",
    ),
    (
        "application.selected_process=",
        "application detail identity written directly (ShellApp::set_row_selection)",
    ),
    (
        "application.selected_service_control=",
        "service control target written directly (the application reducer owns the target)",
    ),
];

/// The TUI deref alias of `ShellApp::selected` — scoped to the TUI root, see
/// the module docs.
const TUI_DEREF_SELECTION_SIGNATURES: &[(&str, &str)] = &[(
    "self.selected=",
    "positional anchor written directly through the ShellApp deref \
     (ShellApp::move_selection_to)",
)];

/// Frontends still holding a direct selection write. The shell's own sources
/// are out of scope by construction: the scan roots never include them.
const WAITING_FIX_SELECTION_WRITES: &[(&str, &str)] = &[];

/// The observable signature of the dBm→0..=100% fold derived inline.
const DBM_FOLD_SIGNATURES: &[(&str, &str)] = &[
    ("+90.0)/60.0", "inline dBm→percent fold"),
    ("+90)/60", "inline integer dBm→percent fold"),
];

/// A frontend may still NAME a private dBm helper; it must delegate to the
/// shared one (same rule as `priority_tier_label` in
/// [`super::control_semantic_parity`]).
const DBM_FOLD_HELPER_NAMES: [&str; 2] = ["fnwifi_signal_quality_percent", "fnsignal_quality_pct"];

/// How much flattened text past a helper head the delegation check reads.
const HELPER_LOOKAHEAD_CHARS: usize = 160;

/// Frontends still deriving the dBm fold themselves.
const WAITING_FIX_DBM_FOLDS: &[(&str, &str)] = &[];

/// The observable signature of a second declaration of the core-owned
/// saved-view transfer protocol.
const SAVED_VIEW_PROTOCOL_SIGNATURES: &[(&str, &str)] = &[
    (
        "constSAVED_VIEW_TRANSFER",
        "a second declaration of the transfer protocol constant",
    ),
    (
        "\"taskmanager.saved-process-views\"",
        "a second spelling of the transfer protocol tag",
    ),
];

/// Frontends still declaring the transfer protocol themselves.
const WAITING_FIX_SAVED_VIEW_PROTOCOL: &[(&str, &str)] = &[];

/// A whitespace-insensitive view of one source file: line comments and every
/// whitespace run are dropped, so a signature matches regardless of how the
/// frontend formats the statement. Each kept character remembers the line it
/// came from, which keeps a finding actionable.
struct FlatSource {
    text: String,
    lines: Vec<usize>,
}

fn flatten(source: &str) -> FlatSource {
    let mut text = String::with_capacity(source.len());
    let mut lines = Vec::with_capacity(source.len());
    for (number, line) in source.lines().enumerate() {
        let code = line.split_once("//").map_or(line, |(code, _)| code);
        for character in code.chars() {
            if !character.is_whitespace() {
                text.push(character);
                lines.push(number + 1);
            }
        }
    }
    FlatSource { text, lines }
}

impl FlatSource {
    /// The line a character offset came from.
    fn line_of(&self, at: usize) -> usize {
        self.lines.get(at).copied().unwrap_or(self.lines.len())
    }

    /// Every occurrence of every signature, as a reportable finding. A
    /// signature that ends in `=` never matches a comparison
    /// (`shell.selected==row` is a read).
    fn findings(&self, signatures: &[(&str, &str)]) -> Vec<String> {
        let mut reports = Vec::new();
        for (pattern, meaning) in signatures {
            let mut head = 0usize;
            while let Some(found) = self.text[head..].find(pattern) {
                let at = head + found;
                head = at + pattern.len();
                let rest = self.text[head..].trim_start();
                if pattern.ends_with('=') && rest.starts_with('=') {
                    continue;
                }
                reports.push(format!("{}: {}: `{}`", self.line_of(at), meaning, pattern));
            }
        }
        reports.sort();
        reports.dedup();
        reports
    }
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
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

/// The `repository-relative file` of a scanned path.
fn relative_file(repo: &Path, path: &Path) -> String {
    path.strip_prefix(repo)
        .expect("scanned path is inside the repository")
        .to_string_lossy()
        .replace('\\', "/")
}

/// The waiting-fix exemption covering `relative`, if any.
fn waiting_fix_for<'a>(allowlist: &[(&'a str, &'a str)], relative: &str) -> Option<&'a str> {
    allowlist
        .iter()
        .find(|(file, _)| *file == relative)
        .map(|(file, _)| *file)
}

/// Read one designated owner source. Panics on a missing file: a moved single
/// source must fail loudly, not skip the gate.
fn read_flat(relative: &str) -> FlatSource {
    let full = repository().join(relative);
    let source = fs::read_to_string(&full)
        .unwrap_or_else(|error| panic!("single source {relative}: {error}"));
    flatten(&source)
}

/// Scan one signature table over the given roots, skipping the files a
/// waiting-fix exemption still covers, and record every exemption that
/// absorbed a hit.
fn scan<'a>(
    signatures: &[(&str, &str)],
    roots: &[&str],
    allowlist: &[(&'a str, &'a str)],
    absorbed: &mut HashSet<&'a str>,
) -> Vec<String> {
    let repo = repository();
    let mut violations = Vec::new();
    for scan_root in roots {
        let root = repo.join(scan_root);
        let mut files = Vec::new();
        collect_rs_files(&root, &mut files);
        assert!(!files.is_empty(), "scan root missing: {scan_root}");

        for path in &files {
            let Ok(source) = fs::read_to_string(path) else {
                continue;
            };
            let relative = relative_file(&repo, path);
            let flat = flatten(&source);
            let findings = flat.findings(signatures);
            if let Some(file) = waiting_fix_for(allowlist, &relative) {
                // A documented waiting-fix exemption covers this whole file:
                // tolerated until its fix lands, and `assert_allowlist_is_
                // absorbing` keeps the entry load bearing while it waits.
                if findings.is_empty() {
                    continue;
                }
                absorbed.insert(file);
                continue;
            }
            for finding in findings {
                violations.push(format!("{relative}:{finding}"));
            }
        }
    }
    violations
}

fn assert_no_violations(violations: &[String], message: &str) {
    assert!(
        violations.is_empty(),
        "{message}:\n{}",
        violations.join("\n")
    );
}

/// Every exemption must still absorb a live violation: an entry whose file no
/// longer offends marks a fix that landed, and its line has to go in the same
/// change (the list must only ever shrink).
fn assert_allowlist_is_absorbing(
    allowlist: &[(&str, &str)],
    absorbed: &HashSet<&'static str>,
    invariant: &str,
) {
    let stale: Vec<&str> = allowlist
        .iter()
        .filter(|(file, _)| !absorbed.contains(file))
        .map(|(file, _)| *file)
        .collect();
    assert!(
        stale.is_empty(),
        "stale waiting-fix exemption(s) for {invariant} — the file no longer violates the \
         gate, delete its allowlist line:\n{}",
        stale.join("\n")
    );
}

/// The named selection write paths are the only writers of the shell's
/// selection state (ADR-027): every frontend mutates the multi-select set,
/// the semantic main row, and the positional anchor through the shell's
/// reducer-style methods, never by assigning the fields.
#[test]
fn frontends_write_shell_selection_only_through_the_named_apis() {
    let mut absorbed = HashSet::new();
    let mut violations = scan(
        SHELL_SELECTION_WRITE_SIGNATURES,
        &FRONTEND_SCAN_ROOTS,
        WAITING_FIX_SELECTION_WRITES,
        &mut absorbed,
    );
    violations.extend(scan(
        TUI_DEREF_SELECTION_SIGNATURES,
        &[TUI_SCAN_ROOT],
        WAITING_FIX_SELECTION_WRITES,
        &mut absorbed,
    ));

    assert_no_violations(
        &violations,
        "direct shell selection write(s) in a frontend — selection is shell state with one \
         named write path per field (ShellApp::set_row_selection / add_selected_identity / \
         move_selection_to / clear_selected_rows); a direct assignment skips the page gate, \
         the frozen-tree rule, and the invalidation bookkeeping",
    );
    assert_allowlist_is_absorbing(
        WAITING_FIX_SELECTION_WRITES,
        &absorbed,
        "shell selection writes",
    );
}

/// The dBm→0..=100% signal-quality fold is single-sourced in
/// `taskmanager_shell::presentation::wifi_signal_quality_percent` (ADR-020
/// folding family): a frontend reads the shared fold, never re-derives it,
/// and a surviving private helper may only delegate to it.
#[test]
fn the_dbm_signal_quality_fold_is_single_sourced_in_the_shell() {
    let shell = read_flat(SHELL_PRESENTATION);
    assert!(
        shell.text.contains("pubfnwifi_signal_quality_percent"),
        "the shared dBm→percent fold disappeared from {SHELL_PRESENTATION} — it must live in \
         the shell presentation layer (§8.1 同一律)"
    );

    let mut absorbed = HashSet::new();
    let violations = scan(
        DBM_FOLD_SIGNATURES,
        &FRONTEND_SCAN_ROOTS,
        WAITING_FIX_DBM_FOLDS,
        &mut absorbed,
    );
    assert_no_violations(
        &violations,
        "private dBm→percent fold(s) in a frontend — read \
         taskmanager_shell::presentation::wifi_signal_quality_percent (and its `optional_` \
         pair) instead of re-deriving the mapping",
    );
    assert_local_dbm_helpers_delegate();
    assert_allowlist_is_absorbing(WAITING_FIX_DBM_FOLDS, &absorbed, "the dBm→percent fold");
}

/// A local dBm helper definition is legal only as a delegation to the shared
/// fold. Files still under a waiting-fix exemption are skipped: their copy is
/// already accounted for, and the exemption line disappears with the fix.
fn assert_local_dbm_helpers_delegate() {
    let repo = repository();
    let mut offenders = Vec::new();
    for scan_root in FRONTEND_SCAN_ROOTS {
        let root = repo.join(scan_root);
        let mut files = Vec::new();
        collect_rs_files(&root, &mut files);
        for path in &files {
            let Ok(source) = fs::read_to_string(path) else {
                continue;
            };
            let relative = relative_file(&repo, path);
            if waiting_fix_for(WAITING_FIX_DBM_FOLDS, &relative).is_some() {
                continue;
            }
            let flat = flatten(&source);
            for name in DBM_FOLD_HELPER_NAMES {
                let mut head = 0usize;
                while let Some(found) = flat.text[head..].find(name) {
                    let at = head + found;
                    head = at + name.len();
                    let body_end = (head + HELPER_LOOKAHEAD_CHARS).min(flat.text.len());
                    if !flat.text[head..body_end].contains("presentation::") {
                        offenders.push(format!(
                            "{relative}:{}: `{}` does not delegate to \
                             taskmanager_shell::presentation (a private dBm→percent table \
                             drifts from the single source the first time the clamp window is \
                             tuned)",
                            flat.line_of(at),
                            name.trim_start_matches("fn")
                        ));
                    }
                }
            }
        }
    }
    assert_no_violations(
        &offenders,
        "private dBm→percent helper(s) that re-implement the fold instead of delegating to it",
    );
}

/// The saved-view transfer protocol (format tag, version, limits) is
/// core-owned: a frontend imports `taskmanager-core`'s constants and never
/// re-declares or re-spells them, so one clipboard document can only ever
/// mean one protocol.
#[test]
fn the_saved_view_transfer_protocol_is_owned_by_core() {
    let core = read_flat(CORE_SAVED_VIEW_TRANSFER);
    assert!(
        core.text.contains("pubconstSAVED_VIEW_TRANSFER_FORMAT"),
        "the saved-view transfer protocol tag disappeared from {CORE_SAVED_VIEW_TRANSFER}"
    );

    let mut absorbed = HashSet::new();
    let violations = scan(
        SAVED_VIEW_PROTOCOL_SIGNATURES,
        &FRONTEND_SCAN_ROOTS,
        WAITING_FIX_SAVED_VIEW_PROTOCOL,
        &mut absorbed,
    );
    assert_no_violations(
        &violations,
        "second declaration of the saved-view transfer protocol in a frontend — import the \
         constants from taskmanager-core::core::config (SAVED_VIEW_TRANSFER_FORMAT and its \
         family) instead of re-declaring them",
    );
    assert_allowlist_is_absorbing(
        WAITING_FIX_SAVED_VIEW_PROTOCOL,
        &absorbed,
        "the saved-view transfer protocol",
    );
}
