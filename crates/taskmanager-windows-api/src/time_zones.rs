//! Audited local time-zone rule queries for the Windows API boundary.
//!
//! `GetDynamicTimeZoneInformation` supplies the active zone's registry key
//! name plus the dynamic-rule token; `GetTimeZoneInformationForYear` resolves
//! that token into per-year standard/daylight rules for a bounded year window
//! around the current UTC year. Results leave this module only as typed owned
//! values: per-year offsets in seconds east of UTC and DST transitions
//! already resolved to Unix UTC seconds. The wall-clock to UTC arithmetic is
//! a pure function over a neutral mirror of the native struct, so it stays
//! fixture-testable on every host; nothing here synthesizes a transition the
//! operating system did not report, and inconsistent native data becomes a
//! typed failure rather than a guessed rule.

use crate::WindowsApiError;

/// Years before the current one covered by the rule window.
pub const YEAR_WINDOW_BACK: u16 = 1;
/// Years after the current one covered by the rule window.
pub const YEAR_WINDOW_FORWARD: u16 = 2;
/// Same offset bound the platform-neutral TZif parser enforces; kept as a
/// private duplicate so this boundary crate stays independent of `core`.
const MAX_UTC_OFFSET_SECONDS: u32 = 93_600;

/// One recurring monthly DST transition as Windows reports it.
///
/// `week` uses the native convention: 1..=4 select the nth weekday of the
/// month and 5 selects the last occurrence. The wall-clock fields describe
/// local civil time in the offset in effect before the transition, exactly as
/// the native struct reports them; `at_utc_seconds` is the same instant
/// resolved against that offset for the owning year. Milliseconds are
/// truncated (Unix-second resolution).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowsTransitionRule {
    /// Calendar month of the transition, 1..=12.
    pub month: u16,
    /// 1..=4 for the nth weekday of the month, 5 for the last.
    pub week: u16,
    /// Day of week, 0 = Sunday ..= 6 = Saturday.
    pub day_of_week: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    /// Transition instant as Unix UTC seconds.
    pub at_utc_seconds: i64,
}

/// One year's resolved zone rules.
///
/// `daylight_offset_seconds`/`daylight_start`/`daylight_end` are all present
/// or all absent: a year either observes DST or it does not. Southern zones
/// legitimately report `daylight_end` earlier in the year than
/// `daylight_start` (DST spans the new year).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowsYearZoneRule {
    pub year: u16,
    /// Seconds east of UTC during standard time.
    pub standard_offset_seconds: i32,
    /// Seconds east of UTC during daylight time, if the year observes DST.
    pub daylight_offset_seconds: Option<i32>,
    /// Entering daylight time (wall clock read in standard time).
    pub daylight_start: Option<WindowsTransitionRule>,
    /// Leaving daylight time (wall clock read in daylight time).
    pub daylight_end: Option<WindowsTransitionRule>,
}

/// Typed per-year local-time rules for the active Windows time zone.
///
/// `years` is non-empty and strictly ascending. A consumer renders the last
/// explicit year as the horizon; nothing in this struct extrapolates rules
/// Windows did not report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsTimeZoneRules {
    /// Registry key name of the active zone (e.g. `Pacific Standard Time`),
    /// typed absence when Windows reports an empty name.
    pub zone_key_name: Option<String>,
    pub years: Vec<WindowsYearZoneRule>,
}

/// Query the active time zone's rules for the bounded year window.
#[must_use = "inspect the time-zone rule query result"]
pub fn query_time_zone_rules() -> Result<WindowsTimeZoneRules, WindowsApiError> {
    #[cfg(windows)]
    {
        query_time_zone_rules_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Neutral mirror of the native `SYSTEMTIME` transition date. The mirror
/// keeps the native-to-typed conversion a pure function that fixture tests
/// can run on every host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RawTransitionDate {
    pub year: u16,
    pub month: u16,
    pub day_of_week: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub millisecond: u16,
}

/// Neutral mirror of the `TIME_ZONE_INFORMATION` fields this boundary
/// consumes; names are dropped because no typed rule field carries them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RawTimeZoneInformation {
    pub bias_minutes: i32,
    pub standard_date: RawTransitionDate,
    pub standard_bias_minutes: i32,
    pub daylight_date: RawTransitionDate,
    pub daylight_bias_minutes: i32,
}

