//! Identity-bound process inspection with fixed controls and one bounded body.

use crate::app::{FrontendTrack, ShellTrack};
use crate::drain::ShellProjectionFolded;
use crate::input::{PendingEffects, ShellInteractionApplied};
use crate::window::{AppShellRoot, WindowPalette};
use bevy::app::{App, PostUpdate, Startup};
use bevy::ecs::{
    component::Component,
    entity::Entity,
    event::Event,
    hierarchy::ChildOf,
    observer::On,
    query::With,
    resource::Resource,
    schedule::IntoScheduleConfigs,
    system::{Commands, NonSendMut, Query, ResMut},
    world::World,
};
use bevy::scene::WorldSceneExt;
use bevy::ui::{ScrollPosition, UiSystems};
use bevy::ui_widgets::Activate;
use taskmanager_application::{PlatformEffect, ProcessInsightFacet, i18n::t};
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessLiveKey};
use taskmanager_shell::ShellApp;

mod insights;
mod model;
mod performance;
mod view;
use model::view_model;
use performance::PerformanceCurve;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ProcessPropertiesSection {
    #[default]
    Overview,
    Performance,
    Command,
    Insights,
}
impl ProcessPropertiesSection {
    pub(crate) const ALL: [Self; 4] = [
        Self::Overview,
        Self::Performance,
        Self::Command,
        Self::Insights,
    ];
    pub(crate) fn label(self) -> &'static str {
        t(match self {
            Self::Overview => "prop.overview",
            Self::Performance => "prop.performance",
            Self::Command => "prop.command",
            Self::Insights => "prop.insights",
        })
    }
}
#[derive(Clone, Debug)]
pub(crate) struct ProcessPropertiesView {
    pub(crate) target: FrozenProcessIdentity,
    pub(crate) section: ProcessPropertiesSection,
    pub(crate) facet: ProcessInsightFacet,
    pub(crate) rows: Vec<(String, String)>,
    pub(crate) curves: Vec<PerformanceCurve>,
    pub(crate) authorize_network: bool,
    pub(crate) live: bool,
}
impl ProcessPropertiesView {
    fn same_rendered(&self, other: &Self) -> bool {
        self.target == other.target
            && self.section == other.section
            && self.facet == other.facet
            && self.rows == other.rows
            && self.authorize_network == other.authorize_network
            && self.live == other.live
            && self.curves.len() == other.curves.len()
            && self
                .curves
                .iter()
                .zip(&other.curves)
                .all(|(a, b)| a.same_rendered(b))
    }
}
#[derive(Resource)]
pub(crate) struct ProcessPropertiesPresentation {
    pub(crate) section: ProcessPropertiesSection,
    pub(crate) facet: ProcessInsightFacet,
    target_key: Option<ProcessLiveKey>,
}
impl Default for ProcessPropertiesPresentation {
    fn default() -> Self {
        Self {
            section: ProcessPropertiesSection::Overview,
            facet: ProcessInsightFacet::Network,
            target_key: None,
        }
    }
}
#[derive(Resource, Default)]
struct PropertiesPaint {
    dirty: bool,
    shown: Option<ProcessPropertiesView>,
}
#[derive(Event)]
pub(crate) struct ProcessPropertiesChanged;
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessPropertiesOverlay;
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessPropertiesBody;
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessPropertiesScrollHint;
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessPropertiesPanel;
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessPropertiesFooter;
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessPropertiesDismissButton;
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessPropertiesTab(pub(crate) ProcessPropertiesSection);
#[derive(Component, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ProcessPropertiesAction {
    #[default]
    Refresh,
    AuthorizeNetwork,
}
#[derive(Component, Clone)]
pub(crate) struct ProcessPropertiesFacet(pub(crate) ProcessInsightFacet);
impl Default for ProcessPropertiesFacet {
    fn default() -> Self {
        Self(ProcessInsightFacet::Network)
    }
}

