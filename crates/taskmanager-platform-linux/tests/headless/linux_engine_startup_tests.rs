use super::*;

fn impact_for(source: StartupSource, locator: &str, blame: &HashMap<String, u64>) -> StartupImpact {
    match source {
        StartupSource::UserService => blame
            .get(locator)
            .map(|&ms| StartupImpact::from_millis(ms))
            .unwrap_or(StartupImpact::None),
        StartupSource::DesktopEntry
        | StartupSource::SystemService
        | StartupSource::RunLevel
        | StartupSource::RegistryEntry
        | StartupSource::ScheduledTask
        | StartupSource::LoginItem
        | StartupSource::StartupFolder
        | StartupSource::Other => StartupImpact::None,
    }
}

#[test]
fn mixed_source_status_distinguishes_empty_partial_and_unavailable() {
    assert_eq!(
        source_status(XDG_PROVIDER_ID, 0, None).outcome,
        SourceOutcome::Empty
    );
    assert_eq!(
        source_status(XDG_PROVIDER_ID, 2, Some(FailureKind::PermissionDenied)).outcome,
        SourceOutcome::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(
        source_status(XDG_PROVIDER_ID, 0, Some(FailureKind::PermissionDenied)).outcome,
        SourceOutcome::Unavailable(FailureKind::PermissionDenied)
    );
}

#[test]
fn parse_desktop_entry_basic() {
    let text = "\
[Desktop Entry]
Type=Application
Name=Telegram
Exec=/usr/bin/telegram-desktop -- %u
X-GNOME-Autostart-enabled=true
";
    let e = parse_desktop_entry(text);
    assert_eq!(e.name.as_deref(), Some("Telegram"));
    assert_eq!(e.exec.as_deref(), Some("/usr/bin/telegram-desktop -- %u"));
    assert!(!e.hidden);
}

#[test]
fn parse_desktop_entry_hidden_and_nodisplay_disable() {
    let hidden = "[Desktop Entry]\nName=A\nExec=a\nHidden=true\n";
    assert!(parse_desktop_entry(hidden).hidden);
    let nodisp = "[Desktop Entry]\nName=B\nExec=b\nNoDisplay=true\n";
    assert!(parse_desktop_entry(nodisp).hidden);
    let clean = "[Desktop Entry]\nName=C\nExec=c\n";
    assert!(!parse_desktop_entry(clean).hidden);
}

#[test]
fn parse_desktop_entry_ignores_other_groups_and_localized() {
    // A second group's Name must not leak; localized Name[de] must not
    // overwrite the bare Name fallback.
    let text = "\
[Desktop Entry]
Name=English
Name[de]=Deutsch
Exec=app

[Desktop Action Foo]
Name=Should Not Leak
";
    let e = parse_desktop_entry(text);
    assert_eq!(e.name.as_deref(), Some("English"));
}

// ── autostart_dirs_from_env (pure path resolution) ──────────────────────
#[test]
fn autostart_dirs_from_env_xdg_config_home_beats_home() {
    // When XDG_CONFIG_HOME is set it wins over HOME — no HOME-derived path.
    let dirs = autostart_dirs_from_env(Some("/home/<user>"), Some("/custom/cfg"), None);
    assert_eq!(dirs[0], PathBuf::from("/custom/cfg/autostart"));
    assert!(!dirs.contains(&PathBuf::from("/home/<user>/.config/autostart")));
    // System default still follows.
    assert_eq!(dirs[1], PathBuf::from("/etc/xdg/autostart"));
}

#[test]
fn autostart_dirs_from_env_home_only_uses_dotconfig() {
    // No XDG_CONFIG_HOME ⇒ fall back to HOME/.config/autostart.
    let dirs = autostart_dirs_from_env(Some("/home/<user>"), None, None);
    assert_eq!(dirs.len(), 2);
    assert_eq!(dirs[0], PathBuf::from("/home/<user>/.config/autostart"));
    assert_eq!(dirs[1], PathBuf::from("/etc/xdg/autostart"));
}

#[test]
fn autostart_dirs_from_env_preserves_every_system_priority_segment() {
    let dirs = autostart_dirs_from_env(Some("/home/<user>"), None, Some("/a:/b"));
    assert_eq!(dirs.len(), 3);
    assert_eq!(dirs[1], PathBuf::from("/a/autostart"));
    assert_eq!(dirs[2], PathBuf::from("/b/autostart"));
}

#[test]
fn autostart_dirs_ignore_relative_and_duplicate_roots() {
    let dirs = autostart_dirs_from_env(
        Some("relative-home"),
        Some("relative-config"),
        Some("relative:/a:/a::/b"),
    );
    assert_eq!(
        dirs,
        vec![PathBuf::from("/a/autostart"), PathBuf::from("/b/autostart")]
    );
    assert_eq!(
        user_autostart_dir_from_env(Some("relative"), Some("also-relative")),
        None
    );
}

#[test]
fn autostart_dirs_from_env_all_none_falls_back_to_etc_xdg() {
    // No HOME and no XDG vars ⇒ only the system default remains.
    let dirs = autostart_dirs_from_env(None, None, None);
    assert_eq!(dirs, vec![PathBuf::from("/etc/xdg/autostart")]);
}

// ── parse_systemd_blame (pure) ──────────────────────────────────────────

#[test]
fn parse_systemd_blame_basic_seconds_and_ms() {
    // Seconds round to nearest milli; ms pass through verbatim.
    let out = "\
  1.234s alpha.service
    8.9s beta.service
  567ms gamma.service
";
    let map = parse_systemd_blame(out);
    assert_eq!(map.get("alpha.service"), Some(&1234));
    assert_eq!(map.get("beta.service"), Some(&8900));
    assert_eq!(map.get("gamma.service"), Some(&567));
    assert_eq!(map.len(), 3);
}

#[test]
fn parse_systemd_blame_skips_blank_and_malformed_lines() {
    // Blank lines, header text, and lines missing a unit name are skipped.
    let out = "\
systemd-analyze blame
  1.500s ok.service

  not-a-time
  5s
";
    let map = parse_systemd_blame(out);
    // Only ok.service survived; the garbage lines were dropped.
    assert_eq!(map.len(), 1);
    assert_eq!(map.get("ok.service"), Some(&1500));
}

#[test]
fn parse_systemd_blame_last_write_wins_on_duplicate_unit() {
    // If a unit appears twice (unusual but possible), the last line wins —
    // matching HashMap::insert semantics.
    let out = "\
  100ms dup.service
  5.5s dup.service
";
    let map = parse_systemd_blame(out);
    assert_eq!(map.get("dup.service"), Some(&5500));
}

// ── StartupImpact bucketing ─────────────────────────────────────────────

#[test]
fn startup_impact_bucket_thresholds() {
    // Thresholds: High > 500ms, Medium > 100ms, Low ≤ 100ms.
    assert_eq!(StartupImpact::from_millis(0), StartupImpact::Low);
    assert_eq!(StartupImpact::from_millis(100), StartupImpact::Low);
    assert_eq!(StartupImpact::from_millis(101), StartupImpact::Medium);
    assert_eq!(StartupImpact::from_millis(500), StartupImpact::Medium);
    assert_eq!(StartupImpact::from_millis(501), StartupImpact::High);
    assert_eq!(StartupImpact::from_millis(10_000), StartupImpact::High);
    // Default variant is None.
    assert_eq!(StartupImpact::default(), StartupImpact::None);
    // i18n-key round-trip (the core model is copy-free by contract).
    assert_eq!(StartupImpact::High.i18n_key(), "startup.impact_high");
    assert_eq!(StartupImpact::None.i18n_key(), "startup.impact_none");
}

#[test]
fn impact_for_xdg_is_none_and_systemd_buckets_from_blame() {
    // DesktopEntry is always None regardless of blame data.
    let mut blame = HashMap::new();
    blame.insert("heavy.service".to_string(), 800);
    blame.insert("medium.service".to_string(), 300);
    blame.insert("light.service".to_string(), 50);
    assert_eq!(
        impact_for(StartupSource::DesktopEntry, "anything", &blame),
        StartupImpact::None
    );
    // UserService buckets from the blame map.
    assert_eq!(
        impact_for(StartupSource::UserService, "heavy.service", &blame),
        StartupImpact::High
    );
    assert_eq!(
        impact_for(StartupSource::UserService, "medium.service", &blame),
        StartupImpact::Medium
    );
    assert_eq!(
        impact_for(StartupSource::UserService, "light.service", &blame),
        StartupImpact::Low
    );
    // UserService missing from the map → None (no data for this boot).
    assert_eq!(
        impact_for(StartupSource::UserService, "absent.service", &blame),
        StartupImpact::None
    );
}

#[test]
fn impact_evidence_never_turns_missing_data_into_zero_milliseconds() {
    let measured = BlameSnapshot::Ready(HashMap::from([("fast.service".into(), 0)]));
    assert_eq!(
        impact_evidence_for(StartupSource::UserService, "fast.service", &measured),
        StartupImpactEvidence::Measured { duration_ms: 0 }
    );
    assert_eq!(
        impact_evidence_for(StartupSource::UserService, "missing.service", &measured),
        StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NoRecordForThisBoot
        }
    );
    assert_eq!(
        impact_evidence_for(
            StartupSource::UserService,
            "fast.service",
            &BlameSnapshot::Failed(FailureKind::TimedOut),
        ),
        StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::TimedOut
        }
    );
    assert_eq!(
        impact_evidence_for(StartupSource::DesktopEntry, "app.desktop", &measured),
        StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented
        }
    );
}
