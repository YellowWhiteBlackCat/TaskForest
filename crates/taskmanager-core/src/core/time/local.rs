//! Validated, platform-neutral local-time rules and pure civil-time projection.

use super::super::FailureKind;
use std::sync::Arc;

const TZIF_HEADER_LEN: usize = 44;
/// Maximum native rule payload accepted at the platform boundary.
pub const MAX_LOCAL_TIME_RULE_BYTES: usize = 1024 * 1024;
const MAX_UTC_OFFSET_SECONDS: u32 = 93_600;

/// Why platform-provided local-time rule bytes were rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalTimeRulesError {
    Empty,
    TooLarge,
    InvalidTzif,
    InvalidOffset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocalTimeType {
    offset_seconds: i32,
    daylight_saving: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocalTimeTransition {
    at_utc_seconds: i64,
    type_index: usize,
}

/// One validated local-time rule set. Construction is the validation boundary;
/// consumers receive only typed transitions and can never inspect raw TZif.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalTimeRules {
    transitions: Arc<[LocalTimeTransition]>,
    types: Arc<[LocalTimeType]>,
    default_type_index: usize,
    valid_through_utc_seconds: Option<i64>,
}

impl LocalTimeRules {
    /// Parse and validate a bounded TZif v1/v2/v3/v4 payload.
    pub fn from_tzif(bytes: &[u8]) -> Result<Self, LocalTimeRulesError> {
        if bytes.is_empty() {
            return Err(LocalTimeRulesError::Empty);
        }
        if bytes.len() > MAX_LOCAL_TIME_RULE_BYTES {
            return Err(LocalTimeRulesError::TooLarge);
        }
        let (counts, block, time_width) = authoritative_block(bytes)?;
        let required = block_size(counts, time_width).ok_or(LocalTimeRulesError::InvalidTzif)?;
        if block.len() < required {
            return Err(LocalTimeRulesError::InvalidTzif);
        }
        if counts.type_count == 0 {
            return Err(LocalTimeRulesError::InvalidTzif);
        }
        let transition_bytes = counts
            .time_count
            .checked_mul(time_width)
            .ok_or(LocalTimeRulesError::InvalidTzif)?;
        let indices_at = transition_bytes;
        let types_at = indices_at
            .checked_add(counts.time_count)
            .ok_or(LocalTimeRulesError::InvalidTzif)?;

        let mut types = Vec::with_capacity(counts.type_count);
        for index in 0..counts.type_count {
            let at = types_at
                .checked_add(
                    index
                        .checked_mul(6)
                        .ok_or(LocalTimeRulesError::InvalidTzif)?,
                )
                .ok_or(LocalTimeRulesError::InvalidTzif)?;
            let offset_seconds = read_i32(block, at).ok_or(LocalTimeRulesError::InvalidTzif)?;
            if offset_seconds.unsigned_abs() > MAX_UTC_OFFSET_SECONDS {
                return Err(LocalTimeRulesError::InvalidOffset);
            }
            let daylight_at = at.checked_add(4).ok_or(LocalTimeRulesError::InvalidTzif)?;
            let daylight_saving = *block
                .get(daylight_at)
                .ok_or(LocalTimeRulesError::InvalidTzif)?
                != 0;
            types.push(LocalTimeType {
                offset_seconds,
                daylight_saving,
            });
        }

        let mut transitions = Vec::with_capacity(counts.time_count);
        let mut previous = None;
        for index in 0..counts.time_count {
            let at = index
                .checked_mul(time_width)
                .ok_or(LocalTimeRulesError::InvalidTzif)?;
            let at_utc_seconds =
                read_signed(block, at, time_width).ok_or(LocalTimeRulesError::InvalidTzif)?;
            if previous.is_some_and(|previous| at_utc_seconds <= previous) {
                return Err(LocalTimeRulesError::InvalidTzif);
            }
            previous = Some(at_utc_seconds);
            let index_at = indices_at
                .checked_add(index)
                .ok_or(LocalTimeRulesError::InvalidTzif)?;
            let type_index = usize::from(
                *block
                    .get(index_at)
                    .ok_or(LocalTimeRulesError::InvalidTzif)?,
            );
            if type_index >= types.len() {
                return Err(LocalTimeRulesError::InvalidTzif);
            }
            transitions.push(LocalTimeTransition {
                at_utc_seconds,
                type_index,
            });
        }
        let default_type_index = types
            .iter()
            .position(|kind| !kind.daylight_saving)
            .unwrap_or(0);
        let has_variable_rules = types.iter().any(|kind| kind.daylight_saving)
            || types.windows(2).any(|pair| pair[0] != pair[1]);
        // TZif v2+ recurrence lives in a POSIX footer. This parser deliberately
        // does not approximate that separate grammar: variable tables are
        // valid only through their last explicit transition. Most system
        // zoneinfo files carry transitions years ahead; after that typed
        // boundary callers get `None`, never a permanently frozen DST offset.
        let valid_through_utc_seconds = has_variable_rules
            .then(|| {
                transitions
                    .last()
                    .map(|transition| transition.at_utc_seconds)
            })
            .flatten();
        Ok(Self {
            transitions: transitions.into(),
            types: types.into(),
            default_type_index,
            valid_through_utc_seconds,
        })
    }