pub(crate) fn republish(commands: &mut Commands) {
    commands.trigger(ProcessPropertiesChanged);
}
fn on_changed(_event: On<ProcessPropertiesChanged>, mut paint: ResMut<PropertiesPaint>) {
    paint.dirty = true;
}
fn on_fold(_event: On<ShellProjectionFolded>, track: ShellTrack, mut commands: Commands) {
    if track.shell().process_properties_target().is_some() {
        republish(&mut commands);
    }
}
fn on_applied(
    _event: On<ShellInteractionApplied>,
    track: ShellTrack,
    mut state: ResMut<ProcessPropertiesPresentation>,
    mut commands: Commands,
) {
    let key = track
        .shell()
        .process_properties_target()
        .and_then(FrozenProcessIdentity::live_key);
    if state.target_key != key {
        state.target_key = key;
        state.section = ProcessPropertiesSection::Overview;
        state.facet = ProcessInsightFacet::Network;
    }
    republish(&mut commands);
}
fn request_insights(track: &mut FrontendTrack, effects: &mut PendingEffects, force: bool) {
    let Some(target) = track.shell.process_properties_target() else {
        return;
    };
    if effects
        .0
        .iter()
        .any(|effect| matches!(effect, PlatformEffect::ProcessInsights(queued) if queued == target))
    {
        return;
    }
    if let Some(projection) = track
        .shell
        .projection()
        .process_insights
        .as_ref()
        .filter(|p| p.target == *target)
        && (!force || projection.is_collecting())
    {
        return;
    }
    if let Some(effect) = track.shell.request_properties_process_insights() {
        effects.0.push(effect);
    }
}
pub(super) fn on_tab(
    event: On<Activate>,
    tabs: Query<&ProcessPropertiesTab>,
    mut state: ResMut<ProcessPropertiesPresentation>,
    mut track: NonSendMut<FrontendTrack>,
    mut effects: ResMut<PendingEffects>,
    mut commands: Commands,
) {
    let Ok(tab) = tabs.get(event.event().entity) else {
        return;
    };
    state.section = tab.0;
    if tab.0 == ProcessPropertiesSection::Insights {
        request_insights(&mut track, &mut effects, false);
    }
    republish(&mut commands);
}
pub(super) fn on_facet(
    event: On<Activate>,
    facets: Query<&ProcessPropertiesFacet>,
    mut state: ResMut<ProcessPropertiesPresentation>,
    mut commands: Commands,
) {
    let Ok(facet) = facets.get(event.event().entity) else {
        return;
    };
    state.facet = facet.0;
    republish(&mut commands);
}
pub(super) fn on_refresh(
    _event: On<Activate>,
    mut track: NonSendMut<FrontendTrack>,
    mut effects: ResMut<PendingEffects>,
    mut commands: Commands,
) {
    request_insights(&mut track, &mut effects, true);
    republish(&mut commands);
}
pub(super) fn on_authorize(
    _event: On<Activate>,
    track: ShellTrack,
    mut effects: ResMut<PendingEffects>,
) {
    let Some(view) = view_model(
        track.shell(),
        ProcessPropertiesSection::Insights,
        ProcessInsightFacet::Network,
    ) else {
        return;
    };
    if view.authorize_network {
        effects
            .0
            .push(ShellApp::request_process_network_escalation());
    }
}
pub(crate) fn on_properties_dismiss_activated(
    _event: On<Activate>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    track.shell.dismiss_overlay();
    republish(&mut commands);
    commands.trigger(ShellInteractionApplied);
}
fn paint(world: &mut World) {
    if !world.resource::<PropertiesPaint>().dirty {
        return;
    }
    world.resource_mut::<PropertiesPaint>().dirty = false;
    let next = {
        let state = world.resource::<ProcessPropertiesPresentation>();
        let Some(track) = world.get_non_send::<FrontendTrack>() else {
            return;
        };
        view_model(&track.shell, state.section, state.facet)
    };
    let unchanged = match (&world.resource::<PropertiesPaint>().shown, &next) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_rendered(b),
        _ => false,
    };
    if unchanged {
        return;
    }
    let scroll = if world
        .resource::<PropertiesPaint>()
        .shown
        .as_ref()
        .zip(next.as_ref())
        .is_some_and(|(a, b)| a.target == b.target && a.section == b.section && a.facet == b.facet)
    {
        world
            .query_filtered::<&ScrollPosition, With<ProcessPropertiesBody>>()
            .iter(world)
            .next()
            .cloned()
            .unwrap_or_default()
    } else {
        ScrollPosition::default()
    };
    let overlays = world
        .query_filtered::<Entity, With<ProcessPropertiesOverlay>>()
        .iter(world)
        .collect::<Vec<_>>();
    for entity in overlays {
        let _ = world.despawn(entity);
    }
    world.resource_mut::<PropertiesPaint>().shown = next.clone();
    let Some(view) = next else { return };
    let Some(root) = world
        .query_filtered::<Entity, With<AppShellRoot>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let palette = world.resource::<WindowPalette>().inner.clone();
    if let Ok(overlay) = world.spawn_scene(view::properties_modal_scene(&view, &palette, scroll)) {
        let entity = overlay.id();
        world.entity_mut(root).add_one_related::<ChildOf>(entity);
    }
}
fn startup(
    track: ShellTrack,
    mut state: ResMut<ProcessPropertiesPresentation>,
    mut commands: Commands,
) {
    if track.shell().process_properties_target().is_some() {
        state.target_key = track
            .shell()
            .process_properties_target()
            .and_then(FrozenProcessIdentity::live_key);
        republish(&mut commands);
    }
}
pub(crate) fn register(app: &mut App) {
    app.init_resource::<ProcessPropertiesPresentation>()
        .init_resource::<PropertiesPaint>()
        .add_systems(Startup, startup)
        .add_observer(on_changed)
        .add_observer(on_fold)
        .add_observer(on_applied)
        .add_systems(
            PostUpdate,
            (paint, view::paint_curves).chain().after(UiSystems::Layout),
        );
}
#[cfg(test)]
#[path = "../../../tests/headless/pages/process_properties.rs"]
mod tests;

