//! Tiny self-contained internationalization layer.
//!
//! No external i18n crate is pulled in: the two message catalogs
//! (`LOCALE_EN` / `LOCALE_ZH`) are embedded at compile time via
//! `include_str!` and parsed once (lazily, on first `t` call) into a
//! `&'static str` map — the parsed [`String`] buffers are leaked so the returned
//! references are truly `'static` with no runtime file I/O and no `unsafe`.
//!
//! # Resolution order
//! `t` resolves a `&'static str` message key against the active language
//! (`current_language`, settable at runtime via `set_language`):
//! 1. the active language's catalog,
//! 2. `en` as the fallback locale (so an untranslated key still renders English),
//! 3. the key itself (so a typo / missing key renders its own key rather than
//!    panicking — the "missing entirely" last resort).
//!
//! Keys are `&'static str` so the last-resort fallback is zero-cost: it just
//! hands the caller's literal back. All call sites in this codebase pass string
//! literals, which is the idiomatic i18n pattern.
//!
//! # Scope
//! The catalogs cover shell chrome and the main page copy, including shared
//! list empty states and action outcomes. Backend providers return identifiers
//! and typed/results diagnostics; sentence order belongs here in locale
//! templates so shared UI remains independent of platform commands.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

/// The two locales this app ships. Add a variant + a matching `locales/<code>.json`
/// (and a wire value below) to extend.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Language {
    /// English (the fallback locale — keys missing from another language resolve here).
    En,
    /// Simplified Chinese (zh_*).
    Zh,
}

impl Language {
    /// Stable lowercase locale code used as the top-level catalog key. Matches the
    /// `include_str!` filename so adding a locale is a one-place edit.
    pub const fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Zh => "zh",
        }
    }

    /// Parse the stable config token used by every frontend.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim().to_ascii_lowercase().as_str() {
            "en" => Some(Self::En),
            "zh" => Some(Self::Zh),
            _ => None,
        }
    }
}

// ── active-language global ───────────────────────────────────────────────────
//
// `AtomicU8` wire encoding. `UNINIT` is the cold-start sentinel: the first
// `current_language()` / `t()` call compare-exchanges it to `detect_language()`
// so the app boots in the host locale without any explicit init call. Tests
// override this via `set_language`.
const EN: u8 = 0;
const ZH: u8 = 1;
const UNINIT: u8 = 2;

static LANG: AtomicU8 = AtomicU8::new(UNINIT);

const fn lang_to_u8(l: Language) -> u8 {
    match l {
        Language::En => EN,
        Language::Zh => ZH,
    }
}

const fn u8_to_lang(v: u8) -> Language {
    match v {
        ZH => Language::Zh,
        _ => Language::En,
    }
}

/// Set the active language at runtime. Idempotent; visible on the next render
/// that calls [`t`]. Thread-safe (one relaxed store). The Settings modal's
/// Language pills call this + `cx.notify()` so the whole shell re-renders
/// localized on the next frame.
pub fn set_language(l: Language) {
    LANG.store(lang_to_u8(l), Ordering::Relaxed);
}

/// The active language. On the very first call this seeds the global from
/// [`detect_language`] (compare-exchange on the `UNINIT` sentinel; first writer
/// wins — concurrent detectors resolve to the same host value, so the winner is
/// irrelevant). Subsequent calls just read the stored value.
pub fn current_language() -> Language {
    if LANG.load(Ordering::Relaxed) == UNINIT {
        let detected = lang_to_u8(detect_language());
        let _ = LANG.compare_exchange(UNINIT, detected, Ordering::Relaxed, Ordering::Relaxed);
    }
    u8_to_lang(LANG.load(Ordering::Relaxed))
}

/// Detect a default language from the process environment, mirroring how gettext
/// / glibc pick a locale: `LC_ALL` → `LC_MESSAGES` → `LANG`, first-set wins. Any
/// value whose locale code starts with `zh` (e.g. `zh_CN.UTF-8`, `zh_TW`) maps
/// to [`Language::Zh`]; everything else (including unset / empty / `C`) maps to
/// [`Language::En`]. Never panics.
pub fn detect_language() -> Language {
    let pick = std::env::var("LC_ALL")
        .ok()
        .or_else(|| std::env::var("LC_MESSAGES").ok())
        .or_else(|| std::env::var("LANG").ok());
    match pick {
        Some(s) if s.starts_with("zh") => Language::Zh,
        _ => Language::En,
    }
}

