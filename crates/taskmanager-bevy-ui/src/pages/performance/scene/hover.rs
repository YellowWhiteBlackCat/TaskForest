//! The curve card's hover pill: the sampling INSTANT of the hovered sample
//! (BEVY-104).
//!
//! A hover pill that answers "which number is that?" with a sample OFFSET
//! (`-3`) answers a question nobody asked: the reader wants to know WHEN the
//! value was observed. The shared contract publishes that instant
//! ([`taskmanager_core::core::metrics::MetricSample::measured_at_ms`]) and
//! the shared formatter prints it
//! ([`taskmanager_shell::presentation::trend::sample_timestamp_text`]), so
//! this module reads the instant of the sample under the pointer and never
//! derives one of its own.
//!
//! A sample whose observation carried no measurement time has no instant to
//! show: the shared dash is the answer, never an offset and never the
//! acceptance time relabeled as the sampling time.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, Res};
use bevy::ui::ComputedNode;
use bevy::ui::widget::Text;
use taskmanager_core::core::metrics::MetricSample;
use taskmanager_core::core::time::{LocalTimeRules, LocalTimeRulesObservation};
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::missing_value;
use taskmanager_shell::presentation::trend::{sample_timestamp_text, timestamped_window};

use crate::app::ShellTrack;
use crate::pages::performance::SystemCurve;
use crate::widgets::chart::hover::{hover_position, hover_slot};
use crate::widgets::chart::painter::paint_chart_canvases;
use crate::widgets::chart::{ChartCanvas, ChartContent, ChartHoverLabel, ChartPointer};

/// The host's local-time rules: the only authority for rendering a sampling
/// instant in local time.
///
/// The rules are a HOST observation owned by the composition layer; this
/// frontend keeps a read-only copy so the hover pill can print the instant
/// an observation was measured at. The copy ships with the explicit fixed
/// UTC rule set — a real rule set, never an unavailable zone silently
/// reinterpreted as UTC — and the window layer replaces it with the host's
/// own observation when it publishes one.
#[derive(Resource, Clone, Debug)]
pub(crate) struct HostLocalTime(pub(crate) LocalTimeRulesObservation);

impl Default for HostLocalTime {
    fn default() -> Self {
        Self(LocalTimeRulesObservation::current(LocalTimeRules::utc(), 0))
    }
}

/// The curve a canvas draws: the marker that ties a canvas to the series it
/// projects, so the hover pill knows which window to read the instant from.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CurveSeries(pub(crate) SystemCurve);

/// The hover pill's readout: the hovered value and the INSTANT it was
/// measured at.
///
/// The instant comes from the sample's own measurement time through the
/// shared formatter. A sample with no measurement time has no instant: the
/// shared dash is printed, never a sample offset such as `-3`.
pub(crate) fn pill_text(
    format: Option<fn(f32) -> String>,
    value: f32,
    sample: Option<&MetricSample>,
    rules: &LocalTimeRulesObservation,
) -> String {
    let value = format.map_or_else(|| format!("{value:.0}"), |format| format(value));
    let instant = sample.map_or_else(missing_value, |sample| sample_timestamp_text(sample, rules));
    format!("{value} · {instant}")
}

/// The timestamped sample under the pointer, when the curve's window carries
/// one.
///
/// The timestamped window and the value window are two projections of the
/// SAME accepted samples, so the slot indexes both. A curve whose window
/// carries no instants (the NPU's shell-side history) has no instant to
/// show, and a window that no longer matches the drawn one proves nothing
/// about the slot: both fall back to the shared dash.
fn hovered_sample(
    shell: &ShellApp,
    curve: SystemCurve,
    points: usize,
    slot: usize,
) -> Option<MetricSample> {
    let samples = timestamped_window(&shell.history, curve.series());
    if samples.len() != points {
        return None;
    }
    samples.get(slot).copied()
}

/// Republish the hover pill's readout: the hovered value and the instant
/// that value was measured at.
///
/// The pill's own geometry and visibility stay with the chart painter; this
/// system owns the READOUT, so the offset spelling never reaches the
/// reader.
pub(crate) fn publish_hover_pill(
    track: ShellTrack,
    rules: Res<HostLocalTime>,
    canvases: Query<(
        Entity,
        &ChartCanvas,
        &ChartPointer,
        &ComputedNode,
        &CurveSeries,
    )>,
    labels: Query<(&ChartHoverLabel, &ChildOf, &Children)>,
    mut texts: Query<&mut Text>,
) {
    for (entity, canvas, pointer, node, series) in &canvases {
        let size = node.size() * node.inverse_scale_factor();
        let Some(position) = hover_position(pointer.normalized, size) else {
            continue;
        };
        let ChartContent::Samples { values, .. } = &canvas.content else {
            continue;
        };
        let Some(slot) = hover_slot(values.len(), size.x, position.x) else {
            continue;
        };
        let Some(value) = values.get(slot).copied().filter(|value| value.is_finite()) else {
            continue;
        };
        let sample = hovered_sample(track.shell(), series.0, values.len(), slot);
        let readout = pill_text(canvas.style.format, value, sample.as_ref(), &rules.0);
        for (_, parent, children) in &labels {
            if parent.parent() != entity {
                continue;
            }
            let Some(text_entity) = children.first() else {
                continue;
            };
            if let Ok(mut label) = texts.get_mut(*text_entity)
                && label.0 != readout
            {
                // The rewrite is equality-guarded: an unchanged readout is
                // not a change the text pipeline has to notice.
                label.0 = readout.clone();
            }
        }
    }
}

/// Register the hover pill's readout seam.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.init_resource::<HostLocalTime>();
    app.add_systems(
        bevy::app::PostUpdate,
        publish_hover_pill.after(paint_chart_canvases),
    );
}
