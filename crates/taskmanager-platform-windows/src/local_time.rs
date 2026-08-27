//! Windows local-time rule discovery at the native platform boundary.
//!
//! The audited boundary returns the active zone's per-year typed rules. This
//! adapter turns those rules into a TZif v2 payload with a pure writer and
//! hands the bytes to the core parser; the parser's own validation is the
//! acceptance gate, so nothing here can smuggle an unvalidated rule into the
//! application. The writer emits exactly the transitions Windows reported
//! (plus a leading anchor so the whole queried year window resolves) and a
//! POSIX footer describing the final year's recurrence. A zone without DST
//! becomes a single-type, zero-transition rule set that is valid forever.
//!
//! This is called by the native composition root, never a renderer. A failed
//! or non-Windows query stays a distinguishable typed failure; none falls
//! back to UTC, and frontends must render unavailable instead.

use std::time::SystemTime;

use taskmanager_core::{FailureKind, LocalTimeRules, LocalTimeRulesObservation, unix_millis};
use taskmanager_windows_api::{WindowsApiError, WindowsTimeZoneRules, WindowsYearZoneRule};

/// Read and validate the Windows process's configured local-time rules.
#[must_use]
pub fn local_time_rules() -> LocalTimeRulesObservation {
    let observed_at_ms = unix_millis(SystemTime::now());
    let rules = match taskmanager_windows_api::query_time_zone_rules() {
        Ok(rules) => rules,
        Err(WindowsApiError::Unsupported) => {
            return LocalTimeRulesObservation::unavailable(
                FailureKind::Unsupported,
                observed_at_ms,
            );
        }
        Err(WindowsApiError::PermissionDenied) => {
            return LocalTimeRulesObservation::unavailable(
                FailureKind::PermissionDenied,
                observed_at_ms,
            );
        }
        Err(_) => {
            return LocalTimeRulesObservation::unavailable(
                FailureKind::ProviderFault,
                observed_at_ms,
            );
        }
    };
    let Some(bytes) = tzif_bytes(&rules) else {
        return LocalTimeRulesObservation::unavailable(FailureKind::ProviderFault, observed_at_ms);
    };
    match LocalTimeRules::from_tzif(&bytes) {
        Ok(rules) => LocalTimeRulesObservation::current(rules, observed_at_ms),
        Err(_) => {
            LocalTimeRulesObservation::unavailable(FailureKind::ProviderFault, observed_at_ms)
        }
    }
}

/// One local-time type in the synthesized type table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TzifType {
    offset_seconds: i32,
    is_daylight: bool,
}