/// Convert one year's native rule record into typed rules. Invalid or
/// inconsistent native data is a typed failure, never a guessed rule.
pub(crate) fn year_rule_from_raw(
    year: u16,
    raw: &RawTimeZoneInformation,
) -> Result<WindowsYearZoneRule, WindowsApiError> {
    let standard_offset_seconds = offset_seconds(raw.bias_minutes, raw.standard_bias_minutes)?;
    let observes_dst = raw.standard_date.month != 0 || raw.daylight_date.month != 0;
    if !observes_dst {
        return Ok(WindowsYearZoneRule {
            year,
            standard_offset_seconds,
            daylight_offset_seconds: None,
            daylight_start: None,
            daylight_end: None,
        });
    }
    let daylight_offset_seconds = offset_seconds(raw.bias_minutes, raw.daylight_bias_minutes)?;
    // DST entry is read against the standard offset in effect before it;
    // DST exit against the daylight offset in effect before it.
    let daylight_start = Some(transition_rule_from_raw(
        year,
        &raw.daylight_date,
        standard_offset_seconds,
    )?);
    let daylight_end = Some(transition_rule_from_raw(
        year,
        &raw.standard_date,
        daylight_offset_seconds,
    )?);
    Ok(WindowsYearZoneRule {
        year,
        standard_offset_seconds,
        daylight_offset_seconds: Some(daylight_offset_seconds),
        daylight_start,
        daylight_end,
    })
}

/// Total offset east of UTC for one season of the native bias pair.
fn offset_seconds(bias_minutes: i32, seasonal_bias_minutes: i32) -> Result<i32, WindowsApiError> {
    let total_minutes = bias_minutes
        .checked_add(seasonal_bias_minutes)
        .ok_or(WindowsApiError::QueryFailed)?;
    let total_seconds = total_minutes
        .checked_mul(60)
        .ok_or(WindowsApiError::QueryFailed)?;
    // Windows bias is minutes to ADD to local time to reach UTC, so seconds
    // east of UTC is the negation.
    let east_seconds = total_seconds
        .checked_neg()
        .ok_or(WindowsApiError::QueryFailed)?;
    if east_seconds.unsigned_abs() > MAX_UTC_OFFSET_SECONDS {
        return Err(WindowsApiError::QueryFailed);
    }
    Ok(east_seconds)
}

/// Resolve one native recurring transition into a typed rule with its UTC
/// instant. Only the documented recurring form (year zero, month/week/weekday
/// plus wall clock) is accepted; an absolute or out-of-range date is a typed
/// failure.
fn transition_rule_from_raw(
    year: u16,
    date: &RawTransitionDate,
    offset_before_seconds: i32,
) -> Result<WindowsTransitionRule, WindowsApiError> {
    if date.year != 0 {
        return Err(WindowsApiError::QueryFailed);
    }
    if !(1..=12).contains(&date.month)
        || date.day_of_week > 6
        || !(1..=5).contains(&date.day)
        || date.hour > 23
        || date.minute > 59
        || date.second > 59
    {
        return Err(WindowsApiError::QueryFailed);
    }
    let day = nth_weekday_day_of_month(year, date.month, date.day, date.day_of_week)
        .ok_or(WindowsApiError::QueryFailed)?;
    // Milliseconds are truncated: Unix transitions are second-granular.
    let wall_seconds = civil_seconds(year, date.month, day, date.hour, date.minute, date.second)
        .ok_or(WindowsApiError::QueryFailed)?;
    let at_utc_seconds = wall_seconds
        .checked_sub(i64::from(offset_before_seconds))
        .ok_or(WindowsApiError::QueryFailed)?;
    Ok(WindowsTransitionRule {
        month: date.month,
        week: date.day,
        day_of_week: date.day_of_week,
        hour: date.hour,
        minute: date.minute,
        second: date.second,
        at_utc_seconds,
    })
}

/// Day of month of the `week`-th `day_of_week` in `month` of `year`
/// (`week` 5 = last occurrence). `None` when the occurrence does not exist.
pub(crate) fn nth_weekday_day_of_month(
    year: u16,
    month: u16,
    week: u16,
    day_of_week: u16,
) -> Option<u16> {
    let days_in_month = days_in_month(year, month)?;
    let first_of_month = civil_days(year, month, 1)?;
    // 1970-01-01 was a Thursday; with Sunday = 0 that is day 4.
    let first_day_of_week = (first_of_month + 4).rem_euclid(7);
    let first_occurrence = 1 + (i64::from(day_of_week) - first_day_of_week).rem_euclid(7);
    let day = if week == 5 {
        // Last occurrence: step whole weeks from the first one while
        // staying inside the month.
        first_occurrence + 7 * ((days_in_month - first_occurrence) / 7)
    } else {
        first_occurrence + 7 * i64::from(week - 1)
    };
    if day < 1 || day > days_in_month {
        return None;
    }
    u16::try_from(day).ok()
}

