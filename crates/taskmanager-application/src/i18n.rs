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

use crate::process_details_section::ProcessDetailsSection;
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};
use taskforest_core::core::alerts::{AlertMetric, AlertSeverity};
use taskforest_core::core::failure::FailureKind;
use taskforest_core::core::history::HistoryWindow;
use taskforest_core::core::npu::NpuEngineKind;
use taskforest_core::core::services::ServiceAction;
use taskforest_core::core::session::{SessionControlAction, SessionFamily, SessionFamilyFact};
use taskforest_core::core::startup::{StartupControlPolicy, StartupImpact, StartupScope};

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
    /// Every bundled catalog, in picker order. Frontends iterate this source of
    /// truth instead of carrying a drifting hand-written list (ADR-051).
    pub const ALL: [Self; 2] = [Language::En, Language::Zh];

    /// Stable lowercase locale code used as the top-level catalog key. Matches the
    /// `include_str!` filename so adding a locale is a one-place edit.
    pub const fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Zh => "zh",
        }
    }

    /// The catalog key naming this language in its own tongue. Every frontend's
    /// language picker resolves its labels through these keys, so the self-names
    /// stay catalog-owned and no product can fork its own picker vocabulary.
    #[must_use]
    pub const fn self_name_key(self) -> &'static str {
        match self {
            Language::En => "settings.lang_en",
            Language::Zh => "settings.lang_zh",
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

// Temporary language for synchronous fixture construction. Unlike the
// process-wide selection, this override cannot race another frontend's locale
// while a shared demo projection resolves catalog text.
std::thread_local! {
    static LANGUAGE_OVERRIDE: Cell<Option<Language>> = const { Cell::new(None) };
}

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
/// that calls [`t`]. A scoped [`with_language`] call keeps this change local to
/// that synchronous operation; otherwise it updates the shared atomic language.
/// The Settings modal's Language pills call this + `cx.notify()` so the whole
/// shell re-renders localized on the next frame.
pub fn set_language(l: Language) {
    let scoped = LANGUAGE_OVERRIDE.with(|language| {
        if language.get().is_some() {
            language.set(Some(l));
            true
        } else {
            false
        }
    });
    if scoped {
        return;
    }
    LANG.store(lang_to_u8(l), Ordering::Relaxed);
}

/// Resolve catalog text under a temporary language on this thread without
/// changing the process-wide language. The override also contains
/// `set_language` calls made by synchronous construction paths, and nesting
/// restores the prior scoped value on return or unwind.
pub fn with_language<T>(language: Language, operation: impl FnOnce() -> T) -> T {
    let previous = LANGUAGE_OVERRIDE.with(|current| current.replace(Some(language)));
    let _restore = LanguageOverrideRestore(previous);
    operation()
}

struct LanguageOverrideRestore(Option<Language>);

impl Drop for LanguageOverrideRestore {
    fn drop(&mut self) {
        LANGUAGE_OVERRIDE.with(|current| current.set(self.0));
    }
}

/// The scoped language on this thread, if present; otherwise the active shared
/// language. On the very first unscoped call this seeds the global from
/// [`detect_language`] (compare-exchange on the `UNINIT` sentinel; first writer
/// wins — concurrent detectors resolve to the same host value, so the winner is
/// irrelevant). Subsequent calls just read the stored value.
pub fn current_language() -> Language {
    if let Some(language) = LANGUAGE_OVERRIDE.with(Cell::get) {
        return language;
    }
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
pub fn alert_severity_label(severity: AlertSeverity) -> &'static str {
    match severity {
        AlertSeverity::Info => t("alert.info"),
        AlertSeverity::Warning => t("alert.warning"),
        AlertSeverity::Critical => t("alert.critical"),
    }
}

/// Shared localized label for the session-family fact, including its
/// availability. All four frontends call this so "observed unknown",
/// "unobserved", and "unavailable" keep the same meaning and wording.
#[must_use]
pub fn session_family_label(family: SessionFamilyFact) -> &'static str {
    match family {
        SessionFamilyFact::Observed(SessionFamily::Graphical) => t("users.family_graphical"),
        SessionFamilyFact::Observed(SessionFamily::Console) => t("users.family_console"),
        SessionFamilyFact::Observed(SessionFamily::Remote) => t("users.family_remote"),
        SessionFamilyFact::Observed(SessionFamily::Unknown) => t("users.family_unknown"),
        SessionFamilyFact::Unobserved => t("users.family_unobserved"),
        SessionFamilyFact::Unavailable => t("users.family_unavailable"),
    }
}

/// Localized label for one alert metric.
///
/// The metric vocabulary and its catalog keys are owned by
/// `taskforest-core`; resolving them against the active language is owned
/// here. Every frontend reads alert metric names through this single fold so
/// the GPUI, Iced, TUI and Bevy surfaces cannot drift on the same metric.
#[must_use]
pub fn alert_metric_label(metric: AlertMetric) -> &'static str {
    t(metric.i18n_key())
}

/// Localized label for one service lifecycle action.
///
/// `ServiceAction` owns the action vocabulary and its catalog keys; this
/// function owns the one resolution of those keys against the active
/// language. Frontends must not compose `t(action.i18n_key())` themselves.
#[must_use]
pub fn service_action_label(action: ServiceAction) -> &'static str {
    t(action.i18n_key())
}

