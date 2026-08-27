//! source-inspection: static-policy
//!
//! Negative gate for raw `px(<literal>)` styling in UI code (layout
//! governance pass, 2026-08).
//!
//! The theme token layer (`taskmanager_theme::tokens`) owns the spacing and
//! type scales; views and owned components must read `SPACE_*` / `FONT_*` /
//! `LINE_HEIGHT_*` tokens instead of writing new raw pixel literals that
//! silently drift from the scale. Raw `px(..)` remains legitimate for the
//! documented LAYOUT CONTRACTS — column widths, chart dimensions, control
//! sizes, geometry math — so this gate is an allowlist firewall: every
//! production `px(<number>)` literal in the scanned trees must appear in
//! [`ALLOWED`], and every entry in [`ALLOWED`] must still be used. A new
//! literal fails the build with a hint to either use a token or add an
//! allowlist entry (with justification) — and a stale entry fails too, so the
//! list shrinks as literals migrate to tokens.
//!
//! The reference definition mirrors `ui_component_boundary.rs`: line comments
//! are stripped first, `#[cfg(test)]` / `mod tests` blocks are excluded (test
//! geometry is not UI styling), and files named `*tests.rs` / `*_test.rs` are
//! not scanned. Test-only px values (assert geometry) therefore never trip
//! the gate.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const SCAN_ROOTS: [&str; 2] = [
    "crates/taskmanager-gpui/src/gpui_app",
    "crates/taskmanager-ui/src",
];