/// Days from the Unix epoch to a civil date (Howard Hinnant's algorithm).
pub(crate) fn civil_days(year: u16, month: u16, day: u16) -> Option<i64> {
    if i64::from(day) < 1 || i64::from(day) > days_in_month(year, month)? {
        return None;
    }
    let year = i64::from(year);
    let month = i64::from(month);
    let day = i64::from(day);
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let year_of_era = y - era * 400;
    let month_prime = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

/// Civil date from days since the Unix epoch; the inverse of
/// [`civil_days`]. Shared shape with the platform-neutral parser's own
/// projection, duplicated here so the boundary stays independent.
pub(crate) fn civil_from_days(days: i64) -> Option<(u16, u16, u16)> {
    let z = i128::from(days) + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i128::from(month <= 2);
    Some((
        u16::try_from(year).ok()?,
        u16::try_from(month).ok()?,
        u16::try_from(day).ok()?,
    ))
}

fn days_in_month(year: u16, month: u16) -> Option<i64> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 if is_leap_year(year) => Some(29),
        2 => Some(28),
        _ => None,
    }
}

const fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

/// Local wall-clock seconds since the Unix epoch for a civil date/time,
/// used as the intermediate before applying the pre-transition offset.
fn civil_seconds(
    year: u16,
    month: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
) -> Option<i64> {
    civil_days(year, month, day)?
        .checked_mul(86_400)?
        .checked_add(i64::from(hour) * 3_600 + i64::from(minute) * 60 + i64::from(second))
}

#[cfg(windows)]
fn query_time_zone_rules_windows() -> Result<WindowsTimeZoneRules, WindowsApiError> {
    use windows::Win32::System::Time::{
        DYNAMIC_TIME_ZONE_INFORMATION, GetDynamicTimeZoneInformation,
        GetTimeZoneInformationForYear, TIME_ZONE_INFORMATION,
    };

    let mut dynamic = DYNAMIC_TIME_ZONE_INFORMATION::default();
    // The returned state (unknown/standard/daylight) describes the current
    // instant, not the rules; only the filled struct is consumed.
    // SAFETY: `dynamic` is a writable caller-owned struct; the call writes
    // only this fixed-size value and retains no pointer.
    let current_state = unsafe { GetDynamicTimeZoneInformation(&mut dynamic) };
    if current_state == u32::MAX {
        return Err(WindowsApiError::QueryFailed);
    }
    let zone_key_name = decode_zone_key_name(&dynamic.TimeZoneKeyName)?;

    let current_year = current_utc_year()?;
    let first_year = current_year.saturating_sub(YEAR_WINDOW_BACK);
    let last_year = current_year.saturating_add(YEAR_WINDOW_FORWARD);
    let mut years = Vec::with_capacity(usize::from(last_year - first_year + 1));
    for year in first_year..=last_year {
        let mut information = TIME_ZONE_INFORMATION::default();
        // SAFETY: `dynamic` is a readable caller-owned struct and
        // `information` a writable caller-owned struct; the synchronous call
        // retains neither pointer.
        unsafe {
            GetTimeZoneInformationForYear(
                year,
                Some(std::ptr::from_ref(&dynamic)),
                &mut information,
            )
        }
        .map_err(|_| WindowsApiError::QueryFailed)?;
        let raw = RawTimeZoneInformation {
            bias_minutes: information.Bias,
            standard_date: raw_transition_date(&information.StandardDate),
            standard_bias_minutes: information.StandardBias,
            daylight_date: raw_transition_date(&information.DaylightDate),
            daylight_bias_minutes: information.DaylightBias,
        };
        years.push(year_rule_from_raw(year, &raw)?);
    }
    Ok(WindowsTimeZoneRules {
        zone_key_name,
        years,
    })
}

#[cfg(windows)]
fn raw_transition_date(date: &windows::Win32::Foundation::SYSTEMTIME) -> RawTransitionDate {
    RawTransitionDate {
        year: date.wYear,
        month: date.wMonth,
        day_of_week: date.wDayOfWeek,
        day: date.wDay,
        hour: date.wHour,
        minute: date.wMinute,
        second: date.wSecond,
        millisecond: date.wMilliseconds,
    }
}

#[cfg(windows)]
fn current_utc_year() -> Result<u16, WindowsApiError> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| WindowsApiError::QueryFailed)?
        .as_secs();
    let (year, _, _) = civil_from_days(
        i64::try_from(seconds)
            .ok()
            .and_then(|seconds| seconds.checked_div(86_400))
            .ok_or(WindowsApiError::QueryFailed)?,
    )
    .ok_or(WindowsApiError::QueryFailed)?;
    Ok(year)
}

/// Decode the zone key name without trusting the fixed buffer to hold a
/// terminator; undecodable text is a typed `InvalidText` failure and an empty
/// name is typed absence.
#[cfg(any(windows, test))]
fn decode_zone_key_name(buffer: &[u16]) -> Result<Option<String>, WindowsApiError> {
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    if end == 0 {
        return Ok(None);
    }
    String::from_utf16(&buffer[..end])
        .map(Some)
        .map_err(|_| WindowsApiError::InvalidText)
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_time_zones.rs"]
mod tests;
