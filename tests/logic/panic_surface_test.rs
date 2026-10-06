//! source-inspection: static-policy
//!
//! Negative gate: the whole production tree is panic-free-by-CI, with a
//! documented allowlist for the remaining justified sites.
//!
//! Platform-caused failures must surface as typed `ProviderFailure` /
//! `SourceOutcome` results — never as a process-wide panic. This guard makes
//! that property compile-CI permanent across the workspace: every `src/`
//! tree (the four frontends, the CLI/shell harness, app-host, core,
//! application, history/telemetry stores, the OS adapters and composition
//! runtime, the afpacket/fd-bridge/perf-ioctl/windows-api boundary crates,
//! the platform contract/native/portable/provider adapters,
//! theme/icons/ui/ui-contract/assets, accessibility, the privilege/escalation
//! and net-launcher/process-control helpers, and the remaining helper and
//! support crates) is scanned.
//!
//! The reference definition mirrors the `ui_component_boundary` firewall:
//! line comments are stripped, test blocks (`#[cfg(test)] mod ...;` /
//! `mod tests {`) are removed by declaration, and the remaining production
//! text must not contain `.unwrap(`, `.expect(`, `panic!`, `todo!`,
//! `unimplemented!`, or `unreachable!`.
//!
//! Every production panic site left in the tree must appear in
//! [`ALLOWED_PANIC_SITES`] with a reason. Today those are: UI-internal
//! contract asserts whose invariants live inside the same struct (table
//! column indices and gpui/iced Element lifecycle `take()`/child-tree
//! accesses), authoring-time data fail-fast (the embedded locale catalog,
//! the command spec table, and the iced demo-log seed), and the
//! verified-infallible `serde_json` export. Adding a NEW panic site
//! anywhere — or extending one of these without a reason — fails CI.

use std::fs;
use std::path::{Path, PathBuf};

const SCAN_ROOTS: [&str; 44] = [
    "src",
    "crates/taskmanager-accessibility-linux/src",
    "crates/taskmanager-afpacket/src",
    "crates/taskmanager-app-host/src",
    "crates/taskmanager-application/src",
    "crates/taskmanager-assets/src",
    "crates/taskmanager-bevy-ui/src",
    "crates/taskmanager-cli/src",
    "crates/taskmanager-core/src",
    "crates/taskmanager-escalation/src",
    "crates/taskmanager-fd-bridge/src",
    "crates/taskmanager-gpui/src",
    "crates/taskmanager-history-store/src",
    "crates/taskmanager-iced/src",
    "crates/taskmanager-icons/src",
    "crates/taskmanager-msr-helper/src",
    "crates/taskmanager-net-launcher/src",
    "crates/taskmanager-perf-ioctl/src",
    "crates/taskmanager-platform-android/src",
    "crates/taskmanager-platform-conformance/src",
    "crates/taskmanager-platform-contract/src",
    "crates/taskmanager-platform-linux/src",
    "crates/taskmanager-platform-macos/src",
    "crates/taskmanager-platform-native/src",
    "crates/taskmanager-platform-ohos/src",
    "crates/taskmanager-platform-portable/src",
    "crates/taskmanager-platform-provider/src",
    "crates/taskmanager-platform-runtime/src",
    "crates/taskmanager-platform-windows/src",
    "crates/taskmanager-privilege-helper/src",
    "crates/taskmanager-process-control-helper/src",
    "crates/taskmanager-rapl-helper/src",
    "crates/taskmanager-setup-helper/src",
    "crates/taskmanager-shell/src",
    "crates/taskmanager-smbios-helper/src",
    "crates/taskmanager-smbios-tables/src",
    "crates/taskmanager-telemetry-store/src",
    "crates/taskmanager-test-support/src",
    "crates/taskmanager-theme/src",
    "crates/taskmanager-tray-muda/src",
    "crates/taskmanager-tui/src",
    "crates/taskmanager-ui/src",
    "crates/taskmanager-ui-contract/src",
    "crates/taskmanager-windows-api/src",
];

