//! Fixture round-trips for the Windows local-time adapter: typed rule
//! inputs are synthesized into TZif by the pure writer and must parse back
//! through the core parser's own validation with the exact offsets and
//! transitions Windows reported.

use super::*;
use taskmanager_core::LocalTimeRules;
use taskmanager_windows_api::{WindowsTransitionRule, WindowsYearZoneRule};

fn transition(month: u16, week: u16, at_utc_seconds: i64) -> WindowsTransitionRule {
    WindowsTransitionRule {
        month,
        week,
        day_of_week: 0,
        hour: 2,
        minute: 0,
        second: 0,
        at_utc_seconds,
    }
}

/// One US Eastern year: DST enters the second Sunday of March at 07:00Z and
/// leaves the first Sunday of November at 06:00Z. Instants below are the
/// documented civil dates resolved against those offsets.
fn eastern_year(year: u16, start: i64, end: i64) -> WindowsYearZoneRule {
    WindowsYearZoneRule {
        year,
        standard_offset_seconds: -18_000,
        daylight_offset_seconds: Some(-14_400),
        daylight_start: Some(transition(3, 2, start)),
        daylight_end: Some(transition(11, 1, end)),
    }
}

fn eastern_rules() -> WindowsTimeZoneRules {
    WindowsTimeZoneRules {
        zone_key_name: Some("Eastern Standard Time".to_string()),
        years: vec![
            // 2025-03-09T07:00Z and 2025-11-02T06:00Z.
            eastern_year(2025, 1_741_503_600, 1_762_063_200),
            // 2026-03-08T07:00Z and 2026-11-01T06:00Z.
            eastern_year(2026, 1_772_953_200, 1_793_512_800),
            // 2027-03-14T07:00Z and 2027-11-07T06:00Z.
            eastern_year(2027, 1_805_007_600, 1_825_567_200),
            // 2028-03-12T07:00Z and 2028-11-05T06:00Z.
            eastern_year(2028, 1_836_457_200, 1_857_016_900),
        ],
    }
}

/// India-like fixed zone: UTC+5:30, no DST in any year.
fn fixed_rules() -> WindowsTimeZoneRules {
    WindowsTimeZoneRules {
        zone_key_name: Some("India Standard Time".to_string()),
        years: (2025..=2028)
            .map(|year| WindowsYearZoneRule {
                year,
                standard_offset_seconds: 19_800,
                daylight_offset_seconds: None,
                daylight_start: None,
                daylight_end: None,
            })
            .collect(),
    }
}

#[test]
fn synthesized_eastern_tzif_parses_with_reported_offsets() {
    let bytes = tzif_bytes(&eastern_rules()).expect("representable eastern rules");
    let rules = LocalTimeRules::from_tzif(&bytes).expect("core parser accepts the synthesis");
    // Winter is standard, summer is daylight, at the exact reported offsets.
    let january = rules
        .offset_at(1_769_058_000) // 2026-01-22T05:00:00Z
        .expect("inside the window");
    assert_eq!(january.seconds_east_of_utc(), -18_000);
    assert!(!january.is_daylight_saving());
    let july = rules
        .offset_at(1_783_915_200) // 2026-07-13T04:00:00Z
        .expect("inside the window");
    assert_eq!(july.seconds_east_of_utc(), -14_400);
    assert!(july.is_daylight_saving());
    // The window's leading anchor instant resolves to standard time, so the
    // whole first year answers queries, not only post-March instants.
    let anchor = rules
        .offset_at(1_735_689_600) // 2025-01-01T00:00:00Z
        .expect("anchor instant resolves");
    assert_eq!(anchor.seconds_east_of_utc(), -18_000);
    assert!(!anchor.is_daylight_saving());
    // The horizon is exactly the last explicit transition: the parser owns
    // the recurrence boundary, so nothing beyond it is guessed.
    assert_eq!(
        rules.valid_through_utc_seconds(),
        Some(1_857_016_900) // 2028-11-05 DST exit
    );
}

