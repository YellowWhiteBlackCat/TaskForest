//! Boot-evidence rendering for the Startup page: a non-interactive stat-pill
//! strip (failed systemd units + critical-chain total) followed by a bounded
//! waterfall over the measured critical chain.
//!
//! Split out of [`super::startup_view`] (BN-05 / P1-ARCH-10). Pure render
//! functions and their unit tests; holds no UI state of its own — the typed
//! [`StartupBootEvidenceSnapshot`] arrives as a render param.

use gpui::{Div, InteractiveElement, IntoElement, ParentElement, Styled, div, px, relative};

use crate::gpui_app::formatting;
use taskmanager_application::i18n;
use taskmanager_core::core::startup::{
    BootTimeline, DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS, DEFAULT_BOOT_TIMELINE_MAX_UNTIMED,
    StartupBootEvidenceSnapshot,
};
use taskmanager_theme::Color;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

// ── boot evidence strip: failed units + critical chain ──────────────────────
//
// P1-ARCH-10/#11 closure: the typed systemd boot evidence (`systemctl
// --failed` + `systemd-analyze critical-chain`) surfaces here as two
// non-interactive stat pills. Failures stay typed ("boot evidence
// unavailable"), a true empty set reads as "no failed units" — never a
// fabricated zero or a silent absence.

/// Up to this many failed-unit names appear in the summary pill.
const MAX_FAILED_UNIT_NAMES: usize = 2;

/// Pure value text for the failed-units pill. `None` means the strip is
/// omitted entirely (evidence not collected yet); otherwise the text is honest
/// for every typed state: a failure reads "boot evidence unavailable", a true
/// empty set reads `0`, and a populated set shows the count plus up to
/// [`MAX_FAILED_UNIT_NAMES`] unit names.
fn failed_units_summary(evidence: &StartupBootEvidenceSnapshot) -> Option<String> {
    if evidence.failed_units_failure.is_some() {
        return Some(i18n::t("startup.evidence_unavailable").to_string());
    }
    if evidence.failed_units.is_empty() {
        return Some("0".to_string());
    }
    let names = evidence
        .failed_units
        .iter()
        .take(MAX_FAILED_UNIT_NAMES)
        .map(|unit| unit.unit.as_str())
        .collect::<Vec<_>>()
        .join(" · ");
    Some(format!("{} · {names}", evidence.failed_units.len()))
}

/// Pure value text for the critical-chain pill: total measured boot time
/// across the chain's nodes plus the head unit name.
fn critical_chain_summary(evidence: &StartupBootEvidenceSnapshot) -> Option<String> {
    if evidence.critical_chain_failure.is_some() {
        return Some(i18n::t("startup.evidence_unavailable").to_string());
    }
    let measured: Vec<&taskmanager_core::core::startup::StartupCriticalChainNode> = evidence
        .critical_chain
        .iter()
        .filter(|node| node.duration_ms.is_some())
        .collect();
    if measured.is_empty() {
        return Some(formatting::missing_value());
    }
    let total_ms = measured.iter().fold(0_u64, |sum, node| {
        sum.saturating_add(node.duration_ms.unwrap_or(0))
    });
    let head = measured
        .first()
        .map(|node| node.unit.as_str())
        .unwrap_or("");
    Some(format!(
        "{total_ms} ms{}",
        if head.is_empty() {
            String::new()
        } else {
            format!(" · {head}")
        }
    ))
}

