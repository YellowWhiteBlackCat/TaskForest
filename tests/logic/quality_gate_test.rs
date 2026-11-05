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
    use taskforest_core::core::{
        DeviceStatus, PriorityTier, ProcessAnomalyKind, SmartAvailability, StartupImpact,
    };
    use taskmanager_application::CommandId;
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

/// The bodies of every `"..."` string literal in one Rust source, comment text
/// removed. Rendered copy lives in a literal; `//` and `/* */` text never
/// paints, so neither may be read (and a literal must not be missed just
/// because a `//` inside a URL-like literal looks like a comment start).
fn string_literal_bodies(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut bodies = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        match chars[index] {
            '/' if chars.get(index + 1) == Some(&'/') => {
                index += 2;
                while index < chars.len() && chars[index] != '\n' {
                    index += 1;
                }
            }
            '/' if chars.get(index + 1) == Some(&'*') => {
                index += 2;
                let mut depth = 1usize;
                while index < chars.len() && depth > 0 {
                    if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
                        depth += 1;
                        index += 2;
                    } else if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                        depth -= 1;
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
            }
            '"' => {
                index += 1;
                let mut body = String::new();
                while index < chars.len() {
                    let current = chars[index];
                    if current == '\\' {
                        body.push(current);
                        if let Some(next) = chars.get(index + 1) {
                            body.push(*next);
                        }
                        index += 2;
                        continue;
                    }
                    if current == '"' || current == '\n' {
                        index += 1;
                        break;
                    }
                    body.push(current);
                    index += 1;
                }
                bodies.push(body);
            }
            _ => index += 1,
        }
    }
    bodies
}

fn contains_chinese(text: &str) -> bool {
    text.chars().any(|c| matches!(c, '\u{4e00}'..='\u{9fff}'))
}

/// Every Chinese literal the copy-emitting product trees paint, as
/// `(file:line, literal)`. `taskmanager-application::i18n` is the only Rust
/// source allowed to hold catalog text, and it holds none: the catalogs are
/// `locales/*.json`.
fn painted_chinese_literals() -> Vec<(String, String)> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut pending: Vec<std::path::PathBuf> = [
        "crates/taskmanager-gpui/src",
        "crates/taskmanager-iced/src",
        "crates/taskmanager-tui/src",
        "crates/taskmanager-bevy-ui/src",
        "crates/taskmanager-ui/src",
    ]
    .into_iter()
    .map(|dir| manifest.join(dir))
    .collect();
    let mut found = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("product source directory must be readable") {
            let path = entry.expect("product source entry must be readable").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                continue;
            }
            for (index, line) in fs::read_to_string(&path)
                .expect("product Rust source must be readable")
                .lines()
                .enumerate()
            {
                for body in string_literal_bodies(line)
                    .into_iter()
                    .filter(|body| contains_chinese(body))
                {
                    found.push((format!("{}:{}", path.display(), index + 1), body));
                }
            }
        }
    }
    found
}

/// One message catalog, one authority: a frontend that paints Chinese copy the
/// shared `zh` catalog does not already own has forked the message layer (a
/// private dictionary), so the string can drift from the other three products
/// with nothing to compare it against. Chinese that is catalog-owned is
/// allowed because it is host-data-shaped fixture text or a product name the
/// catalog itself carries — the catalog stays the single place to change it.
#[test]
fn no_frontend_paints_chinese_copy_the_catalog_does_not_own() {
    let zh = locale_messages(include_str!("../../locales/zh.json"));
    let owned: BTreeSet<String> = zh
        .values()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_string)
        .collect();
    let found = painted_chinese_literals();
    assert!(
        !found.is_empty(),
        "the scan reached no Chinese literal at all, so the tree list is stale"
    );
    let unowned: Vec<String> = found
        .into_iter()
        .filter(|(_, body)| !owned.contains(body))
        .map(|(site, body)| format!("{site}: {body:?} is not a value in locales/zh.json"))
        .collect();
    assert!(
        unowned.is_empty(),
        "frontends must resolve localized copy through the shared catalog; \
         unowned Chinese literals fork the message layer:\n{}",
        unowned.join("\n")
    );
}

/// Negative proof the scan above is not vacuous: a literal is reported, a
/// comment containing the same text is not, and the catalog-owned/painted split
/// is decided by the catalog's values.
#[test]
fn chinese_literal_scan_reads_literals_not_comments() {
    let source = "// 设置\nlet title = \"设置\"; // 关闭\n/* let hidden = \"隐藏\"; */\n";
    let bodies: Vec<String> = string_literal_bodies(source)
        .into_iter()
        .filter(|body| contains_chinese(body))
        .collect();
    assert_eq!(
        bodies,
        ["设置"],
        "only the rendered literal counts; comment text never paints"
    );
}

