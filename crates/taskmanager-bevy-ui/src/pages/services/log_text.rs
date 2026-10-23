//! Service log text: RFC3339 stamps and level-keyword highlighting.
//!
//! **Ownership**: the shell owns the entries and their typed levels; this
//! module owns only the PRESENTATION of one entry — the timestamp column and
//! the message run split. The provider's realtime stamp is rendered as
//! RFC3339 UTC (second-and-microsecond precision, the value the provider
//! actually issued), in a small monospace run that is visually quiet against
//! the message body; the message body keeps the full mono size.
//!
//! **Keyword highlighting** splits one message into runs: an error keyword
//! wears the destructive red, a warning keyword the amber, everything else
//! the body ink. The split is a pure function over the message text — no
//! provider knowledge, no locale table.
//!
//! **Ink ownership**: the window's role stamp owns the default text ink, so a
//! scene cannot simply declare a color and keep it. The runs therefore carry
//! a typed [`ServiceLogInk`] marker and one system applies the palette ink
//! after the role stamp has run.

use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::system::{Query, Res};
use bevy::scene::{Scene, bsn};
use bevy::text::{FontSize, LineBreak, TextColor, TextFont, TextLayout};
use bevy::ui::prelude::{FlexDirection, Node, Overflow, Val, px};
use bevy::ui::widget::Text;

use crate::palette::UiPalette;
use crate::window::{Role, TextRole};

/// The ink one log text run wears.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LogInk {
    /// The message body of an ordinary entry.
    #[default]
    Base,
    /// The timestamp: present, but quiet.
    Quiet,
    /// An error keyword — and the body of an error-level entry.
    Error,
    /// A warning keyword — and the body of a warning-level entry.
    Warning,
    /// An info keyword.
    Info,
    /// The body of a debug or unknown-level entry.
    Dim,
}

/// The marker on one log text run: the ink it wears. The marker is the run's
/// typed identity — the window's role stamp owns the default ink, and one
/// system applies the palette ink for these runs afterwards.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ServiceLogInk(pub(crate) LogInk);

/// The semantic kind of one highlighted message run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LogSegmentKind {
    /// Ordinary message text.
    Plain,
    /// An error keyword.
    Error,
    /// A warning keyword.
    Warning,
    /// An info keyword.
    Info,
}

/// One run of a log message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LogSegment {
    /// The run's text, verbatim from the message.
    pub(crate) text: String,
    /// The run's semantic kind.
    pub(crate) kind: LogSegmentKind,
}

/// The error vocabulary: the log level word itself.
const ERROR_KEYWORDS: &[&str] = &["error"];

/// The warning vocabulary. Longest-first matching keeps `warning` from being
/// read as `warn`.
const WARNING_KEYWORDS: &[&str] = &["warning", "warn"];

/// The info vocabulary.
const INFO_KEYWORDS: &[&str] = &["info"];

/// The small mono size of the timestamp run, relative to the body size.
const STAMP_FONT_REM: f32 = 0.75;

#[must_use]
fn is_word_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Whether the haystack starts with the keyword, case-insensitively. The
/// keyword may be longer than the haystack — that is a mismatch, not a
/// match.
fn matches_keyword(haystack: &str, keyword: &str) -> bool {
    let mut hay = haystack.as_bytes().iter();
    let mut key = keyword.as_bytes().iter();
    loop {
        let Some(expected) = key.next() else {
            return true;
        };
        let Some(actual) = hay.next() else {
            return false;
        };
        if !actual.eq_ignore_ascii_case(expected) {
            return false;
        }
    }
}

/// The longest keyword that starts at `index`, if any.
fn keyword_at(message: &str, index: usize) -> Option<(LogSegmentKind, usize)> {
    let tail = &message[index..];
    let mut best: Option<(LogSegmentKind, usize)> = None;
    for (kind, keywords) in [
        (LogSegmentKind::Error, ERROR_KEYWORDS),
        (LogSegmentKind::Warning, WARNING_KEYWORDS),
        (LogSegmentKind::Info, INFO_KEYWORDS),
    ] {
        for keyword in keywords {
            if !matches_keyword(tail, keyword) {
                continue;
            }
            let length = keyword.len();
            if best.is_none_or(|(_, current)| length > current) {
                best = Some((kind, length));
            }
        }
    }
    best
}

