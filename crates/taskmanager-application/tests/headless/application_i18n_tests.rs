use super::*;

/// `t` / `current_language` read a process-wide global, so every assertion
/// lives in ONE sequential test rather than parallel `#[test]`s — the
/// language never changes under foot mid-assertion. Covers the four cases
/// the i18n contract promises: en-known, zh-known, zh→en fallback, and
/// missing-entirely → key.
#[test]
fn t_resolves_and_falls_back() {
    // 1. En: a known key resolves to the en string.
    set_language(Language::En);
    assert_eq!(t("tab.performance"), "Performance");
    assert_eq!(t("settings.skin"), "Skin");
    assert_eq!(t("chrome.close"), "Close");

    // 2. Zh: a known key resolves to the zh string.
    set_language(Language::Zh);
    assert_eq!(t("tab.performance"), "性能");
    assert_eq!(t("settings.skin"), "皮肤");
    assert_eq!(t("chrome.close"), "关闭");

    // 3. Zh fallback to en when a key is present in en but missing from zh.
    set_language(Language::Zh);
    assert_eq!(t("fallback.sample"), "English Only");

    // 4. Missing entirely from every locale: returns the key itself.
    set_language(Language::En);
    assert_eq!(t("no.such.key"), "no.such.key");
    set_language(Language::Zh);
    assert_eq!(t("no.such.key"), "no.such.key");

    // 4b. The exact contract key from P1-I18N-02: a key absent from every
    // locale resolves to its own literal (never empty, never panics) under
    // both languages — the last-resort `key` branch of `t`. This is the
    // graceful-degradation promise every call site relies on.
    set_language(Language::En);
    let miss_en = t("a.definitely.missing.key");
    assert_eq!(miss_en, "a.definitely.missing.key");
    assert!(!miss_en.is_empty());
    set_language(Language::Zh);
    let miss_zh = t("a.definitely.missing.key");
    assert_eq!(miss_zh, "a.definitely.missing.key");
    assert!(!miss_zh.is_empty());

    // current_language tracks the live global.
    set_language(Language::En);
    assert_eq!(current_language(), Language::En);
    set_language(Language::Zh);
    assert_eq!(current_language(), Language::Zh);

    // The Language code is the catalog key (round-trip sanity).
    assert_eq!(Language::En.code(), "en");
    assert_eq!(Language::Zh.code(), "zh");
    assert_eq!(Language::from_code("EN"), Some(Language::En));
    assert_eq!(Language::from_code(" zh "), Some(Language::Zh));
    assert_eq!(Language::from_code("fr"), None);
    assert_eq!(language_from_locale("zh-CN"), Language::Zh);
    assert_eq!(language_from_locale("en-US"), Language::En);

    // Restore En so the rest of the test suite (which assumes the default
    // English host locale) is unaffected by this global mutation.
    set_language(Language::En);
}

/// `detect_language` must be total (no panic on unset vars) and map a `zh*`
/// locale to [`Language::Zh`]. The host `LANG` is whatever the dev/CI box
/// sets, so we only assert the no-panic + type-correctness here; the
/// `zh*` → `Zh` mapping is exercised by the `starts_with` predicate which
/// is trivially correct by inspection.
#[test]
fn detect_language_is_total() {
    let _ = detect_language();
}