// ── mandatory public-resource convergence standard (ADR-063) ───────────────

/// The message layer has exactly one Rust address. Outside
/// `taskmanager-application/src/i18n.rs` no production source may embed a
/// locale catalog, define its own `Language`, or spell a per-language literal
/// table — each is the signature of a second dictionary, which is what let one
/// frontend's copy drift from the other three products.
const LOCALE_OWNER: &str = "crates/taskmanager-application/src/i18n.rs";

fn locale_fork_finding_in_line(line: &str) -> Option<&'static str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
        return None;
    }
    if line.contains("include_str!(") && line.contains("locales/") {
        return Some("embeds a locale catalog instead of calling the owner resolver");
    }
    if line.contains("enum Language") {
        return Some("defines a second `Language` type");
    }
    if line.contains("Language::En => \"") || line.contains("Language::Zh => \"") {
        return Some("spells a per-language literal table");
    }
    None
}

/// Every production source tree (`crates/*/src` and the root `src/`).
fn locale_fork_findings() -> Vec<String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut pending: Vec<std::path::PathBuf> = vec![manifest.join("src")];
    for entry in fs::read_dir(manifest.join("crates")).expect("crates directory must be readable") {
        let src = entry
            .expect("crate entry must be readable")
            .path()
            .join("src");
        if src.is_dir() {
            pending.push(src);
        }
    }
    let mut findings = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("production source directory must be readable") {
            let path = entry
                .expect("production source entry must be readable")
                .path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                continue;
            }
            let relative = path
                .strip_prefix(manifest)
                .unwrap_or(&path)
                .display()
                .to_string()
                .replace('\\', "/");
            if relative == LOCALE_OWNER {
                continue;
            }
            for (index, line) in fs::read_to_string(&path)
                .expect("production Rust source must be readable")
                .lines()
                .enumerate()
            {
                if let Some(finding) = locale_fork_finding_in_line(line) {
                    findings.push(format!("{relative}:{}: {finding}", index + 1));
                }
            }
        }
    }
    findings
}

#[test]
fn only_the_owner_holds_the_message_layer() {
    let findings = locale_fork_findings();
    assert!(
        findings.is_empty(),
        "the message layer has one owner (`{LOCALE_OWNER}`): `locales/*.json` is the only \
         copy store, `Language` the only language type, and `t` the only lookup (ADR-063):\n{}",
        findings.join("\n")
    );
}

/// Negative proof: each fork signature is detected, the owner's own forms are
/// not matched by accident, and a comment cannot hide one.
#[test]
fn locale_fork_check_reports_a_second_dictionary() {
    assert_eq!(
        locale_fork_finding_in_line("const EN: &str = include_str!(\"../../../locales/en.json\");"),
        Some("embeds a locale catalog instead of calling the owner resolver")
    );
    assert_eq!(
        locale_fork_finding_in_line("pub enum Language { En, Zh }"),
        Some("defines a second `Language` type")
    );
    assert_eq!(
        locale_fork_finding_in_line("Language::En => \"Settings\","),
        Some("spells a per-language literal table")
    );
    assert_eq!(
        locale_fork_finding_in_line("// Language::En => \"Settings\","),
        None
    );
    assert_eq!(
        locale_fork_finding_in_line("let label = t(\"settings.title\");"),
        None
    );
}

/// Crate-scoped render-entry markers: at each of these a string literal is
/// painted directly, so a literal there is product copy that bypassed the
/// catalog. This table is the audited surface — a frontend that adds a new
/// copy-emitting API must extend it (and its ceiling) in the same change.
const HARD_COPY_MARKERS: [(&str, &[&str]); 5] = [
    (
        "crates/taskmanager-gpui/src",
        &[".child(\"", ".label(\"", ".placeholder(\"", ".tooltip(\""],
    ),
    (
        "crates/taskmanager-ui/src",
        &[".child(\"", ".label(\"", ".placeholder(\"", ".tooltip(\""],
    ),
    (
        "crates/taskmanager-iced/src",
        &[
            "text(\"",
            "button(\"",
            "titled(\"",
            "tooltip(\"",
            "section(\"",
        ],
    ),
    (
        "crates/taskmanager-tui/src",
        &[
            "Span::raw(\"",
            "Line::from(\"",
            "Span::styled(\"",
            "Line::styled(\"",
            "title(\"",
        ],
    ),
    (
        "crates/taskmanager-bevy-ui/src",
        &["label: \"", "text(\"", "title(\""],
    ),
];