/// Non-interactive boot-evidence strip shown between the controls row and the
/// table. Renders nothing until a typed evidence snapshot arrives.
pub(super) fn boot_evidence_strip(
    theme: &Theme,
    evidence: Option<&StartupBootEvidenceSnapshot>,
) -> Div {
    let Some(evidence) = evidence else {
        return div();
    };
    if evidence.failed_units_failure.is_some() && evidence.critical_chain_failure.is_some() {
        return div();
    }
    let failed = failed_units_summary(evidence);
    let chain = critical_chain_summary(evidence);
    let failed_color = if evidence.failed_units.is_empty() {
        theme.fg_dim
    } else {
        theme.danger
    };
    let pill = |id: &'static str, label: &str, value: &str, value_color: Color| {
        div()
            .id(id)
            .debug_selector(move || id.to_string())
            .flex()
            .flex_row()
            .items_center()
            .gap(tokens::SPACE_3)
            .px(tokens::SPACE_3)
            .py(tokens::SPACE_2)
            .rounded(tokens::card_radius(theme))
            .border(px(1.0))
            .border_color(theme.border)
            .bg(theme.card_surface())
            .child(
                div()
                    .text_size(tokens::FONT_11)
                    .text_color(theme.fg_dim)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_size(tokens::FONT_11)
                    .text_color(value_color)
                    .child(value.to_string()),
            )
    };
    div()
        .flex()
        .flex_row()
        .gap(tokens::SPACE_4)
        .child(if let Some(failed) = failed {
            pill(
                "startup-evidence-failed",
                i18n::t("startup.failed_units"),
                &failed,
                failed_color,
            )
            .into_any_element()
        } else {
            div().into_any_element()
        })
        .child(if let Some(chain) = chain {
            pill(
                "startup-evidence-chain",
                i18n::t("startup.critical_chain"),
                &chain,
                theme.fg,
            )
            .into_any_element()
        } else {
            div().into_any_element()
        })
}

// ── boot timeline waterfall (BN-05) ──────────────────────────────────────────
//
// Extends the typed boot-evidence strip with a bounded waterfall over the
// measured systemd critical chain: per-unit windows on a normalized time
// axis. Nodes without activation data are counted and listed, never placed
// (honesty: no invented positions); the whole block stays silent until typed
// evidence arrives and stays silent on typed failure.

/// Minimum visible bar width so a 0-duration activation is still a mark.
const TIMELINE_MIN_BAR_PX: f32 = 2.0;

/// Pure value rows for the waterfall; `None` means the block renders nothing.
fn boot_timeline_rows(evidence: &StartupBootEvidenceSnapshot) -> Option<BootTimeline> {
    if evidence.critical_chain_failure.is_some() {
        return None;
    }
    let timeline = BootTimeline::from_critical_chain(
        &evidence.critical_chain,
        DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS,
        DEFAULT_BOOT_TIMELINE_MAX_UNTIMED,
    );
    if timeline.segments.is_empty() && timeline.untimed_count == 0 {
        return None;
    }
    Some(timeline)
}

/// Format a signed comparison delta for one unit (`+123 ms` / `-45 ms`).
/// Plain ASCII sign so frame-text tests and captures stay locale-neutral.
pub(super) fn format_delta_ms(delta_ms: i64) -> String {
    format!("{delta_ms:+} ms")
}

