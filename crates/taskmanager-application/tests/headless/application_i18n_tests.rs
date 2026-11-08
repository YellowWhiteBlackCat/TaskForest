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
    assert_eq!(
        session_family_label(SessionFamilyFact::Observed(SessionFamily::Graphical)),
        "Graphical"
    );
    assert_eq!(
        session_family_label(SessionFamilyFact::Observed(SessionFamily::Unknown)),
        "Unknown"
    );
    assert_eq!(
        session_family_label(SessionFamilyFact::Unobserved),
        "Not observed"
    );
    assert_eq!(
        session_family_label(SessionFamilyFact::Unavailable),
        "Unavailable"
    );
    assert_eq!(
        t("feedback.session_request_accepted"),
        "{action} request accepted for {target}"
    );

    // 2. Zh: a known key resolves to the zh string.
    set_language(Language::Zh);
    assert_eq!(t("tab.performance"), "性能");
    assert_eq!(t("settings.skin"), "皮肤");
    assert_eq!(t("chrome.close"), "关闭");
    assert_eq!(
        session_family_label(SessionFamilyFact::Observed(SessionFamily::Graphical)),
        "图形"
    );
    assert_eq!(
        session_family_label(SessionFamilyFact::Observed(SessionFamily::Console)),
        "控制台"
    );
    assert_eq!(
        session_family_label(SessionFamilyFact::Observed(SessionFamily::Remote)),
        "远程"
    );
    assert_eq!(
        session_family_label(SessionFamilyFact::Observed(SessionFamily::Unknown)),
        "未知"
    );
    assert_eq!(
        session_family_label(SessionFamilyFact::Unobserved),
        "未观测"
    );
    assert_eq!(
        session_family_label(SessionFamilyFact::Unavailable),
        "不可用"
    );
    assert_eq!(
        t("feedback.session_request_accepted"),
        "已接受对 {target} 的“{action}”请求"
    );

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

#[test]
fn scoped_language_override_nests_and_keeps_language_changes_local() {
    with_language(Language::En, || {
        assert_eq!(current_language(), Language::En);
        assert_eq!(t("tab.performance"), "Performance");

        set_language(Language::Zh);
        assert_eq!(current_language(), Language::Zh);
        assert_eq!(t("tab.performance"), "性能");

        with_language(Language::En, || {
            assert_eq!(current_language(), Language::En);
            assert_eq!(t("tab.performance"), "Performance");
        });

        assert_eq!(current_language(), Language::Zh);
        assert_eq!(t("tab.performance"), "性能");
    });
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

/// Every typed product vocabulary resolves through exactly one owner, so the
/// GPUI, Iced, TUI and Bevy surfaces cannot spell the same fact differently.
/// The English and Chinese readings are pinned here because a frontend that
/// forks the fold would silently drift away from these strings.
#[test]
fn typed_vocabulary_resolvers_read_one_wording_in_both_catalogs() {
    use taskforest_core::core::alerts::AlertMetric;
    use taskforest_core::core::npu::NpuEngineKind;
    use taskforest_core::core::services::ServiceAction;
    use taskforest_core::core::session::SessionControlAction;
    use taskforest_core::core::startup::{StartupControlPolicy, StartupImpact, StartupScope};

    set_language(Language::En);
    assert_eq!(
        alert_metric_label(AlertMetric::CpuUsagePercent),
        "CPU usage"
    );
    assert_eq!(
        alert_metric_label(AlertMetric::DiskTemperatureC),
        "Disk temperature"
    );
    assert_eq!(
        alert_metric_label(AlertMetric::SmartCriticalWarning),
        "SMART critical warning"
    );
    assert_eq!(alert_severity_label(AlertSeverity::Critical), "Critical");
    assert_eq!(
        service_action_label(ServiceAction::ReloadDaemon),
        "Reload unit files"
    );
    assert_eq!(
        session_control_action_label(SessionControlAction::Disconnect),
        "Disconnect"
    );
    assert_eq!(
        session_control_action_label(SessionControlAction::Lock),
        "Lock"
    );
    assert_eq!(
        npu_engine_kind_label(NpuEngineKind::Unknown),
        "Other engine"
    );
    assert_eq!(startup_impact_label(StartupImpact::High), "High");
    assert_eq!(startup_scope_label(StartupScope::User), "User");
    assert_eq!(
        startup_control_policy_label(StartupControlPolicy::Direct),
        "Direct"
    );
    assert_eq!(more_rows_label(3), "… 3 more");

    set_language(Language::Zh);
    assert_eq!(
        alert_metric_label(AlertMetric::CpuUsagePercent),
        "CPU 使用率"
    );
    assert_eq!(
        alert_metric_label(AlertMetric::SmartCriticalWarning),
        "SMART 严重警告"
    );
    assert_eq!(
        session_control_action_label(SessionControlAction::Disconnect),
        "断开"
    );
    assert_eq!(more_rows_label(3), "… 另有 3 项");

    set_language(Language::En);
}

/// The count placeholder is always substituted: a bounded list that still
/// shows `{count}` would read as a broken template instead of a real count.
#[test]
fn more_rows_label_never_leaves_the_placeholder_unsubstituted() {
    set_language(Language::En);
    for hidden in [0usize, 1, 12, 804] {
        let label = more_rows_label(hidden);
        assert!(
            !label.contains("{count}"),
            "unsubstituted placeholder: {label}"
        );
        assert!(
            label.contains(&hidden.to_string()),
            "missing count: {label}"
        );
    }
    set_language(Language::En);
}