/// Allowlist of raw px literals per file: `(relative path, values)`. Each
/// entry is a documented layout contract — column widths, chart dimensions,
/// control heights/sizes, geometry math — or a component metric that predates
/// the token layer. Spacing/type values that belong on the token scale must
/// NOT be added here; migrate the call site to `tokens::SPACE_*` / `FONT_*`.
const ALLOWED: &[(&str, &[f32])] = &[
    // crates/taskmanager-ui/src/data/table.rs
    &[12.0, 28.0],
    // crates/taskmanager-ui/src/data/table/columns.rs
    &[100.0],
    // crates/taskmanager-ui/src/data/table/render.rs
    &[-1.0, 0.0, 1.0, 2.0, 10.0, 12.0, 16.0, 20.0, 28.0, 1200.0],
    // crates/taskmanager-ui/src/data/tree.rs
    &[4.0, 12.0, 16.0, 24.0],
    // crates/taskmanager-ui/src/data/virtual_list.rs
    &[0.0],
    // crates/taskmanager-ui/src/data/virtual_list/layout.rs
    &[0.0],
    // crates/taskmanager-ui/src/inputs/checkbox.rs
    &[4.0, 10.0, 18.0],
    // crates/taskmanager-ui/src/inputs/search_input.rs
    &[14.0],
    // crates/taskmanager-ui/src/inputs/select.rs
    &[26.0],
    // crates/taskmanager-ui/src/inputs/slider.rs
    &[-8.0, 2.0, 4.0, 16.0, 28.0],
    // crates/taskmanager-ui/src/inputs/switch.rs
    &[16.0, 20.0, 36.0],
    // crates/taskmanager-ui/src/overlays/popup.rs
    &[8.0, 12.0, 14.0, 16.0, 26.0],
    // crates/taskmanager-ui/src/overlays/toast.rs
    &[16.0],
    // crates/taskmanager-ui/src/primitives/badge.rs
    &[20.0],
    // crates/taskmanager-ui/src/primitives/button.rs
    &[14.0, 28.0],
    // crates/taskmanager-ui/src/primitives/icon_button.rs
    &[28.0],
    // crates/taskmanager-ui/src/primitives/pill.rs
    &[24.0],
    // crates/taskmanager-ui/src/primitives/scrollbar.rs
    &[0.0],
    // crates/taskmanager-ui/src/primitives/tooltip.rs
    &[0.0, 8.0],
    // crates/taskmanager-gpui/src/gpui_app/chrome.rs
    &[12.0, 13.0, 22.0, 28.0, 46.0],
    // crates/taskmanager-gpui/src/gpui_app/containers_view.rs
    &[0.0],
    // crates/taskmanager-gpui/src/gpui_app/cpu_view.rs
    &[0.0, 1.0, 2.0, 5.0, 150.0, 280.0],
    // crates/taskmanager-gpui/src/gpui_app/dashboard/panels/alerts.rs
    &[142.0],
    // crates/taskmanager-gpui/src/gpui_app/dashboard/panels.rs
    &[190.0],
    // crates/taskmanager-gpui/src/gpui_app/dashboard/view.rs
    &[0.0, 14.0, 132.0],
    // crates/taskmanager-gpui/src/gpui_app/elements.rs
    &[0.0, 1.0, 2.0, 12.0, 13.0, 26.0, 480.0],
    // crates/taskmanager-gpui/src/gpui_app/graph.rs
    &[0.0, 1.5, 2.0, 4.0, 10.0, 12.0],
    // crates/taskmanager-gpui/src/gpui_app/list_view.rs
    &[0.0, 12.0, 14.0, 280.0],
    // crates/taskmanager-gpui/src/gpui_app/perf_views/gpu_engines_panel.rs
    &[0.0, 4.0],
    // crates/taskmanager-gpui/src/gpui_app/perf_views/memory_composition.rs
    &[10.0, 14.0, 44.0, 74.0],
    // crates/taskmanager-gpui/src/gpui_app/perf_views/smart_dialog.rs
    &[360.0],
    // crates/taskmanager-gpui/src/gpui_app/perf_views.rs
    &[0.0, 92.0, 280.0],
    // crates/taskmanager-gpui/src/gpui_app/process_insights/view.rs
    &[0.0, 102.0, 116.0],
    // crates/taskmanager-gpui/src/gpui_app/process_insights/view/gpu_engines.rs
    &[0.0, 116.0],
    // crates/taskmanager-gpui/src/gpui_app/process_insights/view/open_files.rs
    &[0.0, 116.0],
    // crates/taskmanager-gpui/src/gpui_app/process_insights/view/threads.rs
    &[0.0, 116.0],
    // crates/taskmanager-gpui/src/gpui_app/processes_view/chrome.rs
    &[0.0, 18.0, 56.0, 120.0],
    // crates/taskmanager-gpui/src/gpui_app/processes_view/chrome/action_button.rs
    &[14.0],
    // crates/taskmanager-gpui/src/gpui_app/processes_view/chrome/render.rs
    &[0.0, 16.0, 1142.0],
    // crates/taskmanager-gpui/src/gpui_app/processes_view/chrome/resize.rs
    &[1.0, 2.0],
    // crates/taskmanager-gpui/src/gpui_app/processes_view/rows.rs
    &[0.0, 14.0, 18.0, 56.0, 60.0, 70.0, 80.0, 90.0, 100.0, 120.0],
    // crates/taskmanager-gpui/src/gpui_app/processes_view/rows/cells.rs
    &[0.0, 56.0],
    // crates/taskmanager-gpui/src/gpui_app/root/alert_ui.rs
    &[0.0],
    // crates/taskmanager-gpui/src/gpui_app/root/batch_process.rs
    &[150.0, 420.0],
    // crates/taskmanager-gpui/src/gpui_app/root/chrome.rs
    &[0.0, 12.0, 110.0],
    // crates/taskmanager-gpui/src/gpui_app/root/diagnostic_bundle.rs
    &[230.0],
    // crates/taskmanager-gpui/src/gpui_app/root/nav.rs
    &[0.0, 14.0, 16.0],
    // crates/taskmanager-gpui/src/gpui_app/root/render.rs
    &[0.0, 14.0, 320.0],
    // crates/taskmanager-gpui/src/gpui_app/root/render/overlays.rs
    &[360.0],
    // crates/taskmanager-gpui/src/gpui_app/root/responsive.rs
    &[18.0, 460.0, 480.0, 720.0, 780.0, 1180.0],
    // crates/taskmanager-gpui/src/gpui_app/root/service_control.rs
    &[420.0],
    // crates/taskmanager-gpui/src/gpui_app/root/startup.rs
    &[80.0, 120.0, 480.0, 720.0],
    // crates/taskmanager-gpui/src/gpui_app/root/system_health.rs
    &[420.0],
    // crates/taskmanager-gpui/src/gpui_app/root/termination.rs
    &[420.0],
    // crates/taskmanager-gpui/src/gpui_app/services_view.rs
    &[0.0, 80.0, 280.0, 400.0],
    // crates/taskmanager-gpui/src/gpui_app/services_view/details.rs
    &[0.0, 165.0, 190.0, 430.0],
    // crates/taskmanager-gpui/src/gpui_app/settings_view.rs
    &[1.0, 20.0],
    // crates/taskmanager-gpui/src/gpui_app/sidebar.rs
    &[0.0, 14.0, 16.0, 34.0, 58.0, 260.0],
    // crates/taskmanager-gpui/src/gpui_app/startup_view.rs
    &[0.0, 80.0, 90.0, 140.0, 280.0, 400.0],
    // crates/taskmanager-gpui/src/gpui_app/system_health_view.rs
    &[0.0, 118.0, 120.0],
    // crates/taskmanager-gpui/src/gpui_app/system_view.rs
    &[0.0, 14.0],
    // crates/taskmanager-gpui/src/gpui_app/users_view.rs
    &[0.0, 14.0, 70.0, 80.0, 120.0, 140.0],
];

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn relative(path: &Path) -> String {
    path.strip_prefix(repository())
        .expect("scanned path is inside the repository")
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_test_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.ends_with("tests.rs") || name.ends_with("_test.rs")
}