/// Audited census of the markers above, all retired in the ADR-063 zeroing
/// change. Every crate is zero: a rise is new hard-coded copy, and a stale
/// ledger number means the ratchet was weakened without recording it.
const HARD_COPY_CEILINGS: [(&str, usize); 5] = [
    ("crates/taskmanager-gpui/src", 0),
    ("crates/taskmanager-ui/src", 0),
    ("crates/taskmanager-iced/src", 0),
    ("crates/taskmanager-tui/src", 0),
    ("crates/taskmanager-bevy-ui/src", 0),
];

/// Calls whose first literal is a stable identity token, not painted copy.
/// `TableColumn::text(id, key, width)` renders the `key` through the catalog;
/// the `id` is a persisted column address. Counting it as copy would punish a
/// table for having an identity at all.
const IDENTITY_FIRST_CONSTRUCTORS: &[&str] = &["TableColumn::text("];

/// The literals painted on one source line through the given markers. A comment
/// line is skipped; a body containing `.` is a catalog key or a number, and a
/// body without three consecutive ASCII letters is a symbol (an escape such as
/// `"\\u{2715}"`) rather than translatable copy.
fn hard_copied_literals_in_line(line: &str, markers: &[&str]) -> Vec<String> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
        return Vec::new();
    }
    if IDENTITY_FIRST_CONSTRUCTORS
        .iter()
        .any(|constructor| line.contains(constructor))
    {
        return Vec::new();
    }
    let mut found = Vec::new();
    for marker in markers {
        let mut from = 0usize;
        while let Some(relative) = line[from..].find(marker) {
            let start = from + relative + marker.len();
            let Some(relative_end) = line[start..].find('"') else {
                break;
            };
            let body = &line[start..start + relative_end];
            from = start + relative_end + 1;
            if body.len() >= 3 && !body.contains('.') && has_letter_run(body) {
                found.push(body.to_string());
            }
        }
    }
    found
}

/// Three consecutive ASCII letters: the shape of a word, not a glyph or a unit.
fn has_letter_run(body: &str) -> bool {
    body.as_bytes()
        .windows(3)
        .any(|triplet| triplet.iter().all(u8::is_ascii_alphabetic))
}

fn hard_copied_literals(tree: &Path, markers: &[&str]) -> Vec<String> {
    let mut pending: Vec<std::path::PathBuf> = vec![tree.to_path_buf()];
    let mut found = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("render source directory must be readable") {
            let path = entry.expect("render source entry must be readable").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                continue;
            }
            for (index, line) in fs::read_to_string(&path)
                .expect("render Rust source must be readable")
                .lines()
                .enumerate()
            {
                for body in hard_copied_literals_in_line(line, markers) {
                    found.push(format!("{}:{}: {body:?}", path.display(), index + 1));
                }
            }
        }
    }
    found
}

#[test]
fn hard_copied_widget_copy_stays_within_its_audited_census() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (dir, markers) in HARD_COPY_MARKERS {
        let ceiling = HARD_COPY_CEILINGS
            .iter()
            .find(|(ledger, _)| *ledger == dir)
            .map(|(_, count)| *count)
            .expect("every scanned render tree carries an audited census entry");
        let found = hard_copied_literals(&manifest.join(dir), markers);
        if found.len() > ceiling {
            panic!(
                "{dir}: hard-coded widget copy grew from the audited {ceiling} to {}; \
                 resolve new copy through the shared `t` resolver and catalog (ADR-063):\n{}",
                found.len(),
                found.join("\n")
            );
        }
        assert_eq!(
            found.len(),
            ceiling,
            "{dir}: the audited census says {ceiling} but the tree now holds {}; lower \
             `HARD_COPY_CEILINGS` in the same change so the ledger stays true",
            found.len()
        );
    }
}

/// Negative proof the census detects a painted literal, spares a catalog call,
/// ignores comments, and does not count symbols.
#[test]
fn hard_copy_census_detects_a_painted_literal() {
    let markers = ["text(\""];
    assert_eq!(
        hard_copied_literals_in_line("let a = text(\"Name\");", &markers),
        ["Name"]
    );
    assert!(
        hard_copied_literals_in_line("let a = text(t(\"startup.name\"));", &markers).is_empty()
    );
    assert!(hard_copied_literals_in_line("// let a = text(\"Name\");", &markers).is_empty());
    assert!(hard_copied_literals_in_line("let a = text(\"·\");", &markers).is_empty());
    assert!(
        hard_copied_literals_in_line(
            "TableColumn::text(\"name\", \"common.name\", ColumnWidth::Fill)",
            &markers
        )
        .is_empty(),
        "a column identity is not painted copy"
    );
}

