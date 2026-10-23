//! The NPU card's power section: the accelerator's power curve beside the
//! compute curve (BEVY-162).
//!
//! The shared contract publishes the accelerator's power draw and its
//! cumulative energy as ONE typed fact
//! ([`taskmanager_core::core::npu::NpuPowerReport`]). A platform that cannot
//! read the capability publishes no fact at all, and this section then does
//! not exist: the NPU card keeps its compute curve alone, never a flat zero
//! line standing in for the energy series and never a value derived from the
//! utilization curve.
//!
//! The curve's samples come from the SHELL's bounded power history — the
//! frontend owns no buffer. An empty history means an empty curve.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, ResMut};
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{
    AlignItems, Display, FlexDirection, JustifyContent, Node, Overflow, PositionType, Val, percent,
    px, rem,
};
use bevy::ui::widget::Text;
use taskmanager_application::i18n::t;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::power_w;

use crate::app::ShellTrack;
use crate::drain::ShellProjectionFolded;
use crate::pages::performance::metrics::{npu_power_fact, npu_power_samples};
use crate::palette::{UiPalette, space_2};
use crate::widgets::chart::canvas::{CANVAS_FILL_ALPHA, CanvasFill};
use crate::widgets::chart::geometry::CHART_LINE_THICKNESS_PX;
use crate::widgets::chart::{ChartCanvas, ChartContent, ChartPaintState, ChartPointer, ChartStyle};
use crate::window::{Role, TextRole};

/// The power strip's track height, relative to the control height.
///
/// The power curve is the NPU card's SECOND series, so it rides a shorter
/// track than the headline compute curve: the card's footprint stays close
/// to the other curve cards, and the section is admitted as one whole group.
const POWER_STRIP_HEIGHT_FACTOR: f32 = 2.0;

/// Marker on the NPU card's power section: the whole group, admitted or
/// hidden as one.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct NpuPowerSection;

/// Marker on the NPU card's power canvas.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct NpuPowerStrip;

/// The refresh flag: a folded projection republishes the power series.
#[derive(Resource, Default)]
pub(crate) struct NpuPowerRefresh(bool);

/// The power series' own scale: the measured peak, never a fabricated zero.
///
/// An empty series has no scale to project against, so the ceiling stays at
/// its floor — a full-width bar would claim a draw the host never reported.
pub(crate) fn power_ceiling(samples: &[f32]) -> f32 {
    samples
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(1.0_f32, f32::max)
        * 1.1
}

/// The NPU card's power section, or `None` when no accelerator reports the
/// power capability.
///
/// The section carries the power curve (its own scale, beside the compute
/// curve) and the measured readout: the current draw in watts and the
/// consumed energy in joules.
pub(crate) fn section(shell: &ShellApp, palette: &UiPalette) -> Option<Box<dyn Scene>> {
    let fact = npu_power_fact(shell)?;
    let samples = npu_power_samples(shell);
    let ceiling = power_ceiling(&samples);
    let content = ChartContent::Samples {
        values: samples,
        ceiling,
    };
    let style = ChartStyle {
        line: palette.warning_color,
        thickness: CHART_LINE_THICKNESS_PX,
        fill: Some(CanvasFill {
            color: palette.warning_color,
            alpha: CANVAS_FILL_ALPHA,
        }),
        grid: None,
        flow: None,
        format: Some(power_w),
    };
    let height_rem = palette.control_height_rem * POWER_STRIP_HEIGHT_FACTOR;
    let labels = power_ruler_labels(ceiling);
    let readout = format!("{} · {} J", power_w(fact.watts), fact.energy_j);
    Some(Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        NpuPowerSection
        Children [
             Text({ t("common.power").to_owned() }) TextRole(Role::Caption) --
             Node {
                 width: percent(100),
                 height: rem(height_rem),
                 flex_direction: FlexDirection::Row,
                 column_gap: Val::Px(space_2()),
                 position_type: PositionType::Relative,
                 overflow: Overflow::clip_x(),
             }
             Children [
                  Node {
                      flex_grow: 1.0,
                      min_width: px(0.0),
                      height: percent(100),
                      position_type: PositionType::Relative,
                      overflow: Overflow::clip_x(),
                  }
                  Children [
                       Node {
                           width: percent(100),
                           height: percent(100),
                           position_type: PositionType::Absolute,
                           left: px(0.0),
                           top: px(0.0),
                           overflow: Overflow::clip(),
                       }
                       ChartCanvas { content: {content}, style: {style} }
                       ChartPointer
                       ChartPaintState
                       NpuPowerStrip
                  ] --
                  @{ ruler_scene(labels, palette) }
             ] --
             Text(readout) TextRole(Role::Mono)
        ]
    }))
}

/// The power scale's labels: the watts the curve's own scale marks.
pub(crate) fn power_ruler_labels(ceiling: f32) -> Vec<String> {
    [1.0_f32, 0.75, 0.5, 0.25, 0.0]
        .iter()
        .map(|fraction| power_w(ceiling * fraction))
        .collect()
}

/// The scale ruler beside the strip: the watts the power curve marks.
fn ruler_scene(labels: Vec<String>, palette: &UiPalette) -> Box<dyn Scene> {
    let cells: Vec<Box<dyn Scene>> = labels
        .into_iter()
        .map(|label| {
            Box::new(bsn! {
                Text(label)
                TextRole(Role::Caption)
                bevy::text::TextLayout { linebreak: bevy::text::LineBreak::NoWrap }
            }) as Box<dyn Scene>
        })
        .collect();
    Box::new(bsn! {
        Node {
            width: rem(palette.control_height_rem * 3.0),
            height: percent(100),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::FlexEnd,
        }
        Children [ { cells } ]
    })
}

/// The folded projection republished the power series.
fn request_refresh(_fold: On<ShellProjectionFolded>, mut refresh: ResMut<NpuPowerRefresh>) {
    refresh.0 = true;
}

/// Republish the power series into the mounted canvas and admit or hide the
/// section as one whole group.
///
/// The section exists only while the accelerator reports the power fact: a
/// capability that disappears leaves the card with its compute curve alone.
pub(crate) fn paint_npu_power(
    track: ShellTrack,
    mut refresh: ResMut<NpuPowerRefresh>,
    mut canvases: Query<&mut ChartCanvas, With<NpuPowerStrip>>,
    mut sections: Query<&mut Node, With<NpuPowerSection>>,
) {
    if !refresh.0 {
        return;
    }
    let samples = npu_power_samples(track.shell());
    let ceiling = power_ceiling(&samples);
    let present = npu_power_fact(track.shell()).is_some();
    for mut canvas in &mut canvases {
        canvas.content = ChartContent::Samples {
            values: samples.clone(),
            ceiling,
        };
    }
    for mut node in &mut sections {
        node.display = if present {
            Display::Flex
        } else {
            Display::None
        };
    }
    refresh.0 = false;
}

/// Register the power section's refresh seam.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.init_resource::<NpuPowerRefresh>();
    app.add_observer(request_refresh);
    app.add_systems(
        bevy::app::PostUpdate,
        paint_npu_power.after(bevy::ui::UiSystems::PostLayout),
    );
}