/// Per-line test-module mask: true for lines inside `#[cfg(test)]` / `mod
/// tests` blocks (their px values are test geometry, not UI styling).
fn test_block_mask(lines: &[&str]) -> Vec<bool> {
    let mut mask = vec![false; lines.len()];
    let mut in_test = false;
    let mut depth = 0usize;
    for (i, raw) in lines.iter().enumerate() {
        let stripped = raw.trim();
        if !in_test
            && (stripped.contains("#[cfg(test)]")
                || stripped.starts_with("mod tests")
                || stripped.starts_with("mod test "))
        {
            in_test = true;
            depth = 0;
        }
        if in_test {
            depth += stripped.bytes().filter(|b| *b == b'{').count();
            let closes = stripped.bytes().filter(|b| *b == b'}').count();
            depth = depth.saturating_sub(closes);
            mask[i] = true;
            if depth == 0 && i > 0 {
                in_test = false;
            }
        }
    }
    mask
}

/// Strip line comments (a literal `//` in a string is vanishingly rare in
/// these files; the gate errs toward flagging, which is the safe direction).
fn code_part(line: &str) -> &str {
    line.split_once("//").map_or(line, |(code, _)| code)
}

/// Extract the raw numeric values of every `px(<number>)` occurrence.
fn px_literals(code: &str) -> Vec<f32> {
    let mut values = Vec::new();
    let mut rest = code;
    while let Some(pos) = rest.find("px(") {
        let after = &rest[pos + 3..];
        let digits: String = after
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
            .collect();
        if let Ok(value) = digits.parse::<f32>() {
            values.push(value);
        }
        rest = after;
    }
    values
}

fn allowed_for(path: &str) -> Option<&'static [f32]> {
    ALLOWED
        .iter()
        .find_map(|(p, values)| (*p == path).then_some(*values))
}

#[test]
fn raw_px_literals_stay_on_the_allowlist() {
    let repo = repository();
    let mut failures: Vec<String> = Vec::new();
    let mut seen: BTreeMap<&'static str, bool> = ALLOWED.iter().map(|(p, _)| (*p, false)).collect();

    for root in SCAN_ROOTS {
        let dir = repo.join(root);
        let mut stack = vec![dir];
        let mut files: Vec<PathBuf> = Vec::new();
        while let Some(current) = stack.pop() {
            for entry in fs::read_dir(&current)
                .unwrap_or_else(|e| panic!("cannot read scan root {root}: {e}"))
            {
                let entry = entry.unwrap();
                if entry.path().is_dir() {
                    stack.push(entry.path());
                } else {
                    files.push(entry.path());
                }
            }
        }
        for path in files {
            if is_test_file(&path) {
                continue;
            }
            let rel = relative(&path);
            let source =
                fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {rel}: {e}"));
            let lines: Vec<&str> = source.lines().collect();
            let mask = test_block_mask(&lines);
            let mut found: Vec<f32> = Vec::new();
            for (i, raw) in lines.iter().enumerate() {
                if mask[i] {
                    continue;
                }
                found.extend(px_literals(code_part(raw)));
            }
            found.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            found.dedup();
            if found.is_empty() {
                continue;
            }
            let Some((static_path, expected)) = ALLOWED.iter().find(|(p, _)| *p == rel) else {
                failures.push(format!(
                    "{rel} uses raw px literals {found:?} but has no allowlist entry — \
                     use tokens::SPACE_* / FONT_* / LINE_HEIGHT_* instead, or add a \
                     documented layout-contract entry"
                ));
                continue;
            };
            seen.insert(static_path, true);
            let unexpected: Vec<f32> = found
                .iter()
                .copied()
                .filter(|value| !expected.contains(value))
                .collect();
            let stale: Vec<f32> = expected
                .iter()
                .copied()
                .filter(|value| !found.contains(value))
                .collect();
            if !unexpected.is_empty() {
                failures.push(format!(
                    "{rel} uses raw px literals {unexpected:?} not on the allowlist — \
                     use tokens::SPACE_* / FONT_* / LINE_HEIGHT_* instead, or add a \
                     documented layout-contract entry"
                ));
            }
            if !stale.is_empty() {
                failures.push(format!(
                    "{rel} allowlist entries {stale:?} are no longer used — remove them \
                     (the list must shrink as literals migrate to tokens)"
                ));
            }
        }
    }

    let untouched: Vec<&str> = seen
        .iter()
        .filter(|(_, used)| !**used)
        .map(|(path, _)| *path)
        .collect();
    if !untouched.is_empty() {
        failures.push(format!(
            "allowlist entries for {untouched:?} reference files that are no longer \
             scanned or contain no raw px literals — remove them"
        ));
    }

    assert!(
        failures.is_empty(),
        "px literal allowlist violations:\n  {}",
        failures.join("\n  ")
    );
}
