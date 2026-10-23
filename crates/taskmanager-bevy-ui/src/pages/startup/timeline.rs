//! Startup boot timeline: the systemd critical-chain gantt (BEVY-146).
//!
//! The shared projection is [`taskmanager_application::boot_timeline_rows`]
//! — the one authority every frontend's waterfall reads: a typed
//! critical-chain failure keeps the block silent, an empty chain keeps it
//! silent, and a measured chain becomes a bounded [`BootTimeline`] whose
//! segments carry activation offsets and durations.
//!
//! The chart is a real gantt, not a duration-only bar list: each unit's
//! window is PLACED at its activation offset (the bar's left margin is the
//! unit's share of the measured span) and SIZED by its duration share, so
//! the boot's ordering and its overlap stay readable. Nodes the provider
//! could not place are counted and listed, never positioned; overflow past
//! the segment cap is counted, never silently dropped.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, Overflow,
    UiRect, Val, percent, rem,
};
use bevy::ui::widget::Text;
use taskmanager_application::boot_timeline_rows;
use taskmanager_application::i18n::t;
use taskmanager_core::core::startup::StartupBootEvidenceSnapshot;

use super::phases::{boot_phase_slices, phase_rows_scene, phase_total_ms};
use crate::palette::{UiPalette, space_2, space_4, space_8};
use crate::window::{Role, TextRole};

/// One measured unit window on the gantt axis.
///
/// The fields are positional for the scene's component syntax: the unit,
/// then the activation offset from boot, then the window's own duration.
///
/// The `Default` seed only exists for the bsn! template mechanism; a marked
/// node always carries the window's own facts.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupTimelineWindow(pub(crate) String, pub(crate) u64, pub(crate) u64);

/// The gantt's measured span — the axis total every window is scaled against.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupTimelineTotal(pub(crate) u64);

/// The chain nodes the provider could not place on the axis.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupTimelineUntimed(pub(crate) usize);

/// The chain nodes the segment cap collapsed into a count.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupTimelineCollapsed(pub(crate) usize);

/// The boot overview's own row budget: how many measured windows the card
/// shows. The shared segment cap bounds the DATA (20 windows); this bounds
/// the CARD, so a pathological chain cannot push the inventory table out of
/// the viewport. The overflow is counted in the same "+N" row the shared
/// cap uses — never silently dropped.
pub(crate) const OVERVIEW_WINDOW_ROWS: usize = 8;