const PANIC_TOKENS: [&str; 6] = [
    ".unwrap(",
    ".expect(",
    "panic!(",
    "todo!(",
    "unimplemented!(",
    "unreachable!(",
];

/// (path fragment, line fragment, reason) — the only production panic sites
/// the tree may contain. Path fragments match as a substring of the relative
/// path; line fragments must appear on the offending line.
const ALLOWED_PANIC_SITES: &[(&str, &str, &str)] = &[
    (
        "crates/taskmanager-application/src/i18n.rs",
        "locale JSON must be a flat object",
        "embedded locale catalogs are authoring-time data (include_str!); a malformed catalog is \
         a build-tree error and fail-fast is the correct startup behavior",
    ),
    (
        "crates/taskmanager-application/src/command.rs",
        "command spec table covers every CommandId",
        "COMMAND_SPECS is authoring-time data compiled into the binary; the spec-coverage test \
         pins table/enum parity, so a miss is a build-tree error and fail-fast is correct",
    ),
    (
        "crates/taskmanager-gpui/src/gpui_app/users_view.rs",
        "col_ix < columns.len()",
        "TableDelegate contract: col_ix is bounded by columns_count, which reads the same \
         immutable `columns` vec this indexes",
    ),
    (
        "crates/taskmanager-gpui/src/gpui_app/startup_view.rs",
        "col_ix < columns.len()",
        "TableDelegate contract (same invariant as users_view)",
    ),
    (
        "crates/taskmanager-gpui/src/gpui_app/services_view.rs",
        "col_ix < columns.len()",
        "TableDelegate contract (same invariant as users_view)",
    ),
    (
        "crates/taskmanager-ui/src/data/virtual_list.rs",
        "global_id.unwrap()",
        "gpui request_layout contract: the closure runs inside `with_global_id` with the id set",
    ),
    (
        "crates/taskmanager-ui/src/overlays/context_menu.rs",
        "element must be set",
        "gpui Element lifecycle: paint/prepaint consume the element exactly once",
    ),
    (
        "crates/taskmanager-ui/src/overlays/dropdown_menu.rs",
        "trigger must be set",
        "gpui Element lifecycle: the trigger is configured before painting",
    ),
    (
        "crates/taskmanager-ui/src/primitives/tooltip.rs",
        "trigger must be set",
        "gpui Element lifecycle: the trigger is configured before painting",
    ),
    (
        "crates/taskmanager-iced/src/ui/components/popover.rs",
        "anchor tree",
        "iced Widget contract: `children()` declares exactly the anchor and panel trees, so \
         `overlay`'s child iterator always yields two",
    ),
    (
        "crates/taskmanager-iced/src/ui/components/popover.rs",
        "panel tree",
        "iced Widget contract (same invariant as `anchor tree`)",
    ),
    (
        "crates/taskmanager-iced/src/app/service_details.rs",
        "demo entries are non-empty",
        "the demo seed passes a literal three-entry vector to ServiceLogEntries::new, which \
         returns None only for an empty vector — authoring-time data, fail-fast is correct",
    ),
    (
        "crates/taskmanager-core/src/core/export/format.rs",
        "snapshot serialization is infallible",
        "serde_json maps non-finite floats to `null` (empirically verified) and the payload has \
         no map keys; to_string_pretty cannot fail",
    ),
    (
        "crates/taskmanager-cli/src/cli/suggest.rs",
        "threshold object serialization is infallible",
        "serde_json Value::Object with plain-string keys and finite/null values (the alert engine \
         clamps thresholds to a finite sane range and from_samples drops non-finite input); \
         to_string_pretty cannot fail — same infallible-export category as format.rs",
    ),
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

/// Sanitize one source file for structural scanning: replace the contents
/// of line comments, block comments, string literals (standard, raw, and
/// char) with spaces so brace counting and token scanning never see them,
/// while preserving every newline and thus every line index. Comments and
/// literals are handled in one pass so quotes inside comments cannot open a
/// bogus string (and `//` inside a string cannot start a bogus comment).
fn sanitize(source: &str) -> String {
    let mut out = Vec::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();
        if byte == b'/' && next == Some(b'/') {
            let start = index;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            push_spaces(&mut out, index - start);
        } else if byte == b'/' && next == Some(b'*') {
            let start = index;
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            if index + 1 < bytes.len() {
                index += 2;
            }
            push_spaces(&mut out, index - start);
        } else if byte == b'"' {
            let start = index;
            index += 1;
            while index < bytes.len() && bytes[index] != b'"' {
                if bytes[index] == b'\\' {
                    index += 2;
                } else {
                    index += 1;
                }
            }
            if index < bytes.len() {
                index += 1; // closing quote
            }
            push_spaces(&mut out, index - start);
        } else if byte == b'r' && (next == Some(b'"') || next == Some(b'#')) {
            let start = index;
            index += 1;
            let mut hashes = 0usize;
            while bytes.get(index) == Some(&b'#') {
                hashes += 1;
                index += 1;
            }
            if bytes.get(index) == Some(&b'"') {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'"' {
                        let tail = &bytes[index + 1..index + 1 + hashes];
                        if tail.iter().all(|byte| *byte == b'#') {
                            index += 1 + hashes;
                            break;
                        }
                    }
                    index += 1;
                }
            }
            push_spaces(&mut out, index - start);
        } else if byte == b'\'' {
            // Char literal only if it closes within a few bytes; otherwise the
            // quote starts a lifetime (`'a`) and is left as-is.
            let mut probe = index + 1;
            if bytes.get(probe) == Some(&b'\\') {
                probe += 2;
            } else {
                probe += 1;
            }
            if bytes.get(probe) == Some(&b'\'') {
                let start = index;
                index = probe + 1;
                push_spaces(&mut out, index - start);
            } else {
                out.push(byte);
                index += 1;
            }
        } else {
            out.push(byte);
            index += 1;
        }
    }
    String::from_utf8(out).expect("replacing literal bytes keeps valid UTF-8")
}