/// The standard must stay registered end-to-end: charter invariant → ADR →
/// quality-gate table → ADR index → task routing → the real gate names and
/// their blocking execution layer. Deleting one link (or renaming a gate in
/// one place) weakens the standard without breaking a compiler, so it is
/// asserted mechanically here.
#[test]
fn public_resource_single_authority_is_registered_end_to_end() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let read = |relative: &str| {
        fs::read_to_string(manifest.join(relative))
            .unwrap_or_else(|_| panic!("{relative} must exist for the standard to stay registered"))
    };
    let charter = read("AGENTS.md");
    let standards = read("docs/STANDARDS.md");
    let adr = read("adr/063-public-resource-single-authority.md");
    let gates = read("docs/QUALITY_GATES.md");
    let index = read("adr/README.md");
    let routing = read("docs/README.md");
    let runner = read("scripts/quality/local-gates.sh");
    let workflow = read(".github/workflows/ci.yml");

    assert!(
        charter.contains("ADR-063") && charter.contains("frontends bind, never fork"),
        "AGENTS.md must state the one-authority invariant"
    );
    // The type and settings axes are named in the same invariant, so a new
    // frontend cannot treat them as frontend-local.
    assert!(
        charter.contains("taskmanager-theme` owns all design tokens")
            && charter.contains("No second dictionary"),
        "AGENTS.md must name the theme owner for tokens as well as copy"
    );
    assert!(
        standards.contains("ADR-063") && standards.contains("公共资源单源门"),
        "the eight closure questions must require the ADR-063 gate"
    );
    assert!(
        adr.contains("## Decision") && adr.contains("only_the_owner_holds_the_message_layer"),
        "ADR-063 must own the decision and name its gates"
    );
    assert!(
        gates.contains("公共资源单源门（ADR-063）") && gates.contains("standard `nextest-logic`"),
        "QUALITY_GATES.md must register the gate at its blocking layer"
    );
    assert!(
        index.contains("[063](063-public-resource-single-authority.md)"),
        "the ADR index must link ADR-063"
    );
    assert!(
        routing.contains("ADR-063") && routing.contains("`locales/*`"),
        "the task routing table must point copy/locale edits at ADR-063"
    );
    assert!(
        runner.contains("run_stage nextest-logic standard cargo nextest run")
            && runner.contains("-p taskmanager-gates --test logic -j 4"),
        "the standard gate must actually run the logic test binary"
    );
    assert!(
        workflow.contains("cargo nextest run --locked --workspace --all-targets"),
        "CI's test job must execute the workspace test binaries"
    );

    let source = read("tests/logic/quality_gate_test.rs");
    for gate in [
        "fn only_the_owner_holds_the_message_layer()",
        "fn no_frontend_paints_chinese_copy_the_catalog_does_not_own()",
        "fn every_i18n_t_callsite_literal_exists_in_the_catalog()",
        "fn every_dynamic_i18n_key_producer_resolves_in_both_catalogs()",
        "fn english_and_chinese_locale_keys_match_except_explicit_fallback_fixture()",
        "fn hard_copied_widget_copy_stays_within_its_audited_census()",
    ] {
        assert!(
            source.contains(gate),
            "ADR-063 gate {gate} must exist in the logic test binary"
        );
    }

    // The type/settings half of the same standard lives in its own source
    // inspection module; deleting it would silently reopen the fork it closed.
    let typography = read("tests/logic/typography_authority_gate_test.rs");
    let logic_manifest = read("tests/logic.rs");
    assert!(
        logic_manifest.contains("logic/typography_authority_gate_test.rs"),
        "the typography gate must be registered in the logic test binary"
    );
    for gate in [
        "fn no_frontend_spells_a_font_family_name()",
        "fn no_frontend_spells_a_hardcoded_type_size()",
        "fn no_frontend_spells_a_hardcoded_font_weight()",
        "fn no_frontend_reimplements_a_theme_axis_codec()",
        "fn the_bundled_font_collection_is_the_asset_table()",
        "fn the_public_axis_codecs_round_trip_and_match_the_wire_tokens()",
    ] {
        assert!(
            typography.contains(gate),
            "ADR-063 typography gate {gate} must exist"
        );
    }
    for control in [
        "fn type_size_scan_detects_a_literal_and_spares_a_token()",
        "fn weight_scan_detects_a_literal_and_spares_a_token()",
        "fn axis_codec_scan_detects_a_private_table()",
    ] {
        assert!(
            typography.contains(control),
            "ADR-063 typography gate {control} must prove its scanner is not vacuous"
        );
    }
    assert!(
        typography.contains("//! source-inspection: static-policy"),
        "the typography gate must declare its source-inspection class"
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