/// The gantt scene: a titled card with one placed window per measured unit.
///
/// `None` when neither boot source published anything: the critical chain is
/// unavailable or empty AND the platform published no phase split. The two
/// sources are independent, so a boot that published one of them still
/// shows its overview.
pub(crate) fn boot_timeline_scene(
    evidence: Option<&StartupBootEvidenceSnapshot>,
    palette: &UiPalette,
) -> Option<Box<dyn Scene>> {
    let evidence = evidence?;
    let timeline = boot_timeline_rows(evidence);
    let slices = boot_phase_slices(Some(evidence));
    if timeline.is_none() && slices.is_none() {
        return None;
    }
    // The card's own scale: the measured boot span of the chain, or — when
    // only the phase split was published — the measured total of the
    // compared phases.
    let total_ms = timeline
        .as_ref()
        .map(|timeline| timeline.total_ms)
        .or_else(|| slices.as_ref().map(|slices| phase_total_ms(slices)))
        .unwrap_or_default();
    // The card's own row budget: the windows past it are counted, never
    // dropped, and the measured axis stays the boot's real span.
    let shown = timeline.as_ref().map_or(0, |timeline| {
        timeline.segments.len().min(OVERVIEW_WINDOW_ROWS)
    });
    let rows: Vec<Box<dyn Scene>> = timeline
        .as_ref()
        .map(|timeline| {
            timeline
                .segments
                .iter()
                .take(shown)
                .map(|segment| {
                    let start_share = share_of_total(segment.start_ms, total_ms);
                    let duration_share = share_of_total(segment.duration_ms, total_ms);
                    let unit = segment.unit.clone();
                    let duration_text = format!("{} ms", segment.duration_ms);
                    let window_unit = unit.clone();
                    let window_start = segment.start_ms;
                    let window_duration = segment.duration_ms;
                    Box::new(bsn! {
                        Node {
                            width: percent(100),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(space_8()),
                            padding: UiRect::vertical(Val::Px(space_2())),
                        }
                        Children [
                             Node {
                                 width: percent(28),
                                 min_width: Val::Px(0.0),
                                 overflow: Overflow::clip_x(),
                             }
                             Children [
                                  Text(unit) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
                             ]
                            --
                             Node {
                                 width: percent(48),
                                 min_width: Val::Px(0.0),
                                 height: rem(palette.control_height_rem * 0.45),
                                 border_radius: BorderRadius::all(Val::Px(space_2())),
                                 overflow: Overflow::clip_x(),
                             }
                             BackgroundColor({ palette.panel_fill })
                             Children [
                                  Node {
                                      width: percent(duration_share * 100.0),
                                      min_width: rem(palette.control_height_rem * 0.08),
                                      height: percent(100),
                                      margin: UiRect::left(percent(start_share * 100.0)),
                                      border_radius: BorderRadius::all(Val::Px(space_2())),
                                  }
                                  BackgroundColor({ palette.accent })
                                  StartupTimelineWindow(window_unit, window_start, window_duration)
                             ]
                            --
                             Node {
                                 width: percent(20),
                                 min_width: Val::Px(0.0),
                                 justify_content: JustifyContent::FlexEnd,
                                 overflow: Overflow::clip_x(),
                             }
                             Children [
                                  Text(duration_text) TextRole(Role::Mono) TextLayout { linebreak: LineBreak::NoWrap }
                             ]
                        ]
                    }) as Box<dyn Scene>
                })
                .collect()
        })
        .unwrap_or_default();

    let mut children: Vec<Box<dyn Scene>> = vec![Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        Children [
             Text({ t("startup.timeline").to_owned() }) TextRole(Role::Caption)
             TextLayout { linebreak: LineBreak::NoWrap }
            --
             Node { flex_grow: 1.0 } --
             Text({ format!("{total_ms} ms") }) TextRole(Role::Mono)
             TextLayout { linebreak: LineBreak::NoWrap }
             StartupTimelineTotal({ total_ms })
        ]
    }) as Box<dyn Scene>];
    // The boot-phase proportion rows sit on the same measured scale as the
    // unit windows below them: firmware, kernel and userspace compared
    // against the boot they belong to.
    if let Some(slices) = &slices {
        children.extend(phase_rows_scene(slices, palette));
    }
    children.extend(rows);
    if let Some(timeline) = &timeline {
        if timeline.untimed_count > 0 {
            let untimed = format!(
                "{} · {}",
                timeline.untimed_count,
                timeline.untimed_units.join(" · ")
            );
            children.push(Box::new(bsn! {
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_8()),
                }
                Children [
                     Text({ t("startup.timeline_untimed").to_owned() }) TextRole(Role::Caption)
                     TextLayout { linebreak: LineBreak::NoWrap }
                    --
                     Text(untimed) TextRole(Role::Caption)
                     TextLayout { linebreak: LineBreak::NoWrap }
                     StartupTimelineUntimed({ timeline.untimed_count })
                ]
            }) as Box<dyn Scene>);
        }
        // The shared cap's overflow plus the card's own budget collapse into
        // one counted row: the reader learns how many windows the boot had.
        let collapsed = timeline
            .collapsed_count
            .saturating_add(timeline.segments.len().saturating_sub(shown));
        if collapsed > 0 {
            children.push(Box::new(bsn! {
                Node { width: percent(100) }
                Children [
                     Text({ format!("+{collapsed}") }) TextRole(Role::Caption)
                     StartupTimelineCollapsed({ collapsed })
                ]
            }) as Box<dyn Scene>);
        }
    }

    Some(Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.content_bg })
        Children [
            { children }
        ]
    }))
}

/// One measured quantity's share of the axis total, as a `0.0..=1.0` ratio.
///
/// A zero total yields `0.0` for every window: there is nothing to scale by,
/// and a full-width bar would be a fabricated measurement.
fn share_of_total(value_ms: u64, total_ms: u64) -> f32 {
    if total_ms == 0 {
        return 0.0;
    }
    (value_ms as f32 / total_ms as f32).clamp(0.0, 1.0)
}