    /// Explicit fixed UTC rules. This is a real rule set, not an unavailable
    /// local zone silently reinterpreted as UTC.
    #[must_use]
    pub fn utc() -> Self {
        Self {
            transitions: Arc::from([]),
            types: Arc::from([LocalTimeType {
                offset_seconds: 0,
                daylight_saving: false,
            }]),
            default_type_index: 0,
            valid_through_utc_seconds: None,
        }
    }

    /// Resolve the offset and DST state that apply at a UTC instant.
    #[must_use]
    pub fn offset_at(&self, at_utc_seconds: i64) -> Option<LocalTimeOffset> {
        if self
            .valid_through_utc_seconds
            .is_some_and(|last| at_utc_seconds > last)
        {
            return None;
        }
        let reached = self
            .transitions
            .partition_point(|transition| transition.at_utc_seconds <= at_utc_seconds);
        let type_index = reached
            .checked_sub(1)
            .map_or(self.default_type_index, |index| {
                self.transitions[index].type_index
            });
        let kind = self.types[type_index];
        Some(LocalTimeOffset {
            seconds_east_of_utc: kind.offset_seconds,
            daylight_saving: kind.daylight_saving,
        })
    }

    /// Last UTC instant covered by explicit transitions. `None` means a fixed
    /// offset rule set is valid without a transition horizon.
    #[must_use]
    pub const fn valid_through_utc_seconds(&self) -> Option<i64> {
        self.valid_through_utc_seconds
    }

    /// Project one UTC instant to its local civil date/time.
    #[must_use]
    pub fn date_time_at(&self, at_utc_seconds: i64) -> Option<LocalDateTime> {
        let offset = self.offset_at(at_utc_seconds)?;
        let local_seconds = at_utc_seconds.checked_add(i64::from(offset.seconds_east_of_utc))?;
        LocalDateTime::from_local_seconds(local_seconds, offset)
    }
}

/// UTC offset plus whether the selected transition type is daylight-saving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTimeOffset {
    seconds_east_of_utc: i32,
    daylight_saving: bool,
}

impl LocalTimeOffset {
    #[must_use]
    pub const fn seconds_east_of_utc(self) -> i32 {
        self.seconds_east_of_utc
    }

    #[must_use]
    pub const fn is_daylight_saving(self) -> bool {
        self.daylight_saving
    }
}

/// Read-only local civil date/time projected from one injected rule set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalDateTime {
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    offset: LocalTimeOffset,
}

