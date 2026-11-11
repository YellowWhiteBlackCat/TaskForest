//! Boot-evidence rendering for the Startup page: a non-interactive stat-pill
//! strip (failed systemd units + critical-chain total) followed by a bounded
//! waterfall over the measured critical chain.
//!
//! Split out of [`super::startup_view`] (BN-05 / P1-ARCH-10). Pure render
//! functions and their unit tests; holds no UI state of its own — the typed
//! [`StartupBootEvidenceSnapshot`] arrives as a render param.

use gpui::{Div, InteractiveElement, IntoElement, ParentElement, Styled, div, px, relative};
use taskforest_core::core::BootSegmentDelta;
use taskforest_core::core::segment_deltas;
use taskmanager_ui::primitives::card_surface::{CardSurface, GlassSurfaceRole};
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::fill;
use taskmanager_ui::theme_binding::font_size;
use taskmanager_ui::theme_binding::font_weight;
use taskmanager_ui::theme_binding::hsla;
use taskmanager_ui::theme_binding::rem_length;

use crate::gpui_app::formatting;
use taskforest_core::core::startup::{
    BootTimeline, DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS, DEFAULT_BOOT_TIMELINE_MAX_UNTIMED,
    StartupBootEvidenceSnapshot,
};
use taskmanager_application::i18n;
use taskmanager_application::{critical_chain_summary, failed_units_summary};
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
    let chain = critical_chain_summary(evidence, &formatting::missing_value());
    let failed_color = if evidence.failed_units.is_empty() {
        theme.fg_dim
    } else {
        theme.danger
    };
    let pill = |id: &'static str, label: &str, value: &str, value_color: Color| {
        CardSurface::unpadded_for_role(theme.palette(), GlassSurfaceRole::InlinePlot)
            .radius(tokens::card_radius(theme))
            .render()
            .id(id)
            .debug_selector(move || id.to_string())
            .flex()
            .flex_row()
            .items_center()
            .gap(rem_length(tokens::SPACE_3))
            .px(rem_length(tokens::SPACE_3))
            .py(rem_length(tokens::SPACE_2))
            .child(
                div()
                    .text_size(font_size(tokens::FONT_11))
                    .text_color(hsla(theme.fg_dim))
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_size(font_size(tokens::FONT_11))
                    .text_color(hsla(value_color))
                    .child(value.to_string()),
            )
    };
    div()
        .flex()
        .flex_row()
        .gap(definite_length(tokens::SPACE_4))
        .child(
            pill(
                "startup-evidence-failed",
                i18n::t("startup.failed_units"),
                &failed,
                failed_color,
            )
            .into_any_element(),
        )
        .child(
            pill(
                "startup-evidence-chain",
                i18n::t("startup.critical_chain"),
                &chain,
                theme.fg,
            )
            .into_any_element(),
        )
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

/// The inline waterfall bar track: the shared Card-tier inset surface paints
/// the same glass material without a second floating elevation. A filled bar
/// overlays it, so the track itself owns no layout or progress geometry.
fn inline_plot_surface(theme: &Theme) -> Div {
    CardSurface::unpadded_for_role(theme.palette(), GlassSurfaceRole::InlinePlot)
        .radius(tokens::control_radius(theme))
        .bordered(false)
        .render()
        .absolute()
        .inset_0()
}

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
    baseline: Option<&BootTimeline>,
    row_limit: usize,
) -> Option<Div> {
    let evidence = evidence?;
    let timeline = boot_timeline_rows(evidence)?;
    let deltas = baseline.map(|baseline| {
        segment_deltas(&timeline, baseline)
            .into_iter()
            .map(|delta| (delta.unit.clone(), delta))
            .collect::<std::collections::HashMap<String, BootSegmentDelta>>()
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
                .gap(definite_length(tokens::SPACE_4))
                .child(
                    div()
                        .w(px(160.0))
                        .overflow_hidden()
                        .text_size(font_size(tokens::FONT_11))
                        .text_color(hsla(theme.fg))
                        .child(segment.unit.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(48.0))
                        .h(px(8.0))
                        .rounded_full()
                        .overflow_hidden()
                        .relative()
                        .child(inline_plot_surface(theme))
                        .child(
                            div()
                                .w(relative(fraction.clamp(0.0, 1.0)))
                                .min_w(px(TIMELINE_MIN_BAR_PX))
                                .h(px(8.0))
                                .rounded_full()
                                .bg(fill(theme.accent))
                                .child(div()),
                        ),
                )
                .child(
                    div()
                        .w(px(64.0))
                        .text_size(font_size(tokens::FONT_11))
                        .text_color(hsla(theme.fg_dim))
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
                            .text_size(font_size(tokens::FONT_11))
                            .text_color(hsla(color))
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
                .gap(definite_length(tokens::SPACE_4))
                .child(
                    div()
                        .w(px(160.0))
                        .text_size(font_size(tokens::FONT_11))
                        .text_color(hsla(theme.fg_dim))
                        .child(i18n::t("startup.timeline_untimed")),
                )
                .child(
                    div()
                        .text_size(font_size(tokens::FONT_11))
                        .text_color(hsla(theme.fg_dim))
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
                .text_size(font_size(tokens::FONT_11))
                .text_color(hsla(theme.fg_dim))
                .child(format!("+{collapsed_count}")),
        );
    }
    Some(
        CardSurface::unpadded_for_role(theme.palette(), GlassSurfaceRole::InlinePlot)
            .radius(tokens::card_radius(theme))
            .render()
            .debug_selector(|| "boot-timeline".to_string())
            .flex()
            .flex_col()
            .gap(rem_length(tokens::SPACE_4))
            .px(rem_length(tokens::SPACE_4))
            .py(rem_length(tokens::SPACE_3))
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
                            .gap(definite_length(tokens::SPACE_4))
                            .child(
                                div()
                                    .text_size(font_size(tokens::FONT_12))
                                    .font_weight(font_weight(tokens::FONT_WEIGHT_SEMIBOLD))
                                    .text_color(hsla(theme.fg))
                                    .child(i18n::t("startup.timeline")),
                            )
                            .children((deltas.is_some()).then(|| {
                                div()
                                    .text_size(font_size(tokens::FONT_11))
                                    .text_color(hsla(theme.fg_dim))
                                    .debug_selector(|| "timeline-delta-legend".to_string())
                                    .child(i18n::t("startup.timeline_vs_previous"))
                            })),
                    )
                    .child(
                        div()
                            .text_size(font_size(tokens::FONT_11))
                            .text_color(hsla(theme.fg_dim))
                            .child(format!("{} ms", timeline.total_ms)),
                    ),
            )
            .children(rows),
    )
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_startup_view_boot_evidence_tests.rs"]
mod tests;
