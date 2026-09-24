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

/// Enumerate every variant of a fieldless enum that has no owner-provided
/// `ALL`/`variants`/`iter`. The `match` inside the helper has **no wildcard
/// arm**, so adding a variant to `$ty` is a compile error here until it is named
/// in the invocation — the guard cannot silently stop covering a variant.
/// `PriorityTier` and `CommandId` expose an owner `ALL` and are iterated through
/// that list instead (see [`dynamic_key_producers`]).
macro_rules! enumerate_variants {
    ($ty:ty; $($variant:path),+ $(,)?) => {{
        let exhaustive = |value: $ty| match value {
            $($variant => {}),+
        };
        [$({
            exhaustive($variant);
            $variant
        }),+]
    }};
}

/// Every dynamic key producer reachable from this gate host: a helper that
/// returns a locale-catalog key from a typed input and is invoked as
/// `t(helper(input))` somewhere in a product. The literal gate above only sees
/// `t("...")`; a key these helpers emit is invisible to it, so removing that key
/// from a catalog would silently degrade the rendered string to the raw key.
///
/// Enumeration uses the owner's own list where one exists (`PriorityTier::ALL`,
/// `CommandId::ALL`); otherwise [`enumerate_variants`] pins the variant set with
/// an exhaustive match, so a new variant is either covered automatically or
/// fails to compile.
///
/// Frontend-local helpers (`DecorationOutcomeNotice::i18n_key` in gpui, iced's
/// `device_label_key`, …) are not reachable from this host and stay uncovered;
/// see the guard's report for that residual gap.
fn dynamic_key_producers() -> Vec<(&'static str, Vec<&'static str>)> {
    use taskmanager_application::CommandId;
    use taskmanager_core::core::{
        DeviceStatus, PriorityTier, ProcessAnomalyKind, SmartAvailability, StartupImpact,
    };
    use taskmanager_shell::presentation::{
        device_action_i18n_key, device_status_i18n_key, smart_availability_i18n_key,
    };

    let device_statuses = enumerate_variants!(
        DeviceStatus;
        DeviceStatus::Healthy,
        DeviceStatus::Stale,
        DeviceStatus::PermissionDenied,
        DeviceStatus::MissingTool,
        DeviceStatus::Unsupported,
    );

    vec![
        (
            "PriorityTier::i18n_key (via PriorityTier::ALL)",
            PriorityTier::ALL
                .iter()
                .map(|tier| tier.i18n_key())
                .collect(),
        ),
        (
            "ProcessAnomalyKind::i18n_key",
            enumerate_variants!(
                ProcessAnomalyKind;
                ProcessAnomalyKind::ZombieStorm,
                ProcessAnomalyKind::FileDescriptorPressure,
                ProcessAnomalyKind::MonotonicMemoryGrowth,
            )
            .iter()
            .map(|kind| kind.i18n_key())
            .collect(),
        ),
        (
            "StartupImpact::i18n_key",
            enumerate_variants!(
                StartupImpact;
                StartupImpact::High,
                StartupImpact::Medium,
                StartupImpact::Low,
                StartupImpact::None,
            )
            .iter()
            .map(|impact| impact.i18n_key())
            .collect(),
        ),
        (
            "CommandId::label_key (via CommandId::ALL)",
            CommandId::ALL.iter().map(|id| id.label_key()).collect(),
        ),
        (
            "CommandId::description_key (via CommandId::ALL)",
            CommandId::ALL
                .iter()
                .map(|id| id.description_key())
                .collect(),
        ),
        (
            "device_status_i18n_key",
            device_statuses
                .iter()
                .map(|status| device_status_i18n_key(*status))
                .collect(),
        ),
        (
            "device_action_i18n_key",
            device_statuses
                .iter()
                .map(|status| device_action_i18n_key(*status))
                .collect(),
        ),
        (
            "smart_availability_i18n_key",
            enumerate_variants!(
                SmartAvailability;
                SmartAvailability::Available,
                SmartAvailability::Unsupported,
                SmartAvailability::Unavailable,
                SmartAvailability::MissingTool,
                SmartAvailability::PermissionDenied,
            )
            .iter()
            .map(|availability| smart_availability_i18n_key(*availability))
            .collect(),
        ),
    ]
}

/// Check every [`dynamic_key_producers`] key against both catalogs, and require
/// each producer's outputs to be distinct (all of these helpers are total over
/// their typed input). Returns one diagnostic per violation so a failure names
/// the exact helper and key.
fn missing_dynamic_i18n_keys(
    en: &serde_json::Map<String, serde_json::Value>,
    zh: &serde_json::Map<String, serde_json::Value>,
) -> Vec<String> {
    let mut failures = Vec::new();
    for (helper, keys) in dynamic_key_producers() {
        let mut seen = BTreeSet::new();
        for key in keys {
            if !en.contains_key(key) {
                failures.push(format!(
                    "{helper} produces {key:?} absent from locales/en.json"
                ));
            }
            if !zh.contains_key(key) {
                failures.push(format!(
                    "{helper} produces {key:?} absent from locales/zh.json"
                ));
            }
            if !seen.insert(key) {
                failures.push(format!("{helper} produces duplicate key {key:?}"));
            }
        }
    }
    failures
}

/// A dynamic call site (`t(device_status_i18n_key(status))`) resolves a key the
/// literal gate above cannot see. This guard iterates every typed input of every
/// reachable dynamic key producer and asserts each emitted key is present in
/// **both** catalogs, so a whole-key removal can no longer silently degrade the
/// rendered string to the raw key.
#[test]
fn every_dynamic_i18n_key_producer_resolves_in_both_catalogs() {
    let en = locale_messages(include_str!("../../locales/en.json"));
    let zh = locale_messages(include_str!("../../locales/zh.json"));
    let failures = missing_dynamic_i18n_keys(&en, &zh);
    assert!(
        failures.is_empty(),
        "a dynamic t(...) key producer emits a key absent from a catalog (or a \
         duplicate), so removing that key would silently render the raw key:\n{}",
        failures.join("\n")
    );
}

