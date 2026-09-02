//! source-inspection: static-policy
//!
//! Repository-level localization and test-quality gates executed by nextest
//! on every platform.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

fn locale_messages(json: &str) -> serde_json::Map<String, serde_json::Value> {
    serde_json::from_str(json).expect("locale catalog must be a JSON object")
}

fn placeholders(message: &str) -> BTreeSet<String> {
    message
        .split('{')
        .skip(1)
        .filter_map(|tail| tail.split_once('}').map(|(name, _rest)| name))
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

#[test]
fn english_and_chinese_locale_keys_match_except_explicit_fallback_fixture() {
    let en = locale_messages(include_str!("../../locales/en.json"));
    let zh = locale_messages(include_str!("../../locales/zh.json"));
    let en_keys: BTreeSet<_> = en.keys().cloned().collect();
    let zh_keys: BTreeSet<_> = zh.keys().cloned().collect();

    let missing_in_zh: BTreeSet<_> = en_keys.difference(&zh_keys).cloned().collect();
    let missing_in_en: BTreeSet<_> = zh_keys.difference(&en_keys).cloned().collect();
    assert_eq!(
        missing_in_zh,
        BTreeSet::from(["fallback.sample".to_string()])
    );
    assert!(
        missing_in_en.is_empty(),
        "English is missing: {missing_in_en:?}"
    );

    for key in en_keys.intersection(&zh_keys) {
        let en_message = en[key]
            .as_str()
            .expect("English locale values must be strings");
        let zh_message = zh[key]
            .as_str()
            .expect("Chinese locale values must be strings");
        assert_eq!(
            placeholders(en_message),
            placeholders(zh_message),
            "locale placeholder mismatch for {key}"
        );
    }
}

#[test]
fn ui_does_not_add_obvious_hard_coded_widget_copy() {
    let ui_root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/taskmanager-gpui/src/gpui_app");
    let mut pending = vec![ui_root];
    let mut violations = Vec::new();
    let markers = [".child(\"", ".label(\"", ".placeholder(\"", ".tooltip(\""];
    // Icon glyphs are presentation primitives rather than translatable copy.
    let allowed_symbol_literals = [".child(\"\\u{2715}\")"];

    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).expect("UI source directory must be readable") {
            let entry = entry.expect("UI source entry must be readable");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                continue;
            }
            let source = fs::read_to_string(&path).expect("UI Rust source must be readable");
            for (index, line) in source.lines().enumerate() {
                let trimmed = line.trim_start();
                if !trimmed.starts_with("//")
                    && markers.iter().any(|marker| line.contains(marker))
                    && !allowed_symbol_literals
                        .iter()
                        .any(|allowed| line.contains(allowed))
                {
                    violations.push(format!("{}:{}: {}", path.display(), index + 1, trimmed));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "localize widget text through i18n::t instead of a string literal:\n{}",
        violations.join("\n")
    );
}

/// Keys referenced by `t("...")` call-sites on a single source line, in source
/// order. Matches BOTH the fully-qualified `i18n::t("...")` form (used by the
/// gpui shell under `src/`) and the bare `t("...")` form (used by the tui/iced
/// front-ends, which `use taskmanager_application::i18n::t;`). A `t` only counts
/// as a call when the byte before it is neither an identifier char nor `.`,
/// so method calls (`.select("`, `.insert("`) and identifiers ending in `t`
/// (`format!(`, `expect(`) can't false-match. A literal whose closing `"` is on
/// another line is left to rustc rather than guessed.
fn t_call_keys(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = line[i..].find("t(\"") {
        let at = i + rel;
        let prev = if at == 0 { b' ' } else { bytes[at - 1] };
        let prev_is_call_start =
            !matches!(prev, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.');
        let key_start = at + "t(\"".len();
        if prev_is_call_start && let Some(end) = line[key_start..].find('"') {
            out.push(&line[key_start..key_start + end]);
        }
        i = key_start;
    }
    out
}

/// Walk every Rust source tree that renders localized copy — the GPUI, Iced,
/// TUI, and Bevy product crates — and assert each `t("...")` call-site's
/// literal is a key present in the catalog.
///
/// `t` accepts `&'static str` and on a miss returns the *key itself* (i18n.rs),
/// so a typo like `t("proc.batch_histor")` renders the raw literal into the UI
/// with no compile error and no test failure. The en/zh parity test above can't
/// catch this; only a call-site ↔ catalog cross-check can. This previously
/// A previous version scanned the workspace gate host's `src/` directory
/// instead of the GPUI crate, which left the reference product's copy
/// unvalidated. The gate now covers the actual four copy-emitting trees.
/// Non-literal call-sites (`t(label)`, `t(some_fn())`) are skipped: the scanner
/// anchors on `t("` so an argument that isn't a `"..."` literal won't match.
#[test]
fn every_i18n_t_callsite_literal_exists_in_the_catalog() {
    let en = locale_messages(include_str!("../../locales/en.json"));
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    // The four source trees that render user-visible localized copy. The
    // shared `taskmanager-application` crate is deliberately excluded: it owns
    // the i18n module whose `mod tests` exercises `t("no.such.key")` fixtures.
    let mut pending: Vec<std::path::PathBuf> = vec![
        manifest.join("crates/taskmanager-gpui/src"),
        manifest.join("crates/taskmanager-tui/src"),
        manifest.join("crates/taskmanager-iced/src"),
        manifest.join("crates/taskmanager-bevy-ui/src"),
    ];
    let mut unknown: Vec<String> = Vec::new();

    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("src directory must be readable") {
            let entry = entry.expect("src entry must be readable");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            // Exclude the i18n module itself: its own `mod tests` exercises
            // bare `t("no.such.key")` etc., which are not real call-sites.
            if path.file_name().and_then(|n| n.to_str()) == Some("i18n.rs") {
                continue;
            }
            let source = fs::read_to_string(&path).expect("Rust source must be readable");
            for (index, line) in source.lines().enumerate() {
                // Skip full-line comments — a `//` that mentions a key isn't a
                // call-site and shouldn't be gated against the catalog.
                if line.trim_start().starts_with("//") {
                    continue;
                }
                // Collect every `t("...")` call on the line (there may be
                // several) and gate each literal against the en catalog.
                for key in t_call_keys(line) {
                    if !en.contains_key(key) {
                        unknown.push(format!(
                            "{}:{}: t(\"{}\") — key not in locales/en.json",
                            path.display(),
                            index + 1,
                            key
                        ));
                    }
                }
            }
        }
    }

    assert!(
        unknown.is_empty(),
        "t(...) call-sites reference keys absent from locales/en.json:\n{}",
        unknown.join("\n")
    );
}

#[test]
fn github_actions_release_gate_runs_on_push_pull_request_and_dispatch() {
    let workflow = include_str!("../../.github/workflows/ci.yml");
    let trigger_block = workflow
        .split_once("\non:\n")
        .and_then(|(_, tail)| tail.split_once("\nenv:"))
        .map(|(block, _)| block)
        .expect("CI workflow must have a top-level on block before env");

    // The release gate is enforced: CI runs on every push and pull_request to
    // main. The prior manual-only posture was lifted once the owner enabled
    // remote CI; workflow_dispatch is retained for manual reruns.
    assert!(trigger_block.contains("workflow_dispatch:"));
    assert!(trigger_block.contains("pull_request:"));
    assert!(trigger_block.contains("push:"));
}

#[test]
fn windows_uac_helper_is_built_staged_and_checked_inside_the_msi() {
    let wix = include_str!("../../packaging/windows/taskforest.wxs");
    let packaging = include_str!("../../.github/workflows/packaging.yml");
    let build_script = include_str!("../../packaging/windows/build-msi.sh");
    let helper = "taskmanager-process-control-helper.exe";

    assert!(
        wix.contains(helper),
        "WiX must carry the UAC helper payload"
    );
    assert!(
        packaging.contains("-p taskmanager-process-control-helper"),
        "Windows packaging must build the helper for the selected native target"
    );
    assert!(
        packaging.contains(helper),
        "Windows packaging must stage and validate the helper"
    );
    assert!(
        build_script.contains(helper),
        "the local MSI builder must reject a stage without the helper"
    );
}

/// A `#[test]`/`#[tokio::test]`/`#[gpui::test]` that prints a diagnostic but
/// performs no assertion is theatre, not a test: it runs, prints, and passes
/// regardless of whether the code under test is correct. The deleted
/// `test_cpu_cache_detection` was exactly this — `println!` of cache sizes with
/// zero asserts while the real coverage lived in `hardware_data.rs`. This gate
/// refuses to let another one land.
///
/// The detector is deliberately **narrow** to stay false-positive-free against
/// the many legitimate structural gates in this suite (which delegate to
/// assertion helpers and rarely print): a test is flagged only when its body
/// contains `println!`/`eprintln!` **and** none of the assertion-like constructs
/// (`assert*`, `unwrap(`, `expect(`, `panic!`, `unreachable!`, `unimplemented!`,
/// `todo!`, the `?` try-operator), and it is not `#[should_panic]`. Scoped to
/// `tests/`; crate-inline tests are reviewed per-crate. The subtler vacuous
/// forms (asserting `Option`'s `PartialEq`, tautological `x >= x`,
/// `is_ok()` without checking the side-effect, `let _ = x`) need human review
/// and are spelled out in `docs/STANDARDS.md` §3.6.
#[test]
fn no_test_function_is_assertion_free() {
    let tests_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut violations: Vec<String> = Vec::new();
    let mut pending = vec![tests_root];

    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };
            for name in assertion_free_test_functions(&source) {
                violations.push(format!("{}: {name}", path.display()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "these tests print a diagnostic but perform no assertion — assert real \
         behavior instead of println-ing it (docs/STANDARDS.md §3.6):\n{}",
        violations.join("\n")
    );
}

/// Names of `#[test]`-attributed functions in `source` whose body prints a
/// diagnostic without any assertion-like construct (and are not
/// `#[should_panic]`). Body extraction uses the same brace-depth matching idiom
/// as the rest of this suite.
fn assertion_free_test_functions(source: &str) -> Vec<String> {
    const ASSERTION_LIKE: &[&str] = &[
        "assert!",
        "assert_eq!",
        "assert_ne!",
        "debug_assert",
        "unwrap(",
        "expect(",
        "panic!",
        "unreachable!",
        "unimplemented!",
        "todo!",
    ];
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut search = 0usize;

    while let Some(rel) = source[search..].find("#[") {
        let attr_at = search + rel;
        let header = &source[attr_at..];
        let is_test = header.starts_with("#[test]")
            || header.starts_with("#[tokio::test")
            || header.starts_with("#[gpui::test");

        if !is_test {
            search = attr_at + "#[".len();
            continue;
        }

        // Consume this and any immediately-following `#[...]` attributes
        // (e.g. a stacked `#[cfg]`), noting `#[should_panic]`.
        let mut cursor = attr_at;
        let mut should_panic = false;
        while let Some(close) = source[cursor..].find(']') {
            if source[cursor..cursor + close + 1].contains("should_panic") {
                should_panic = true;
            }
            cursor += close + 1;
            let mut next = cursor;
            while next < source.len() && bytes[next].is_ascii_whitespace() {
                next += 1;
            }
            if source[next..].starts_with("#[") {
                cursor = next;
                continue;
            }
            break;
        }

        let Some(fn_rel) = source[cursor..].find("fn ") else {
            search = attr_at + "#[".len();
            continue;
        };
        let name_at = cursor + fn_rel + "fn ".len();
        let name_len = source[name_at..]
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(source.len() - name_at);
        let name = source[name_at..name_at + name_len].to_string();

        let Some(brace_rel) = source[name_at + name_len..].find('{') else {
            search = attr_at + "#[".len();
            continue;
        };
        let brace = name_at + name_len + brace_rel;

        let end = matching_close(source, brace);
        // Clamp: an unmatched brace (malformed/truncated source) must not panic.
        let body = if end > brace {
            &source[brace + 1..end]
        } else {
            ""
        };
        let prints = body.contains("println!") || body.contains("eprintln!");
        let asserts =
            ASSERTION_LIKE.iter().any(|needle| body.contains(needle)) || body.contains('?');
        if prints && !asserts && !should_panic {
            out.push(name);
        }
        search = end.max(attr_at + "#[".len()) + 1;
    }
    out
}

/// Index of the `}` matching the `{` at `open`, string/char/comment-aware so
/// braces inside `"{"`, `'}'`, `// …`, `/* … */`, or `r#"…"#` do not desync the
/// depth counter (a naive counter panics on tests that print braces). Returns
/// `open` when no match is found; callers clamp on that.
fn matching_close(source: &str, open: usize) -> usize {
    let bytes = source.as_bytes();
    let mut depth: i64 = 0;
    let mut i = open;
    while i < source.len() {
        let byte = bytes[i];
        // line comment
        if byte == b'/' && i + 1 < source.len() && bytes[i + 1] == b'/' {
            i += 2;
            while i < source.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // block comment
        if byte == b'/' && i + 1 < source.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < source.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        // string / byte string literal: skip to the closing quote honoring `\`
        if byte == b'"' || (byte == b'b' && i + 1 < source.len() && bytes[i + 1] == b'"') {
            let mut j = i;
            if byte == b'b' {
                j += 1;
            }
            j += 1;
            while j < source.len() {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'"' {
                    j += 1;
                    break;
                }
                j += 1;
            }
            i = j;
            continue;
        }
        // raw string `r"…"` / `r#"…"#`: skip to the matching close
        if byte == b'r' && i + 1 < source.len() && (bytes[i + 1] == b'"' || bytes[i + 1] == b'#') {
            let mut j = i + 1;
            let mut hashes = 0;
            while j < source.len() && bytes[j] == b'#' {
                hashes += 1;
                j += 1;
            }
            if j < source.len() && bytes[j] == b'"' {
                j += 1;
                while j < source.len() {
                    if bytes[j] == b'"' {
                        let mut ok = true;
                        for h in 0..hashes {
                            if j + 1 + h >= source.len() || bytes[j + 1 + h] != b'#' {
                                ok = false;
                                break;
                            }
                        }
                        if ok {
                            j += 1 + hashes;
                            break;
                        }
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
            // else: an `r` identifier, not a raw string — fall through
        }
        // char literal `'x'` / `'\x'` vs lifetime/label `'a`: disambiguate
        if byte == b'\'' {
            if i + 1 < source.len() && bytes[i + 1] == b'\\' {
                let mut j = i + 2;
                while j < source.len() && bytes[j] != b'\'' {
                    j += 1;
                }
                if j < source.len() {
                    i = j + 1;
                    continue;
                }
            } else if i + 2 < source.len() && bytes[i + 2] == b'\'' && bytes[i + 1] != b'\\' {
                i += 3;
                continue;
            }
            // lifetime/label — skip only the tick, the ident after is harmless
            i += 1;
            continue;
        }
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    open
}