/// Localized label for one NPU engine kind.
///
/// The engine-kind vocabulary lives in `taskforest-core`; this is the one
/// place where its catalog key meets the active language, so the four
/// frontends cannot name the same engine differently.
#[must_use]
pub fn npu_engine_kind_label(kind: NpuEngineKind) -> &'static str {
    t(kind.i18n_key())
}

/// Localized label for one session control action.
///
/// Every surface that offers "Disconnect" or "Lock" on a login session reads
/// the word from this fold, so the GPUI, Iced, TUI and Bevy session menus
/// cannot name the same verb differently.
#[must_use]
pub fn session_control_action_label(action: SessionControlAction) -> &'static str {
    t(action.i18n_key())
}

/// Localized label for one startup impact tier.
#[must_use]
pub fn startup_impact_label(impact: StartupImpact) -> &'static str {
    t(impact.i18n_key())
}

/// Localized label for one startup scope.
#[must_use]
pub fn startup_scope_label(scope: StartupScope) -> &'static str {
    t(scope.i18n_key())
}

/// Localized label for one startup control policy.
#[must_use]
pub fn startup_control_policy_label(policy: StartupControlPolicy) -> &'static str {
    t(policy.i18n_key())
}

/// Localized verb for one startup enable/disable request. The two verbs are a
/// typed boolean at the platform boundary, so this fold is the one place that
/// turns it into catalog copy; no caller may spell "enable"/"disable" itself.
#[must_use]
pub fn startup_action_label(enabled: bool) -> &'static str {
    t(if enabled {
        "startup.enable"
    } else {
        "startup.disable"
    })
}

/// The localized "… {count} more" overflow label.
///
/// A bounded list that hides rows says so with one wording in every
/// frontend; the placeholder spelling belongs to the catalog, and the
/// interpolation rule belongs here.
#[must_use]
pub fn more_rows_label(hidden: usize) -> String {
    t("common.more_rows").replace("{count}", &hidden.to_string())
}

/// The localized choice label for a history replay window.
///
/// The window's catalog key is core vocabulary
/// ([`HistoryWindow::i18n_key`]); this resolver is the single place where a
/// key meets the active language, so the four frontends cannot disagree about
/// how a window is spelled.
#[must_use]
pub fn history_window_label(window: HistoryWindow) -> &'static str {
    t(window.i18n_key())
}

/// The localized label of one process-properties section tab.
///
/// The section's catalog key is product vocabulary
/// ([`ProcessDetailsSection::i18n_key`]); this resolver is the single place
/// where that key meets the active language, so the four frontends cannot
/// disagree about how a section tab is spelled.
#[must_use]
pub fn process_details_section_label(section: ProcessDetailsSection) -> &'static str {
    t(section.i18n_key())
}

/// The one localized reason for a typed control failure. `FailureKind` owns
/// the key vocabulary (a pure core fact); this is the only place a key meets
/// the active language, so no layer below or above it can fork the copy.
#[must_use]
pub fn control_error_detail(failure: FailureKind) -> &'static str {
    t(failure.i18n_key())
}

/// The rejection copy for a refused service-log stream request: the catalog
/// sentence with the shared localized reason interpolated.
///
/// Every frontend that refuses a stream says so with one wording; the
/// placeholder spelling belongs to the catalog, and the interpolation rule
/// belongs here — never in a view. A `Debug` dump of the typed failure is
/// never the reason (it is an English enum variant, not copy).
#[must_use]
pub fn service_log_rejection_detail(failure: FailureKind) -> String {
    t("service.log_request_rejected").replace("{reason}", control_error_detail(failure))
}

/// One control outcome's user-facing text.
///
/// Session, startup and service control outcomes all answer the same question
/// ("what happened to the action on this target"), so they share one
/// composition: the action label from its own typed vocabulary, the target,
/// and — on failure — the shared localized reason. `accepted` selects the
/// session-specific "request accepted" sentence (dispatch acceptance is not
/// completion) versus the generic success sentence. A `Debug` dump of the
/// typed failure is never the reason.
#[must_use]
pub fn control_outcome_text(
    action: &str,
    target: &str,
    accepted: bool,
    result: Result<(), FailureKind>,
) -> String {
    match result {
        Ok(()) => {
            let template = if accepted {
                "feedback.session_request_accepted"
            } else {
                "feedback.action_succeeded"
            };
            t(template)
                .replace("{action}", action)
                .replace("{target}", target)
        }
        Err(failure) => t("feedback.action_failed_detail")
            .replace("{action}", action)
            .replace("{target}", target)
            .replace("{detail}", control_error_detail(failure)),
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

/// Format an hour/minute duration with locale-specific units. The numeric
/// fields stay zero-padded so compact readouts remain stable across locales.
#[must_use]
pub fn format_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        t("duration.days_hours_minutes")
            .replace("{days}", &days.to_string())
            .replace("{hours}", &format!("{hours:02}"))
            .replace("{minutes}", &format!("{minutes:02}"))
    } else {
        t("duration.hours_minutes")
            .replace("{hours}", &format!("{hours:02}"))
            .replace("{minutes}", &format!("{minutes:02}"))
    }
}

#[cfg(test)]
#[path = "../tests/headless/application_i18n_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/headless/application_control_outcome_i18n_tests.rs"]
mod control_outcome_tests;