/// Synthesize a TZif v2 payload from typed Windows rules. `None` means the
/// rules are not representable as a strictly increasing transition table —
/// the caller must surface that as a typed failure, never a partial guess.
///
/// The payload's explicit transitions are exactly the reported DST switches
/// over the queried year window; the trailing POSIX footer describes only the
/// final year's recurrence, because that is the last rule Windows stated.
fn tzif_bytes(rules: &WindowsTimeZoneRules) -> Option<Vec<u8>> {
    let first = rules.years.first()?;
    let last = rules.years.last()?;

    let mut types: Vec<TzifType> = Vec::new();
    push_type(&mut types, first.standard_offset_seconds, false);
    if let Some(offset) = first.daylight_offset_seconds {
        push_type(&mut types, offset, true);
    }
    for rule in &rules.years {
        push_type(&mut types, rule.standard_offset_seconds, false);
        if let Some(offset) = rule.daylight_offset_seconds {
            push_type(&mut types, offset, true);
        }
    }
    let type_index = |offset: i32, is_daylight: bool| -> Option<usize> {
        types
            .iter()
            .position(|kind| kind.offset_seconds == offset && kind.is_daylight == is_daylight)
    };

    let mut transitions: Vec<(i64, usize)> = Vec::new();
    // Leading anchor at the window's first instant: before any DST switch the
    // offset is the first year's, which for southern zones is already the
    // daylight one (their DST spans the new year).
    let anchor = january_first_utc(first.year)?;
    let initial_daylight = match (first.daylight_start, first.daylight_end) {
        (Some(start), Some(end)) => end.at_utc_seconds < start.at_utc_seconds,
        _ => false,
    };
    let initial_offset = if initial_daylight {
        first.daylight_offset_seconds?
    } else {
        first.standard_offset_seconds
    };
    transitions.push((anchor, type_index(initial_offset, initial_daylight)?));
    for rule in &rules.years {
        if let (Some(start), Some(end)) = (rule.daylight_start, rule.daylight_end) {
            let daylight = rule.daylight_offset_seconds?;
            transitions.push((start.at_utc_seconds, type_index(daylight, true)?));
            transitions.push((
                end.at_utc_seconds,
                type_index(rule.standard_offset_seconds, false)?,
            ));
        }
    }
    transitions.sort_by_key(|&(at, _)| at);
    // A transition that does not change the active type is a no-op; dropping
    // it keeps the table minimal and preserves strict monotonicity.
    let mut collapsed: Vec<(i64, usize)> = Vec::with_capacity(transitions.len());
    for (at, index) in transitions {
        if collapsed
            .last()
            .is_some_and(|&(_, previous)| previous == index)
        {
            continue;
        }
        collapsed.push((at, index));
    }
    // Conflicting instants (same second, different types) are corrupt input.
    if collapsed.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return None;
    }
    // Entering-state consistency: at each year boundary the table's state
    // must match what that year declared. A year whose fixed offset differs
    // from the previous year's exit state reports a change WITHOUT an
    // instant, and fabricating one is forbidden — the whole window then
    // stays unrepresentable and surfaces as a typed failure. The anchor
    // guarantees some transition exists at or before every year boundary.
    for rule in &rules.years {
        let year_start = january_first_utc(rule.year)?;
        let entering = collapsed
            .iter()
            .rev()
            .find(|&&(at, _)| at <= year_start)
            .map(|&(_, index)| types[index])?;
        let declared = match (rule.daylight_start, rule.daylight_end) {
            // Southern year: January 1 is still inside the DST period that
            // began the previous autumn.
            (Some(start), Some(end)) if end.at_utc_seconds < start.at_utc_seconds => TzifType {
                offset_seconds: rule.daylight_offset_seconds?,
                is_daylight: true,
            },
            _ => TzifType {
                offset_seconds: rule.standard_offset_seconds,
                is_daylight: false,
            },
        };
        if entering != declared {
            return None;
        }
    }

    let v2_block = data_block(&collapsed, &types, 8)?;
    // The legacy 32-bit block mirrors the same table when every transition
    // fits a signed 32-bit second count; otherwise it degrades to an empty
    // block, which is valid for readers that skip to the 64-bit block.
    let v1_fits = collapsed.iter().all(|&(at, _)| i32::try_from(at).is_ok());
    let v1_block = if v1_fits {
        data_block(&collapsed, &types, 4)?
    } else {
        Vec::new()
    };
    let v1_counts = if v1_fits {
        block_counts(&collapsed, &types)
    } else {
        [0; 6]
    };

    let footer = footer(last)?;
    let mut bytes = Vec::with_capacity(88 + v1_block.len() + v2_block.len() + footer.len());
    bytes.extend_from_slice(&header(&v1_counts));
    bytes.extend_from_slice(&v1_block);
    bytes.extend_from_slice(&header(&block_counts(&collapsed, &types)));
    bytes.extend_from_slice(&v2_block);
    bytes.extend_from_slice(&footer);
    Some(bytes)
}

fn push_type(types: &mut Vec<TzifType>, offset_seconds: i32, is_daylight: bool) {
    if !types
        .iter()
        .any(|kind| kind.offset_seconds == offset_seconds && kind.is_daylight == is_daylight)
    {
        types.push(TzifType {
            offset_seconds,
            is_daylight,
        });
    }
}

/// 00:00:00 UTC on January 1 of `year` as Unix seconds.
fn january_first_utc(year: u16) -> Option<i64> {
    // Civil arithmetic duplicated from the boundary's pure helpers; kept
    // local and tiny so this adapter stays independent of boundary internals.
    fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
        let y = if month <= 2 { year - 1 } else { year };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let year_of_era = y - era * 400;
        let month_prime = if month > 2 { month - 3 } else { month + 9 };
        let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }
    days_from_civil(i64::from(year), 1, 1).checked_mul(86_400)
}

fn header(counts: &[u32; 6]) -> [u8; 44] {
    let mut header = [0_u8; 44];
    header[..4].copy_from_slice(b"TZif");
    header[4] = b'2';
    // Bytes 5..20 stay reserved/zero; 20..44 carry the six count words.
    for (index, count) in counts.iter().enumerate() {
        let at = 20 + index * 4;
        header[at..at + 4].copy_from_slice(&count.to_be_bytes());
    }
    header
}

fn block_counts(transitions: &[(i64, usize)], types: &[TzifType]) -> [u32; 6] {
    [
        0, // UT/local indicators
        0, // standard/wall indicators
        0, // leap-second records
        transitions.len() as u32,
        types.len() as u32,
        1, // one empty designation
    ]
}