/// Split one log message into highlighted runs.
///
/// A keyword matches only on a word boundary, so `errored` and `warnings`
/// stay ordinary text. The returned runs always cover the whole message.
#[must_use]
pub(crate) fn log_segments(message: &str) -> Vec<LogSegment> {
    let bytes = message.as_bytes();
    let mut segments: Vec<LogSegment> = Vec::new();
    let mut plain_start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        if index > 0 && is_word_char(bytes[index - 1]) {
            index += 1;
            continue;
        }
        let Some((kind, length)) = keyword_at(message, index) else {
            index += 1;
            continue;
        };
        let end = index + length;
        if end < bytes.len() && is_word_char(bytes[end]) {
            index += 1;
            continue;
        }
        if index > plain_start {
            segments.push(LogSegment {
                text: message[plain_start..index].to_owned(),
                kind: LogSegmentKind::Plain,
            });
        }
        segments.push(LogSegment {
            text: message[index..end].to_owned(),
            kind,
        });
        index = end;
        plain_start = end;
    }
    if plain_start < message.len() {
        segments.push(LogSegment {
            text: message[plain_start..].to_owned(),
            kind: LogSegmentKind::Plain,
        });
    }
    if segments.is_empty() {
        segments.push(LogSegment {
            text: message.to_owned(),
            kind: LogSegmentKind::Plain,
        });
    }
    segments
}

/// The UTC civil date of a day count since the Unix epoch (Howard Hinnant's
/// `civil_from_days`). Pure integer math — no calendar dependency.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_position + 2) / 5 + 1) as u32;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    } as u32;
    (year + i64::from(month <= 2), month, day)
}

/// The RFC3339 UTC stamp of a realtime provider stamp (microseconds since the
/// Unix epoch), e.g. `2026-10-21T09:15:02.482913Z`.
#[must_use]
pub(crate) fn rfc3339_utc(micros: u64) -> String {
    let seconds = micros / 1_000_000;
    let fraction = micros % 1_000_000;
    let days = (seconds / 86_400) as i64;
    let second_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{fraction:06}Z",
        second_of_day / 3_600,
        (second_of_day % 3_600) / 60,
        second_of_day % 60,
    )
}

/// The timestamp column text of one entry. An entry without a realtime stamp
/// renders an honest dash — never a fabricated time.
#[must_use]
pub(crate) fn log_stamp_text(realtime_timestamp_micros: Option<u64>) -> String {
    realtime_timestamp_micros.map_or_else(|| "--:--:--".to_owned(), rfc3339_utc)
}

/// The palette ink of one log run.
#[must_use]
pub(crate) fn log_run_ink(kind: LogInk, palette: &UiPalette) -> Color {
    match kind {
        LogInk::Base => palette.body_color,
        LogInk::Quiet | LogInk::Dim => palette.dim_color,
        LogInk::Error => palette.danger_color,
        LogInk::Warning => palette.warning_color,
        LogInk::Info => palette.accent,
    }
}

/// The timestamp run: small, monospace and quiet — the RFC3339 stamp sits
/// beside the message without competing with it.
pub(crate) fn log_stamp_scene(text: String) -> impl Scene + use<> {
    bsn! {
        Node {
            flex_shrink: 0.0,
            overflow: Overflow::clip_x(),
        }
        Children [
             Text(text)
             TextRole(Role::Mono)
             TextFont { font_size: FontSize::Rem(STAMP_FONT_REM) }
             ServiceLogInk(LogInk::Quiet)
             TextLayout { linebreak: LineBreak::NoWrap }
        ]
    }
}

/// One highlighted message run.
pub(crate) fn log_segment_scene(segment: &LogSegment, base: LogInk) -> impl Scene + use<> {
    let ink = match segment.kind {
        LogSegmentKind::Plain => base,
        LogSegmentKind::Error => LogInk::Error,
        LogSegmentKind::Warning => LogInk::Warning,
        LogSegmentKind::Info => LogInk::Info,
    };
    bsn! {
        Text({ segment.text.clone() })
        TextRole(Role::Mono)
        ServiceLogInk({ ink })
        TextLayout { linebreak: LineBreak::NoWrap }
    }
}

/// The message body: the highlighted runs flow as one continuous line. The
/// `base` ink carries the entry's own level, so an error entry stays red
/// while a keyword inside an ordinary entry still lights up.
pub(crate) fn log_message_scene(message: &str, base: LogInk) -> Box<dyn Scene> {
    let runs: Vec<Box<dyn Scene>> = log_segments(message)
        .iter()
        .map(|segment| Box::new(log_segment_scene(segment, base)) as Box<dyn Scene>)
        .collect();
    Box::new(bsn! {
        Node {
            flex_shrink: 1.0,
            min_width: px(0.0),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(0.0),
            overflow: Overflow::clip_x(),
        }
        Children [
            { runs }
        ]
    })
}

/// Apply the palette ink to the log runs.
///
/// The window's role stamp owns the default text ink and runs when the role
/// lands; this system runs afterwards (the `Last` schedule) so the typed run
/// ink wins.
pub(crate) fn stamp_log_ink(
    palette: Res<crate::window::WindowPalette>,
    mut runs: Query<(&ServiceLogInk, &mut TextColor)>,
) {
    for (ink, mut color) in &mut runs {
        let next = log_run_ink(ink.0, &palette.inner);
        if color.0.to_srgba() != next.to_srgba() {
            color.0 = next;
        }
    }
}
