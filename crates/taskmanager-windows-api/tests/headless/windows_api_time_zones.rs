use super::*;

/// US Eastern rules as Windows reports them: bias 300 (UTC = local + 300),
/// no standard-season delta bias, one-hour daylight delta bias, DST entering
/// the second Sunday of March at 02:00 standard wall clock and leaving the
/// first Sunday of November at 02:00 daylight wall clock.
fn eastern_raw() -> RawTimeZoneInformation {
    RawTimeZoneInformation {
        bias_minutes: 300,
        standard_date: RawTransitionDate {
            year: 0,
            month: 11,
            day_of_week: 0,
            day: 1,
            hour: 2,
            minute: 0,
            second: 0,
            millisecond: 0,
        },
        standard_bias_minutes: 0,
        daylight_date: RawTransitionDate {
            year: 0,
            month: 3,
            day_of_week: 0,
            day: 2,
            hour: 2,
            minute: 0,
            second: 0,
            millisecond: 0,
        },
        daylight_bias_minutes: -60,
    }
}

/// A southern-hemisphere shape (e.g. Tasmania): standard UTC+10, daylight
/// UTC+11, DST leaving the first Sunday of April at 03:00 daylight wall
/// clock and entering the first Sunday of October at 02:00 standard wall
/// clock, so DST spans the new year.
fn southern_raw() -> RawTimeZoneInformation {
    RawTimeZoneInformation {
        bias_minutes: -600,
        standard_date: RawTransitionDate {
            year: 0,
            month: 4,
            day_of_week: 0,
            day: 1,
            hour: 3,
            minute: 0,
            second: 0,
            millisecond: 0,
        },
        standard_bias_minutes: 0,
        daylight_date: RawTransitionDate {
            year: 0,
            month: 10,
            day_of_week: 0,
            day: 1,
            hour: 2,
            minute: 0,
            second: 0,
            millisecond: 0,
        },
        daylight_bias_minutes: -60,
    }
}

#[test]
fn civil_date_round_trip_spans_leap_boundaries() {
    for (year, month, day) in [
        (1970, 1, 1),
        (2000, 2, 28),
        (2000, 2, 29),
        (2024, 2, 29),
        (2026, 3, 8),
        (2100, 3, 1),
        (1601, 1, 1),
    ] {
        let days = civil_days(year, month, day).expect("valid civil date");
        assert_eq!(civil_from_days(days), Some((year, month, day)));
    }
    // 1900 and 2100 are common years under the Gregorian rule.
    assert_eq!(civil_days(1900, 2, 29), None);
    assert_eq!(civil_days(2100, 2, 29), None);
    // The epoch is day zero.
    assert_eq!(civil_days(1970, 1, 1), Some(0));
}

#[test]
fn nth_weekday_resolution_matches_calendar_fixtures() {
    // 2026-03-01 is a Sunday: the 2nd Sunday of March 2026 is the 8th.
    assert_eq!(nth_weekday_day_of_month(2026, 3, 2, 0), Some(8));
    // 2026-11-01 is a Sunday: the 1st Sunday of November 2026 is the 1st.
    assert_eq!(nth_weekday_day_of_month(2026, 11, 1, 0), Some(1));
    // October 2026 starts on a Thursday; its Sundays are 4/11/18/25, so the
    // "last" (week 5) Sunday is the 25th — the fourth occurrence, not a
    // fifth one — while the last Thursday is the 29th, a real fifth.
    assert_eq!(nth_weekday_day_of_month(2026, 10, 5, 0), Some(25));
    assert_eq!(nth_weekday_day_of_month(2026, 10, 5, 4), Some(29));
    // May 2026 has five Sundays (3/10/17/24/31): week 5 selects the 31st.
    assert_eq!(nth_weekday_day_of_month(2026, 5, 5, 0), Some(31));
    // 2026-01-01 is a Thursday: the first Thursday is the 1st.
    assert_eq!(nth_weekday_day_of_month(2026, 1, 1, 4), Some(1));
    // The 6th occurrence never exists.
    assert_eq!(nth_weekday_day_of_month(2026, 5, 6, 0), None);
    // February 2026 has exactly four Sundays (1/8/15/22).
    assert_eq!(nth_weekday_day_of_month(2026, 2, 5, 0), Some(22));
}

#[test]
fn eastern_year_rule_resolves_the_documented_2026_transitions() {
    let rule = year_rule_from_raw(2026, &eastern_raw()).expect("eastern rules");
    assert_eq!(rule.standard_offset_seconds, -18_000);
    assert_eq!(rule.daylight_offset_seconds, Some(-14_400));
    // 2026-03-08T02:00 EST == 2026-03-08T07:00Z (Unix 1_772_953_200).
    assert_eq!(
        rule.daylight_start.map(|start| start.at_utc_seconds),
        Some(1_772_953_200)
    );
    // 2026-11-01T02:00 EDT == 2026-11-01T06:00Z (Unix 1_793_512_800).
    assert_eq!(
        rule.daylight_end.map(|end| end.at_utc_seconds),
        Some(1_793_512_800)
    );
    // The civil rule fields survive verbatim for downstream recurrence use.
    assert_eq!(
        rule.daylight_start
            .map(|start| (start.month, start.week, start.day_of_week)),
        Some((3, 2, 0))
    );
}