fn data_block(
    transitions: &[(i64, usize)],
    types: &[TzifType],
    time_width: usize,
) -> Option<Vec<u8>> {
    let mut block = Vec::with_capacity(transitions.len() * (time_width + 1) + types.len() * 6 + 1);
    for &(at, _) in transitions {
        if time_width == 8 {
            block.extend_from_slice(&at.to_be_bytes());
        } else {
            let at = i32::try_from(at).ok()?;
            block.extend_from_slice(&at.to_be_bytes());
        }
    }
    for &(_, index) in transitions {
        block.push(u8::try_from(index).ok()?);
    }
    for kind in types {
        block.extend_from_slice(&kind.offset_seconds.to_be_bytes());
        block.push(u8::from(kind.is_daylight));
        block.push(0); // designation index: the single empty designation
    }
    block.push(0); // the single empty (NUL) designation
    Some(block)
}

/// POSIX TZ footer for the final reported year: `\n<TZ string>\n`. The
/// designations are derived from the offsets themselves so the footer never
/// invents a zone name, and the recurrence clauses mirror the final year's
/// month/week/weekday rules verbatim.
fn footer(last: &WindowsYearZoneRule) -> Option<Vec<u8>> {
    let tz_string = match (last.daylight_start, last.daylight_end) {
        (Some(start), Some(end)) => {
            let daylight = last.daylight_offset_seconds?;
            format!(
                "<{standard}>{standard_posix}<{daylight_designation}>{daylight_posix},{start_rule},{end_rule}",
                standard = designation(last.standard_offset_seconds),
                standard_posix = posix_offset(last.standard_offset_seconds),
                daylight_designation = designation(daylight),
                daylight_posix = posix_offset(daylight),
                start_rule = posix_rule(
                    start.month,
                    start.week,
                    start.day_of_week,
                    start.hour,
                    start.minute,
                    start.second
                ),
                end_rule = posix_rule(
                    end.month,
                    end.week,
                    end.day_of_week,
                    end.hour,
                    end.minute,
                    end.second
                ),
            )
        }
        _ => format!(
            "<{standard}>{standard_posix}",
            standard = designation(last.standard_offset_seconds),
            standard_posix = posix_offset(last.standard_offset_seconds),
        ),
    };
    let mut footer = Vec::with_capacity(tz_string.len() + 2);
    footer.push(b'\n');
    footer.extend_from_slice(tz_string.as_bytes());
    footer.push(b'\n');
    Some(footer)
}

/// Signed hour/minute designation text (`+05:30`, `-08`) for a seconds-east
/// offset. Cosmetic only: the parser never reads designations, and the rule
/// offsets carry the truth.
fn designation(offset_seconds: i32) -> String {
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let magnitude = offset_seconds.unsigned_abs();
    format!(
        "{sign}{:02}{minutes}",
        magnitude / 3_600,
        minutes = if !magnitude.is_multiple_of(3_600) {
            format!(":{:02}", (magnitude % 3_600) / 60)
        } else {
            String::new()
        }
    )
}

/// POSIX offset text: the time added to local time to reach UTC, so the
/// sign is the seconds-east sign negated (`-5`, `5:30`).
fn posix_offset(offset_seconds: i32) -> String {
    let sign = if offset_seconds > 0 { "-" } else { "" };
    let magnitude = offset_seconds.unsigned_abs();
    let mut text = format!("{sign}{}", magnitude / 3_600);
    if !magnitude.is_multiple_of(3_600) {
        text.push_str(&format!(":{:02}", (magnitude % 3_600) / 60));
    }
    if !magnitude.is_multiple_of(60) {
        text.push_str(&format!(":{:02}", magnitude % 60));
    }
    text
}

/// POSIX `Mm.w.d/h[:mm[:ss]]` recurrence clause; week 5 is "last", matching
/// the Windows week convention directly.
fn posix_rule(
    month: u16,
    week: u16,
    day_of_week: u16,
    hour: u16,
    minute: u16,
    second: u16,
) -> String {
    let mut rule = format!("M{month}.{week}.{day_of_week}/{hour}");
    if minute != 0 || second != 0 {
        rule.push_str(&format!(":{minute:02}"));
    }
    if second != 0 {
        rule.push_str(&format!(":{second:02}"));
    }
    rule
}

#[cfg(test)]
#[path = "../tests/headless/platform_windows_local_time.rs"]
mod tests;