/// Capture follows the mounted normal tab/facet controls and the painted data.
pub(crate) fn capture_ready(world: &mut World, scenario: &str) -> bool {
    use crate::widgets::chart::CurveMeasurement;
    use bevy::ui::ComputedNode;
    use taskmanager_shell::fixture::process_insights::process_properties_capture_data_ready;
    use view::ProcessPropertiesCurve;
    if !world
        .get_non_send::<FrontendTrack>()
        .is_some_and(|track| process_properties_capture_data_ready(&track.shell, scenario))
    {
        return false;
    }
    let (section, facet) = match scenario {
        "process-properties-performance" => (
            ProcessPropertiesSection::Performance,
            ProcessInsightFacet::Network,
        ),
        "process-memory-pss-swap" => (
            ProcessPropertiesSection::Overview,
            ProcessInsightFacet::Network,
        ),
        "process-network-details" => (
            ProcessPropertiesSection::Insights,
            ProcessInsightFacet::Network,
        ),
        "process-gpu-details" => (ProcessPropertiesSection::Insights, ProcessInsightFacet::Gpu),
        "process-resource-limits" => (
            ProcessPropertiesSection::Insights,
            ProcessInsightFacet::Resources,
        ),
        "process-isolation" => (
            ProcessPropertiesSection::Insights,
            ProcessInsightFacet::Isolation,
        ),
        _ => return false,
    };
    if world.resource::<ProcessPropertiesPresentation>().section != section {
        let button = world
            .query::<(Entity, &ProcessPropertiesTab)>()
            .iter(world)
            .find(|(_, button)| button.0 == section)
            .map(|(entity, _)| entity);
        if let Some(entity) = button {
            world.trigger(Activate { entity });
        }
        return false;
    }
    if world.resource::<ProcessPropertiesPresentation>().facet != facet {
        let button = world
            .query::<(Entity, &ProcessPropertiesFacet)>()
            .iter(world)
            .find(|(_, button)| button.0 == facet)
            .map(|(entity, _)| entity);
        if let Some(entity) = button {
            world.trigger(Activate { entity });
        }
        return false;
    }
    if !world
        .resource::<PropertiesPaint>()
        .shown
        .as_ref()
        .is_some_and(|view| view.section == section && view.facet == facet && view.live)
    {
        return false;
    }
    if !world
        .query_filtered::<&ComputedNode, With<ProcessPropertiesBody>>()
        .iter(world)
        .any(|node| node.size().x > 0.0 && node.size().y > 0.0)
    {
        return false;
    }
    if section == ProcessPropertiesSection::Performance {
        let mut metrics = Vec::new();
        for (curve, measured, node) in world
            .query::<(&ProcessPropertiesCurve, &CurveMeasurement, &ComputedNode)>()
            .iter(world)
        {
            if measured.0.is_none()
                || node.size().x <= 0.0
                || node.size().y <= 0.0
                || curve
                    .samples
                    .iter()
                    .filter(|value| value.is_finite())
                    .count()
                    < 2
                || metrics.contains(&curve.metric)
            {
                return false;
            }
            metrics.push(curve.metric);
        }
        return metrics.len() == 4;
    }
    true
}