#[test]
fn southern_year_rule_spans_the_year_boundary() {
    let rule = year_rule_from_raw(2026, &southern_raw()).expect("southern rules");
    assert_eq!(rule.standard_offset_seconds, 36_000);
    assert_eq!(rule.daylight_offset_seconds, Some(39_600));
    // 2026-04-05T03:00 daylight (+11) == 2026-04-04T16:00Z.
    assert_eq!(
        rule.daylight_end.map(|end| end.at_utc_seconds),
        Some(1_775_318_400)
    );
    // 2026-10-04T02:00 standard (+10) == 2026-10-03T16:00Z.
    assert_eq!(
        rule.daylight_start.map(|start| start.at_utc_seconds),
        Some(1_791_043_200)
    );
    // Leaving DST earlier than entering it is the honest southern shape.
    let (Some(end), Some(start)) = (rule.daylight_end, rule.daylight_start) else {
        panic!("southern rules must carry both transitions");
    };
    assert!(end.at_utc_seconds < start.at_utc_seconds);
}

#[test]
fn fixed_zone_year_has_no_dst_fields() {
    let raw = RawTimeZoneInformation {
        bias_minutes: -330,
        standard_date: RawTransitionDate::default(),
        standard_bias_minutes: 0,
        daylight_date: RawTransitionDate::default(),
        daylight_bias_minutes: -60,
    };
    let rule = year_rule_from_raw(2026, &raw).expect("fixed zone rules");
    assert_eq!(rule.standard_offset_seconds, 19_800);
    assert_eq!(rule.daylight_offset_seconds, None);
    assert!(rule.daylight_start.is_none() && rule.daylight_end.is_none());
}

#[test]
fn inconsistent_or_out_of_range_native_rules_are_typed_failures() {
    // DST marker on exactly one of the two transition dates.
    let mut one_sided = eastern_raw();
    one_sided.standard_date.month = 0;
    assert_eq!(
        year_rule_from_raw(2026, &one_sided),
        Err(WindowsApiError::QueryFailed)
    );
    // Absolute-date form is not the documented recurring rule.
    let mut absolute = eastern_raw();
    absolute.daylight_date.year = 2026;
    assert_eq!(
        year_rule_from_raw(2026, &absolute),
        Err(WindowsApiError::QueryFailed)
    );
    // Impossible calendar fields.
    let mut bad_month = eastern_raw();
    bad_month.daylight_date.month = 13;
    assert_eq!(
        year_rule_from_raw(2026, &bad_month),
        Err(WindowsApiError::QueryFailed)
    );
    let mut bad_week = eastern_raw();
    bad_week.daylight_date.day = 6;
    assert_eq!(
        year_rule_from_raw(2026, &bad_week),
        Err(WindowsApiError::QueryFailed)
    );
    let mut bad_hour = eastern_raw();
    bad_hour.daylight_date.hour = 24;
    assert_eq!(
        year_rule_from_raw(2026, &bad_hour),
        Err(WindowsApiError::QueryFailed)
    );
    // Offsets beyond the parser-bound are rejected before leaving the
    // boundary instead of being fabricated into a rule.
    let huge_bias = RawTimeZoneInformation {
        bias_minutes: 100_000,
        ..RawTimeZoneInformation::default()
    };
    assert_eq!(
        year_rule_from_raw(2026, &huge_bias),
        Err(WindowsApiError::QueryFailed)
    );
}

#[test]
fn zone_key_name_decode_rejects_bad_utf16_and_keeps_empty_as_absence() {
    assert_eq!(
        decode_zone_key_name(&[0x0050, 0x0053, 0x0054, 0]),
        Ok(Some("PST".to_string()))
    );
    assert_eq!(decode_zone_key_name(&[0]), Ok(None));
    assert_eq!(decode_zone_key_name(&[]), Ok(None));
    // A lone trailing surrogate is invalid UTF-16.
    assert_eq!(
        decode_zone_key_name(&[0xDC55, 0]),
        Err(WindowsApiError::InvalidText)
    );
    // No terminator: decode to the buffer end rather than reading past it.
    assert_eq!(
        decode_zone_key_name(&[0x0041, 0x0042]),
        Ok(Some("AB".to_string()))
    );
}

#[test]
fn live_time_zone_rule_query() {
    let result = query_time_zone_rules();
    #[cfg(windows)]
    {
        let rules = result.expect("time zone rules");
        eprintln!(
            "LIVE TIME ZONE: {} (years = {})",
            rules.zone_key_name.as_deref().unwrap_or("<unnamed>"),
            rules.years.len()
        );
        assert!(!rules.years.is_empty());
        for pair in rules.years.windows(2) {
            assert!(pair[0].year < pair[1].year, "years must ascend");
        }
        for year in &rules.years {
            // DST facts are all present or all absent — never half a rule.
            assert_eq!(
                year.daylight_offset_seconds.is_some(),
                year.daylight_start.is_some()
            );
            assert_eq!(year.daylight_start.is_some(), year.daylight_end.is_some());
        }
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}
