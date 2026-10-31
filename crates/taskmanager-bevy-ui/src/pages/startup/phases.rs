//! Startup boot phases: the three-phase proportion rows of the boot
//! overview card (BEVY-148).
//!
//! The three boot phases the overview compares — firmware, kernel and user
//! space — are read from the shared contract's MEASURED phase split
//! ([`taskforest_core::core::startup::StartupTimelineSnapshot`]). A phase
//! the contract did not measure stays absent: its row says so explicitly
//! instead of drawing a zero-width bar, which would claim the phase took no
//! time at all.
//!
//! The rows share ONE scale: every bar is its share of the measured total of
//! the compared phases, so the three read against each other. The contract
//! also publishes the boot loader's handoff span; this card compares the
//! three major phases, and no phase ever borrows another's time.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, Overflow,
    UiRect, Val, percent, rem,
};
use bevy::ui::widget::Text;
use taskforest_core::core::startup::{
    StartupBootEvidenceSnapshot, StartupPhaseKind, StartupTimelineSnapshot,
};
use taskmanager_application::i18n::t;

use super::inks::{InkRole, StartupInk};
use crate::palette::{UiPalette, space_2, space_8};
use crate::window::{Role, TextRole};

/// The boot phases the overview compares.
///
/// The `Default` seed only exists for the bsn! template mechanism; a marked
/// node always carries an explicit phase.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BootPhase {
    /// The firmware phase before the boot loader takes over.
    #[default]
    Firmware,
    /// The kernel phase before the init system takes over.
    Kernel,
    /// The user-space phase, from the init system up to the user session.
    Userspace,
}

/// One phase's row in the proportion comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PhaseSlice {
    /// The phase this row shows.
    pub(crate) phase: BootPhase,
    /// The measured duration, or `None` when the shared contract has no
    /// measurement for this phase. `None` renders as an unavailable row —
    /// never as a measured zero.
    pub(crate) duration_ms: Option<u64>,
}

/// Marker on one phase row: the phase it shows and whether it was measured.
///
/// The fields are positional for the scene's component syntax: the phase,
/// then the measured duration (`None` for an unmeasured phase).
///
/// The `Default` seed only exists for the bsn! template mechanism; a marked
/// node always carries the row's own facts.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupPhaseRow(pub(crate) BootPhase, pub(crate) Option<u64>);

/// Marker on one phase's bar: the measured share of the compared phases.
///
/// An unmeasured phase carries NO bar: the track stays empty instead of
/// holding a zero-width bar.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupPhaseBar(pub(crate) BootPhase);

/// The phase rows of the boot overview, or `None` when this boot published
/// no phase split at all (nothing to compare).
///
/// The phases come from the shared contract: each carries the duration the
/// platform measured, and a phase it did not measure stays absent.
pub(crate) fn boot_phase_slices(
    evidence: Option<&StartupBootEvidenceSnapshot>,
) -> Option<Vec<PhaseSlice>> {
    let timeline = evidence?.phase_timeline?;
    Some(compared_phases(&timeline))
}

/// The three compared phases, in boot order.
///
/// The contract publishes four phases; the loader's handoff span is not one
/// of the three this card compares, and its measured time is never borrowed
/// into a neighbouring phase.
fn compared_phases(timeline: &StartupTimelineSnapshot) -> Vec<PhaseSlice> {
    timeline
        .phases()
        .iter()
        .filter_map(|published| {
            let phase = match published.kind {
                StartupPhaseKind::Firmware => BootPhase::Firmware,
                StartupPhaseKind::Kernel => BootPhase::Kernel,
                StartupPhaseKind::UserSpace => BootPhase::Userspace,
                StartupPhaseKind::Loader => return None,
            };
            Some(PhaseSlice {
                phase,
                duration_ms: published.duration_ms,
            })
        })
        .collect()
}

/// The measured total of the compared phases: the scale the bars share.
///
/// Only the MEASURED phases contribute: an unmeasured phase is not a zero,
/// and a total of zero has no scale to project against.
pub(crate) fn phase_total_ms(slices: &[PhaseSlice]) -> u64 {
    slices.iter().fold(0_u64, |total, slice| {
        total.saturating_add(slice.duration_ms.unwrap_or(0))
    })
}

/// The proportion rows of the boot overview: one aligned row per phase over
/// the shared measured scale.
pub(crate) fn phase_rows_scene(slices: &[PhaseSlice], palette: &UiPalette) -> Vec<Box<dyn Scene>> {
    let total_ms = phase_total_ms(slices);
    slices
        .iter()
        .map(|slice| phase_row_scene(*slice, total_ms, palette))
        .collect()
}

/// One phase row: the phase name, its proportion track and its duration (or
/// the explicit unavailable marker).
///
/// An unmeasured phase gets an empty track and the "unmeasured" marker — no
/// bar at all, because a zero-width bar reads as "this phase took no time".
fn phase_row_scene(slice: PhaseSlice, total_ms: u64, palette: &UiPalette) -> Box<dyn Scene> {
    let label = phase_label(slice.phase).to_owned();
    let measured = slice.duration_ms.is_some();
    let share = match slice.duration_ms {
        Some(duration_ms) if total_ms > 0 => (duration_ms as f32 / total_ms as f32).clamp(0.0, 1.0),
        _ => 0.0,
    };
    let phase = slice.phase;
    let duration = slice.duration_ms;
    let value = match slice.duration_ms {
        Some(duration_ms) => format!("{duration_ms} ms"),
        None => t("startup.impact_unmeasured").to_owned(),
    };
    let value_ink = if measured { InkRole::Cpu } else { InkRole::Dim };
    let bar: Option<Box<dyn Scene>> = measured.then(|| {
        Box::new(bsn! {
            Node {
                width: percent(share * 100.0),
                height: percent(100),
                border_radius: BorderRadius::all(Val::Px(space_2())),
            }
            BackgroundColor({ palette.accent })
            StartupPhaseBar(phase)
        }) as Box<dyn Scene>
    });
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::vertical(Val::Px(space_2())),
        }
        StartupPhaseRow(phase, duration)
        Children [
             Node {
                 width: percent(28),
                 min_width: Val::Px(0.0),
                 overflow: Overflow::clip_x(),
             }
             Children [
                  Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
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
                  @{ bar }
             ]
            --
             Node {
                 width: percent(20),
                 min_width: Val::Px(0.0),
                 justify_content: JustifyContent::FlexEnd,
                 overflow: Overflow::clip_x(),
             }
             Children [
                  Text(value) TextRole(Role::Mono) StartupInk(value_ink)
                  TextLayout { linebreak: LineBreak::NoWrap }
             ]
        ]
    })
}

/// The catalog label of one phase. Every phase carries its own name: the
/// userspace row is the userspace phase, not the fact that measures it (the
/// systemd critical chain is that fact's own row).
fn phase_label(phase: BootPhase) -> &'static str {
    match phase {
        BootPhase::Firmware => t("system.firmware"),
        BootPhase::Kernel => t("system.kernel"),
        BootPhase::Userspace => t("startup.phase_userspace"),
    }
}