#[test]
fn synthesized_fixed_zone_is_valid_without_a_horizon() {
    let bytes = tzif_bytes(&fixed_rules()).expect("representable fixed rules");
    let rules = LocalTimeRules::from_tzif(&bytes).expect("core parser accepts the synthesis");
    assert_eq!(rules.valid_through_utc_seconds(), None);
    for probe in [0_i64, 1_700_000_000, 2_000_000_000] {
        let offset = rules.offset_at(probe).expect("fixed rules never expire");
        assert_eq!(offset.seconds_east_of_utc(), 19_800);
        assert!(!offset.is_daylight_saving());
    }
}

#[test]
fn southern_window_starts_in_daylight_time() {
    // Southern zone (UTC+10 standard / UTC+11 daylight) whose DST spans the
    // new year: leaving DST in April, entering again in October.
    let year = |year: u16, enter: i64, leave: i64| WindowsYearZoneRule {
        year,
        standard_offset_seconds: 36_000,
        daylight_offset_seconds: Some(39_600),
        daylight_start: Some(transition(10, 1, enter)),
        daylight_end: Some(WindowsTransitionRule {
            month: 4,
            week: 1,
            day_of_week: 0,
            hour: 3,
            minute: 0,
            second: 0,
            at_utc_seconds: leave,
        }),
    };
    let rules = WindowsTimeZoneRules {
        zone_key_name: None,
        years: vec![
            // 2026: leaves 2026-04-05T03:00 +11, enters 2026-10-04T02:00 +10.
            year(2026, 1_791_043_200, 1_775_318_400),
            // 2027: leaves 2027-04-04T03:00 +11, enters 2027-10-03T02:00 +10.
            year(2027, 1_822_492_800, 1_806_768_000),
        ],
    };
    let bytes = tzif_bytes(&rules).expect("representable southern rules");
    let parsed = LocalTimeRules::from_tzif(&bytes).expect("core parser accepts the synthesis");
    // January 2026 is still inside the DST period that began in October 2025,
    // so the leading anchor carries the daylight type.
    let january = parsed
        .offset_at(1_769_058_000) // 2026-01-22T05:00:00Z
        .expect("inside the window");
    assert_eq!(january.seconds_east_of_utc(), 39_600);
    assert!(january.is_daylight_saving());
    // Southern mid-winter is standard time.
    let july = parsed
        .offset_at(1_783_915_200) // 2026-07-13T04:00:00Z
        .expect("inside the window");
    assert_eq!(july.seconds_east_of_utc(), 36_000);
    assert!(!july.is_daylight_saving());
}

#[test]
fn a_changed_daylight_offset_between_dst_years_is_representable() {
    // The daylight offset moves while the standard offset holds: the March
    // transition itself carries the new daylight offset, so no instant is
    // missing. (2007-style US rule change shape, compressed into one year.)
    let rules = WindowsTimeZoneRules {
        zone_key_name: None,
        years: vec![
            eastern_year(2026, 1_772_953_200, 1_793_512_800),
            WindowsYearZoneRule {
                year: 2027,
                standard_offset_seconds: -18_000,
                daylight_offset_seconds: Some(-13_800), // 3h30m daylight
                daylight_start: Some(transition(3, 2, 1_805_007_600)),
                daylight_end: Some(transition(11, 1, 1_825_567_200)),
            },
        ],
    };
    let bytes = tzif_bytes(&rules).expect("representable rules");
    let parsed = LocalTimeRules::from_tzif(&bytes).expect("core parser accepts the synthesis");
    let summer = parsed
        .offset_at(1_814_400_000) // 2027-07-01T00:00:00Z
        .expect("inside the window");
    assert_eq!(summer.seconds_east_of_utc(), -13_800);
    assert!(summer.is_daylight_saving());
}