/// Map a native locale name such as `zh-CN` or `en-US` to the closest bundled
/// catalog. This is separate from [`detect_language`]: POSIX environment
/// detection remains useful on Unix, while Windows supplies its locale from
/// the native composition edge.
#[must_use]
pub fn language_from_locale(locale: &str) -> Language {
    if locale.trim().to_ascii_lowercase().starts_with("zh") {
        Language::Zh
    } else {
        Language::En
    }
}

// ── catalog (embedded JSON, parsed + leaked once into 'static refs) ───────────

const LOCALE_EN: &str = include_str!("../../../locales/en.json");
const LOCALE_ZH: &str = include_str!("../../../locales/zh.json");

/// `lang_code -> (key -> text)`. The inner `&'static str`s point into the leaked
/// parsed JSON buffers (see [`leak_map`]).
type Catalog = HashMap<&'static str, HashMap<&'static str, &'static str>>;

static CATALOG: OnceLock<Catalog> = OnceLock::new();

fn catalog() -> &'static Catalog {
    CATALOG.get_or_init(|| {
        let en = parse_locale(LOCALE_EN);
        let zh = parse_locale(LOCALE_ZH);
        let mut out: Catalog = HashMap::with_capacity(2);
        out.insert("en", leak_map(en));
        out.insert("zh", leak_map(zh));
        out
    })
}

/// Parse a flat `{"key": "text"}` JSON object into a `HashMap<String, String>`.
/// A malformed locale file is a build-time authoring error (the file is
/// `include_str!`'d, so it's pinned to the source tree), so the panic message
/// names the contract rather than gracefully degrading.
fn parse_locale(text: &str) -> HashMap<String, String> {
    serde_json::from_str(text)
        .expect("locale JSON must be a flat object of {\"key\": \"text\"} string pairs")
}

/// Move every `(String, String)` pair into a `(&'static str, &'static str)` map
/// by leaking each buffer (see [`leak_str`]). Called exactly twice per process
/// (once per locale) on first [`t`] use; the ~few hundred bytes leaked is the
/// cost of avoiding `unsafe` to promote the parsed buffers to `'static`.
fn leak_map(m: HashMap<String, String>) -> HashMap<&'static str, &'static str> {
    m.into_iter()
        .map(|(k, v)| (leak_str(k), leak_str(v)))
        .collect()
}

/// Promote a [`String`] to `&'static str` by leaking its heap buffer. Safe
/// (`Box::leak` is in the safe std prelude) and idempotent-ish: callers only
/// route compile-time-embedded JSON text through here, so the leak set is fixed
/// at build time and never grows at runtime.
fn leak_str(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Resolve `key` against the active language with `en` and key-self fallbacks.
/// Returns `&'static str` in every branch, so it slots directly into gpui
/// `.child(...)` / `truncated_text(...)` call sites without an owned-String hop.
///
/// See the module docs for the full resolution order.
/// Single-source severity display label (i18n key lookup). Every frontend
/// surfaces alert severities through this so the three copy sites cannot
/// drift (they once duplicated this match in three files).
#[must_use]
pub fn alert_severity_label(severity: taskmanager_core::alerts::AlertSeverity) -> &'static str {
    match severity {
        taskmanager_core::alerts::AlertSeverity::Info => t("alert.info"),
        taskmanager_core::alerts::AlertSeverity::Warning => t("alert.warning"),
        taskmanager_core::alerts::AlertSeverity::Critical => t("alert.critical"),
    }
}

pub fn t(key: &'static str) -> &'static str {
    let lang = current_language();
    let cat = catalog();
    if lang == Language::Zh
        && let Some(m) = cat.get("zh")
        && let Some(v) = m.get(key)
    {
        return v;
    }
    if let Some(m) = cat.get("en")
        && let Some(v) = m.get(key)
    {
        return v;
    }
    // Missing entirely: hand the key literal back so the UI degrades to a
    // recognizable (if ugly) string instead of panicking.
    key
}

#[cfg(test)]
#[path = "../tests/headless/application_i18n_tests.rs"]
mod tests;