/// Non-interactive waterfall block between the evidence strip and the table.
/// `baseline` (the previous boot's waterfall, from the opt-in boot history —
/// roadmap #5) adds a fact-only delta chip per unit measured in BOTH boots;
/// without a baseline the block renders exactly as before.
pub(super) fn boot_timeline_block(
    theme: &Theme,
    evidence: Option<&StartupBootEvidenceSnapshot>,
    baseline: Option<&taskmanager_core::core::BootTimeline>,
    row_limit: usize,
) -> Option<Div> {
    let evidence = evidence?;
    let timeline = boot_timeline_rows(evidence)?;
    let deltas = baseline.map(|baseline| {
        taskmanager_core::core::segment_deltas(&timeline, baseline)
            .into_iter()
            .map(|delta| (delta.unit.clone(), delta))
            .collect::<std::collections::HashMap<String, taskmanager_core::core::BootSegmentDelta>>(
            )
    });
    let mut rows: Vec<Div> = timeline
        .segments
        .iter()
        .take(row_limit)
        .map(|segment| {
            let fraction = timeline.fraction_of_total(segment);
            div()
                .debug_selector(move || format!("timeline-{}", segment.unit))
                .flex()
                .flex_row()
                .items_center()
                .gap(tokens::SPACE_4)
                .child(
                    div()
                        .w(px(160.0))
                        .overflow_hidden()
                        .text_size(tokens::FONT_11)
                        .text_color(theme.fg)
                        .child(segment.unit.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(48.0))
                        .h(px(8.0))
                        .rounded(px(4.0))
                        .overflow_hidden()
                        .bg(theme.card_surface())
                        .child(
                            div()
                                .w(relative(fraction.clamp(0.0, 1.0)))
                                .min_w(px(TIMELINE_MIN_BAR_PX))
                                .h(px(8.0))
                                .rounded(px(4.0))
                                .bg(theme.accent)
                                .child(div()),
                        ),
                )
                .child(
                    div()
                        .w(px(64.0))
                        .text_size(tokens::FONT_11)
                        .text_color(theme.fg_dim)
                        .child(format!("{} ms", segment.duration_ms)),
                )
                .children(deltas.as_ref().and_then(|deltas| {
                    let delta = deltas.get(&segment.unit)?;
                    let color = if delta.delta_ms > 0 {
                        theme.danger
                    } else if delta.delta_ms < 0 {
                        theme.success
                    } else {
                        theme.fg_dim
                    };
                    Some(
                        div()
                            .w(px(72.0))
                            .text_size(tokens::FONT_11)
                            .text_color(color)
                            .debug_selector(move || format!("timeline-delta-{}", delta.unit))
                            .child(format_delta_ms(delta.delta_ms)),
                    )
                }))
        })
        .collect();
    if timeline.untimed_count > 0 {
        rows.push(
            div()
                .debug_selector(|| "timeline-untimed".to_string())
                .flex()
                .flex_row()
                .items_center()
                .gap(tokens::SPACE_4)
                .child(
                    div()
                        .w(px(160.0))
                        .text_size(tokens::FONT_11)
                        .text_color(theme.fg_dim)
                        .child(i18n::t("startup.timeline_untimed")),
                )
                .child(
                    div()
                        .text_size(tokens::FONT_11)
                        .text_color(theme.fg_dim)
                        .child(format!(
                            "{} · {}",
                            timeline.untimed_count,
                            timeline.untimed_units.join(" · ")
                        )),
                ),
        );
    }
    let presentation_collapsed = timeline.segments.len().saturating_sub(row_limit);
    let collapsed_count = timeline
        .collapsed_count
        .saturating_add(presentation_collapsed);
    if collapsed_count > 0 {
        rows.push(
            div()
                .debug_selector(|| "timeline-collapsed".to_string())
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(format!("+{collapsed_count}")),
        );
    }
    Some(
        div()
            .debug_selector(|| "boot-timeline".to_string())
            .flex()
            .flex_col()
            .gap(tokens::SPACE_4)
            .px(tokens::SPACE_4)
            .py(tokens::SPACE_3)
            .rounded(tokens::card_radius(theme))
            .border(px(1.0))
            .border_color(theme.border)
            .bg(theme.card_surface())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(tokens::SPACE_4)
                            .child(
                                div()
                                    .text_size(tokens::FONT_12)
                                    .font_weight(tokens::FONT_WEIGHT_SEMIBOLD.into())
                                    .text_color(theme.fg)
                                    .child(i18n::t("startup.timeline")),
                            )
                            .children((deltas.is_some()).then(|| {
                                div()
                                    .text_size(tokens::FONT_11)
                                    .text_color(theme.fg_dim)
                                    .debug_selector(|| "timeline-delta-legend".to_string())
                                    .child(i18n::t("startup.timeline_vs_previous"))
                            })),
                    )
                    .child(
                        div()
                            .text_size(tokens::FONT_11)
                            .text_color(theme.fg_dim)
                            .child(format!("{} ms", timeline.total_ms)),
                    ),
            )
            .children(rows),
    )
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_startup_view_boot_evidence_tests.rs"]
mod tests;