/// Negative proof the check above is not vacuous: prune one key the guard knows
/// is produced from a scratch copy of each catalog (the committed catalogs are
/// never touched) and confirm the check reports the missing key on that side.
#[test]
fn dynamic_i18n_key_check_reports_a_pruned_catalog() {
    let pruned_key = "device.healthy";
    assert!(
        dynamic_key_producers()
            .iter()
            .any(|(_, keys)| keys.contains(&pruned_key)),
        "fixture key {pruned_key:?} must be one the guard actually produces"
    );

    let mut en = locale_messages(include_str!("../../locales/en.json"));
    let zh = locale_messages(include_str!("../../locales/zh.json"));
    assert!(
        en.remove(pruned_key).is_some(),
        "en fixture key {pruned_key:?} must exist before pruning"
    );
    let failures = missing_dynamic_i18n_keys(&en, &zh);
    assert!(
        failures
            .iter()
            .any(|failure| failure.contains(pruned_key) && failure.contains("locales/en.json")),
        "pruning {pruned_key:?} from en must be reported, got: {failures:?}"
    );

    let en = locale_messages(include_str!("../../locales/en.json"));
    let mut zh = locale_messages(include_str!("../../locales/zh.json"));
    assert!(
        zh.remove(pruned_key).is_some(),
        "zh fixture key {pruned_key:?} must exist before pruning"
    );
    let failures = missing_dynamic_i18n_keys(&en, &zh);
    assert!(
        failures
            .iter()
            .any(|failure| failure.contains(pruned_key) && failure.contains("locales/zh.json")),
        "pruning {pruned_key:?} from zh must be reported, got: {failures:?}"
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

#[test]
fn packaging_matrix_enforces_complete_parity_across_all_frontends() {
    let packaging = include_str!("../../.github/workflows/packaging.yml");
    let build_msi = include_str!("../../packaging/windows/build-msi.sh");
    let build_rpm = include_str!("../../packaging/rpm/build-rpm.sh");

    // 1. Linux packaging must build and validate all 4 frontends for DEB and RPM
    for ui in ["G", "I", "T", "B"] {
        assert!(
            packaging.contains("TaskForest-${ui}-"),
            "Packaging workflow must iterate over frontend {ui} for packages"
        );
    }
    assert!(
        packaging.contains("build-rpm.sh staging"),
        "Packaging workflow must invoke build-rpm.sh"
    );
    assert!(
        build_rpm.contains("ui=${4:-G}"),
        "build-rpm.sh must support frontend parameterization"
    );

    // 2. Windows MSI must build all 4 frontends
    for ui in ["G", "I", "T", "B"] {
        assert!(
            build_msi.contains(&format!("{ui}|")),
            "build-msi.sh must have configuration mapping for UI frontend {ui}"
        );
    }
    assert!(
        packaging.contains("-p taskmanager-gpui")
            && packaging.contains("-p taskmanager-iced")
            && packaging.contains("-p taskmanager-tui")
            && packaging.contains("-p taskmanager-bevy-ui"),
        "Windows packaging must compile all 4 frontend products"
    );

    // 3. Negative gate for TUI RPM spec: strictly zero graphical dependencies
    let tui_spec = include_str!("../../packaging/rpm/taskforest-t.spec");
    assert!(
        !tui_spec.contains("wayland"),
        "taskforest-t.spec must not depend on wayland"
    );
    assert!(
        !tui_spec.contains("vulkan"),
        "taskforest-t.spec must not depend on vulkan"
    );
    assert!(
        !tui_spec.contains("fontconfig"),
        "taskforest-t.spec must not depend on fontconfig"
    );

    // 4. Windows packaging upload and WiX shortcut GUID parameterization
    let wix = include_str!("../../packaging/windows/taskforest.wxs");
    assert!(
        wix.contains("$(var.ShortcutGuid)"),
        "taskforest.wxs must parameterize ShortcutGuid per frontend"
    );
    assert!(
        build_msi.contains("ShortcutGuid=$shortcut_guid"),
        "build-msi.sh must pass ShortcutGuid to WiX"
    );
    assert!(
        !packaging.contains(
            "TaskForest-G-*-${{ matrix.arch }}.msi\n            checksums-sha256-windows"
        ),
        "packaging.yml must not shadow the Windows MSI glob upload"
    );

    // 5. RPM specs for secondary frontends must not introduce conflicting polkit helpers
    for spec in [
        include_str!("../../packaging/rpm/taskforest-i.spec"),
        include_str!("../../packaging/rpm/taskforest-t.spec"),
        include_str!("../../packaging/rpm/taskforest-b.spec"),
    ] {
        assert!(
            !spec.contains("/usr/libexec/taskforest-privilege-helper"),
            "secondary frontend RPM specs must not package conflicting shared polkit helpers"
        );
    }
}

#[test]
fn release_documentation_matches_24_package_full_parity_matrix() {
    let release_doc = include_str!("../../docs/RELEASE.md");

    // 4 frontends (G, I, T, B) × 2 architectures (x64, arm64) × 3 package formats (deb, rpm, msi) = 24
    for ui in ["G", "I", "T", "B"] {
        for arch in ["x64", "arm64"] {
            for ext in ["deb", "rpm", "msi"] {
                let pattern = format!("TaskForest-{ui}-<ver>-{arch}.{ext}");
                assert!(
                    release_doc.contains(&pattern),
                    "docs/RELEASE.md table must contain release artifact entry {pattern}"
                );
            }
        }
    }
}