fn push_spaces(out: &mut Vec<u8>, count: usize) {
    out.extend(std::iter::repeat_n(b' ', count));
}

/// Remove `#[cfg(test)]` blocks (any `mod <name> {` body) by brace counting.
/// Unit tests inside the guarded crates may use `unwrap`/`expect` freely;
/// production code may not. External test bodies (`mod <name>;`) are skipped
/// separately by their declaration in [`declared_test_module_names`].
fn strip_test_blocks(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut depth = 0usize;
    let mut in_test_block = false;
    let mut pending_cfg_test = false;
    for line in source.split('\n') {
        if !in_test_block {
            let trimmed = line.trim_start();
            if pending_cfg_test {
                if trimmed.starts_with("mod ") && trimmed.contains('{') {
                    in_test_block = true;
                    depth = 1;
                    pending_cfg_test = false;
                    out.push('\n');
                    continue;
                }
                if trimmed == "mod tests {" || trimmed.starts_with("mod tests {") {
                    in_test_block = true;
                    depth = 1;
                    pending_cfg_test = false;
                    out.push('\n');
                    continue;
                }
                pending_cfg_test = false;
            }
            if trimmed == "#[cfg(test)]" || trimmed.starts_with("#[cfg(test)]") {
                pending_cfg_test = true;
                out.push('\n');
                continue;
            }
            if trimmed.starts_with("mod ") && trimmed.contains('{') {
                in_test_block = true;
                depth = 1;
                out.push('\n');
                continue;
            }
        }
        if in_test_block {
            depth += line.matches('{').count();
            depth = depth.saturating_sub(line.matches('}').count());
            // Keep line numbering stable: dropped block lines become blank
            // lines so `line - 1` indexes the ORIGINAL source line.
            out.push('\n');
            if depth == 0 {
                in_test_block = false;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn panic_tokens(source: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        for token in PANIC_TOKENS {
            if let Some(position) = line.find(token) {
                found.push((line_index + 1, format!("{line} (at byte {position})")));
            }
        }
    }
    found
}

/// Index of the first `ALLOWED_PANIC_SITES` entry whose path and line
/// fragments both match, or `None`. Matching against the ORIGINAL line keeps
/// string-literal fragments (e.g. `"anchor tree"`) matchable.
fn allowed_index(path: &str, line: &str) -> Option<usize> {
    ALLOWED_PANIC_SITES
        .iter()
        .position(|(path_fragment, line_fragment, _)| {
            path.contains(path_fragment) && line.contains(line_fragment)
        })
}

#[test]
fn production_panic_surface_stays_closed() {
    // Test-module bodies are discovered from their declarations rather than
    // guessed from file names: every `#[cfg(test)] mod <name>;` (external
    // file) or `mod <name> {` block is production-excluded by declaration.
    let mut violations = Vec::new();
    let mut used_allowlist = vec![false; ALLOWED_PANIC_SITES.len()];
    for root in SCAN_ROOTS {
        let root_path = repository().join(root);
        if !root_path.is_dir() {
            continue;
        }
        let files = walk_files(&root_path);
        let declared_test_modules: Vec<String> = files
            .iter()
            .flat_map(|file| {
                let source = fs::read_to_string(file).expect("scanned source is readable");
                declared_test_module_names(&source)
            })
            .collect();
        for entry in files {
            let stem = entry
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("")
                .to_owned();
            if declared_test_modules.contains(&stem) {
                continue;
            }
            let path = relative(&entry);
            let source = fs::read_to_string(&entry).expect("scanned source is readable");
            let original_lines: Vec<&str> = source.lines().collect();
            let code = sanitize(&source);
            let code = strip_test_blocks(&code);
            for (line, snippet) in panic_tokens(&code) {
                // Allowlist matching runs against the ORIGINAL line (string
                // literals intact); stripping preserves line numbering.
                let original_line = original_lines.get(line - 1).copied().unwrap_or("");
                match allowed_index(&path, original_line) {
                    Some(index) => used_allowlist[index] = true,
                    None => violations.push(format!("{path}:{line}: {snippet}")),
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "unexpected panic sites in production code ({}):\n{}\n\
         fix them, or justify them in ALLOWED_PANIC_SITES in tests/logic/panic_surface_test.rs",
        violations.len(),
        violations.join("\n")
    );

    // A stale allowlist entry (its site was removed or its path/line fragment
    // drifted) silently widens the exemption surface. Require every entry to
    // match a real production panic line.
    let stale: Vec<String> = ALLOWED_PANIC_SITES
        .iter()
        .zip(&used_allowlist)
        .filter(|(_, used)| !**used)
        .map(|((path_fragment, line_fragment, _), _)| format!("{path_fragment} :: {line_fragment}"))
        .collect();
    assert!(
        stale.is_empty(),
        "stale ALLOWED_PANIC_SITES entries that no longer match any production panic line \
         ({}):\n{}\n\
         remove them, or fix the path/line fragment, in tests/logic/panic_surface_test.rs",
        stale.len(),
        stale.join("\n")
    );
}

/// Names of `#[cfg(test)]`-gated test modules declared in one source file.
/// External bodies (`mod name;`) live in `name.rs` next to the declarer;
/// inline bodies (`mod name {`) are stripped separately by [`strip_test_blocks`].
fn declared_test_module_names(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let lines = source.split('\n').collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed != "#[cfg(test)]" || !trimmed.starts_with("#[cfg(test)]") {
            continue;
        }
        let Some(next) = lines.get(index + 1) else {
            continue;
        };
        let next = next.trim_start();
        let Some(rest) = next.strip_prefix("mod ") else {
            continue;
        };
        let name = rest.split([';', '{', ' ', '\t']).next().unwrap_or("");
        if !name.is_empty() {
            names.push(name.to_owned());
        }
    }
    names
}

fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).expect("scan root is readable") {
        let entry = entry.expect("directory entry is readable");
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "tests") {
                continue;
            }
            files.extend(walk_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}
