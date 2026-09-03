//! Typed ViewModel contract for telemetry stat panels (ARCH.md §8.1
//! "一次折叠，四端渲染", phase 2).
//!
//! One fold, four renderers: producers hand the frontend an
//! already-folded [`StatRow`] list — label plus value with typed
//! presentation semantics — and each renderer consumes it without
//! re-deriving observations from metrics. The GPUI producers fold here
//! today; the iced and TUI frontends migrate onto the same contract when
//! they are touched (触碰迁移律), not before.
//!
//! `value: None` keeps the row: the fact is applicable but uncollected
//! this sample, and the renderer draws the ONE shared dash in its own
//! dim style (`presentation::MISSING_VALUE` for the
//! glyph). A fact that does not exist on the host is omitted by the
//! producer instead of parked as a dash.

/// One stat row: label + value with typed presentation semantics.
/// Renderers MUST NOT fold observations themselves; they consume this.
#[derive(Clone, Debug, PartialEq)]
pub enum StatRow {
    /// Label + value. `None` value = applicable-but-uncollected → renderer
    /// draws the shared dash (dimmed per its own style).
    Text {
        label: String,
        value: Option<String>,
    },
    /// Label + used/total pair already formatted ("x / y").
    Pair {
        label: String,
        value: Option<String>,
    },
    /// Label + three-item trend summary (latest, average, peak) with a canonical full representation.
    Trend {
        label: String,
        latest: String,
        average: String,
        peak: String,
        raw_full: String,
    },
}

impl StatRow {
    /// Constructor sugar mirroring the old tuple producers.
    #[must_use]
    pub fn text(label: impl Into<String>, value: Option<String>) -> Self {
        Self::Text {
            label: label.into(),
            value,
        }
    }

    /// Constructor sugar for used/total pair rows ("x / y" pre-formatted).
    #[must_use]
    pub fn pair(label: impl Into<String>, value: Option<String>) -> Self {
        Self::Pair {
            label: label.into(),
            value,
        }
    }

    /// Constructor sugar for trend rows (latest / avg / peak + raw string).
    #[must_use]
    pub fn trend(
        label: impl Into<String>,
        latest: impl Into<String>,
        average: impl Into<String>,
        peak: impl Into<String>,
        raw_full: impl Into<String>,
    ) -> Self {
        Self::Trend {
            label: label.into(),
            latest: latest.into(),
            average: average.into(),
            peak: peak.into(),
            raw_full: raw_full.into(),
        }
    }

    /// The row label (already locale-resolved by the producer).
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Text { label, .. } | Self::Pair { label, .. } | Self::Trend { label, .. } => {
                label
            }
        }
    }

    /// Present value or `None`-for-dash (applicable but uncollected).
    #[must_use]
    pub fn value(&self) -> Option<&str> {
        match self {
            Self::Text { value, .. } | Self::Pair { value, .. } => value.as_deref(),
            Self::Trend { raw_full, .. } => Some(raw_full.as_str()),
        }
    }

    /// Split parts for multiline / wrap trend layout: (latest, avg, peak).
    #[must_use]
    pub fn trend_parts(&self) -> Option<(&str, &str, &str)> {
        match self {
            Self::Trend {
                latest,
                average,
                peak,
                ..
            } => Some((latest.as_str(), average.as_str(), peak.as_str())),
            Self::Text { .. } | Self::Pair { .. } => None,
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/shell_viewmodel.rs"]
mod tests;