impl LocalDateTime {
    fn from_local_seconds(local_seconds: i64, offset: LocalTimeOffset) -> Option<Self> {
        let days = local_seconds.div_euclid(86_400);
        let seconds = local_seconds.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days)?;
        Some(Self {
            year,
            month,
            day,
            hour: (seconds / 3_600).try_into().ok()?,
            minute: ((seconds % 3_600) / 60).try_into().ok()?,
            second: (seconds % 60).try_into().ok()?,
            offset,
        })
    }

    #[must_use]
    pub const fn year(self) -> i32 {
        self.year
    }
    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }
    #[must_use]
    pub const fn day(self) -> u8 {
        self.day
    }
    #[must_use]
    pub const fn hour(self) -> u8 {
        self.hour
    }
    #[must_use]
    pub const fn minute(self) -> u8 {
        self.minute
    }
    #[must_use]
    pub const fn second(self) -> u8 {
        self.second
    }
    #[must_use]
    pub const fn offset(self) -> LocalTimeOffset {
        self.offset
    }
}

/// Cache identity for either current rules or an explicit unavailable state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalTimeRulesCacheKey {
    Current(LocalTimeRules),
    Unavailable(FailureKind),
}

/// One read-only platform observation. `Unsupported` is represented by the
/// normal `FailureKind::Unsupported`; no consumer substitutes UTC for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalTimeRulesObservation {
    Current {
        rules: LocalTimeRules,
        observed_at_ms: u64,
    },
    Unavailable {
        failure: FailureKind,
        observed_at_ms: u64,
    },
}

impl LocalTimeRulesObservation {
    #[must_use]
    pub const fn current(rules: LocalTimeRules, observed_at_ms: u64) -> Self {
        Self::Current {
            rules,
            observed_at_ms,
        }
    }

    #[must_use]
    pub const fn unavailable(failure: FailureKind, observed_at_ms: u64) -> Self {
        Self::Unavailable {
            failure,
            observed_at_ms,
        }
    }

    #[must_use]
    pub const fn unsupported(observed_at_ms: u64) -> Self {
        Self::unavailable(FailureKind::Unsupported, observed_at_ms)
    }

    #[must_use]
    pub const fn rules(&self) -> Option<&LocalTimeRules> {
        match self {
            Self::Current { rules, .. } => Some(rules),
            Self::Unavailable { .. } => None,
        }
    }

    #[must_use]
    pub const fn observed_at_ms(&self) -> u64 {
        match self {
            Self::Current { observed_at_ms, .. } | Self::Unavailable { observed_at_ms, .. } => {
                *observed_at_ms
            }
        }
    }

    #[must_use]
    pub fn cache_key(&self) -> LocalTimeRulesCacheKey {
        match self {
            Self::Current { rules, .. } => LocalTimeRulesCacheKey::Current(rules.clone()),
            Self::Unavailable { failure, .. } => LocalTimeRulesCacheKey::Unavailable(*failure),
        }
    }

    #[must_use]
    pub fn date_time_at(&self, at_utc_seconds: i64) -> Option<LocalDateTime> {
        self.rules()?.date_time_at(at_utc_seconds)
    }

    #[must_use]
    pub fn change_since(&self, previous: &Self) -> LocalTimeRulesChange {
        match (previous, self) {
            (
                Self::Current {
                    rules: previous, ..
                },
                Self::Current { rules: current, .. },
            ) if previous == current => LocalTimeRulesChange::Unchanged,
            (
                Self::Unavailable {
                    failure: previous, ..
                },
                Self::Unavailable {
                    failure: current, ..
                },
            ) if previous == current => LocalTimeRulesChange::Unchanged,
            _ => match (previous.cache_key(), self.cache_key()) {
                (LocalTimeRulesCacheKey::Unavailable(_), LocalTimeRulesCacheKey::Current(_)) => {
                    LocalTimeRulesChange::BecameAvailable
                }
                (
                    LocalTimeRulesCacheKey::Current(_),
                    LocalTimeRulesCacheKey::Unavailable(failure),
                ) => LocalTimeRulesChange::BecameUnavailable { failure },
                (LocalTimeRulesCacheKey::Current(_), LocalTimeRulesCacheKey::Current(_)) => {
                    LocalTimeRulesChange::RulesChanged
                }
                (
                    LocalTimeRulesCacheKey::Unavailable(from),
                    LocalTimeRulesCacheKey::Unavailable(to),
                ) => LocalTimeRulesChange::FailureChanged { from, to },
            },
        }
    }
}