#[test]
fn a_silent_fixed_offset_change_is_refused_not_fabricated() {
    // 2027 abandons DST on the old daylight offset: Windows reports no
    // change instant, so the writer must refuse rather than invent one.
    let mut rules = eastern_rules();
    rules.years[1] = WindowsYearZoneRule {
        year: 2026,
        standard_offset_seconds: -18_000,
        daylight_offset_seconds: Some(-14_400),
        daylight_start: Some(transition(3, 2, 1_772_953_200)),
        daylight_end: Some(transition(11, 1, 1_793_512_800)),
    };
    rules.years.truncate(2);
    rules.years.push(WindowsYearZoneRule {
        year: 2027,
        standard_offset_seconds: -14_400,
        daylight_offset_seconds: None,
        daylight_start: None,
        daylight_end: None,
    });
    assert!(tzif_bytes(&rules).is_none());
}

#[test]
fn conflicting_transition_instants_are_rejected_not_guessed() {
    // 2028's DST exit and 2029's DST entry claim the same instant with
    // different types; no strictly increasing table can express that.
    let mut rules = eastern_rules();
    let collision = 1_857_016_900; // 2028-11-05 exit instant
    rules.years[3].daylight_end = Some(transition(11, 1, collision));
    rules.years.push(eastern_year(
        2029,
        collision,
        1_888_466_400, // 2029-11-04T06:00Z
    ));
    assert!(tzif_bytes(&rules).is_none());
}

#[test]
fn an_empty_year_table_is_not_representable() {
    let rules = WindowsTimeZoneRules {
        zone_key_name: None,
        years: Vec::new(),
    };
    assert!(tzif_bytes(&rules).is_none());
}

#[test]
fn posix_footer_and_designation_text_is_derived_from_offsets_only() {
    assert_eq!(designation(-18_000), "-05");
    assert_eq!(designation(19_800), "+05:30");
    assert_eq!(designation(0), "+00");
    assert_eq!(posix_offset(-18_000), "5");
    assert_eq!(posix_offset(19_800), "-5:30");
    assert_eq!(posix_offset(-14_400), "4");
    assert_eq!(posix_rule(3, 2, 0, 2, 0, 0), "M3.2.0/2");
    assert_eq!(posix_rule(10, 1, 0, 2, 30, 0), "M10.1.0/2:30");
    assert_eq!(posix_rule(4, 1, 0, 3, 0, 30), "M4.1.0/3:00:30");
    // The synthesized eastern payload carries the expected footer string,
    // enclosed in the format's trailing newline pair.
    let bytes = tzif_bytes(&eastern_rules()).expect("representable eastern rules");
    let close = bytes
        .iter()
        .rposition(|&byte| byte == b'\n')
        .expect("footer closes with a newline");
    let open = bytes[..close]
        .iter()
        .rposition(|&byte| byte == b'\n')
        .expect("footer opens with a newline");
    let tail = std::str::from_utf8(&bytes[open + 1..close]).expect("footer stays ASCII");
    assert_eq!(tail, "<-05>5<-04>4,M3.2.0/2,M11.1.0/2");
}

#[test]
fn local_time_observation_stays_typed_unavailable_off_windows() {
    let observation = local_time_rules();
    #[cfg(windows)]
    {
        use taskmanager_core::LocalTimeRulesObservation;
        match &observation {
            LocalTimeRulesObservation::Current { rules, .. } => {
                eprintln!(
                    "LIVE LOCAL TIME: valid through {:?}",
                    rules.valid_through_utc_seconds()
                );
            }
            LocalTimeRulesObservation::Unavailable { .. } => {
                eprintln!("LIVE LOCAL TIME: unavailable on this host");
            }
        }
        assert!(observation.observed_at_ms() > 0);
    }
    #[cfg(not(windows))]
    {
        use taskmanager_core::{FailureKind, LocalTimeRulesObservation};
        assert!(matches!(
            observation,
            LocalTimeRulesObservation::Unavailable {
                failure: FailureKind::Unsupported,
                ..
            }
        ));
    }
}