/// Exhaustive transition between two local-time observations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalTimeRulesChange {
    Unchanged,
    BecameAvailable,
    BecameUnavailable { failure: FailureKind },
    RulesChanged,
    FailureChanged { from: FailureKind, to: FailureKind },
}

#[derive(Clone, Copy)]
struct Counts {
    utc_count: usize,
    standard_count: usize,
    leap_count: usize,
    time_count: usize,
    type_count: usize,
    char_count: usize,
}

fn authoritative_block(bytes: &[u8]) -> Result<(Counts, &[u8], usize), LocalTimeRulesError> {
    let (version, counts) = header(bytes)?;
    if version < b'2' {
        return Ok((
            counts,
            bytes
                .get(TZIF_HEADER_LEN..)
                .ok_or(LocalTimeRulesError::InvalidTzif)?,
            4,
        ));
    }
    let first_len = block_size(counts, 4).ok_or(LocalTimeRulesError::InvalidTzif)?;
    let second_at = TZIF_HEADER_LEN
        .checked_add(first_len)
        .ok_or(LocalTimeRulesError::InvalidTzif)?;
    let second = bytes
        .get(second_at..)
        .ok_or(LocalTimeRulesError::InvalidTzif)?;
    let (_, counts) = header(second)?;
    Ok((
        counts,
        second
            .get(TZIF_HEADER_LEN..)
            .ok_or(LocalTimeRulesError::InvalidTzif)?,
        8,
    ))
}

fn header(bytes: &[u8]) -> Result<(u8, Counts), LocalTimeRulesError> {
    if bytes.get(..4) != Some(b"TZif") {
        return Err(LocalTimeRulesError::InvalidTzif);
    }
    let version = *bytes.get(4).ok_or(LocalTimeRulesError::InvalidTzif)?;
    if !matches!(version, 0 | b'2' | b'3' | b'4') {
        return Err(LocalTimeRulesError::InvalidTzif);
    }
    let count = |at| {
        read_u32(bytes, at)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(LocalTimeRulesError::InvalidTzif)
    };
    Ok((
        version,
        Counts {
            utc_count: count(20)?,
            standard_count: count(24)?,
            leap_count: count(28)?,
            time_count: count(32)?,
            type_count: count(36)?,
            char_count: count(40)?,
        },
    ))
}

fn block_size(counts: Counts, time_width: usize) -> Option<usize> {
    counts
        .time_count
        .checked_mul(time_width + 1)?
        .checked_add(counts.type_count.checked_mul(6)?)?
        .checked_add(counts.char_count)?
        .checked_add(counts.leap_count.checked_mul(time_width + 4)?)?
        .checked_add(counts.standard_count)?
        .checked_add(counts.utc_count)
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    Some(u32::from_be_bytes(bytes.get(at..end)?.try_into().ok()?))
}

fn read_i32(bytes: &[u8], at: usize) -> Option<i32> {
    let end = at.checked_add(4)?;
    Some(i32::from_be_bytes(bytes.get(at..end)?.try_into().ok()?))
}

fn read_signed(bytes: &[u8], at: usize, width: usize) -> Option<i64> {
    match width {
        4 => {
            let end = at.checked_add(4)?;
            Some(i64::from(i32::from_be_bytes(
                bytes.get(at..end)?.try_into().ok()?,
            )))
        }
        8 => {
            let end = at.checked_add(8)?;
            Some(i64::from_be_bytes(bytes.get(at..end)?.try_into().ok()?))
        }
        _ => None,
    }
}

// Howard Hinnant's civil_from_days, with day zero at the Unix epoch.
fn civil_from_days(days_since_epoch: i64) -> Option<(i32, u8, u8)> {
    let z = i128::from(days_since_epoch) + 719_468;
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
        year.try_into().ok()?,
        month.try_into().ok()?,
        day.try_into().ok()?,
    ))
}
