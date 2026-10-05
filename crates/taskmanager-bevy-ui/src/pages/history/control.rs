//! History commands share one native session and canonical request controller.

use super::{ApplicationHistoryChanged, HistoryProjectionResource, HistoryRuntime};
use bevy::app::App;
use bevy::ecs::{
    event::Event,
    observer::On,
    resource::Resource,
    system::{Commands, NonSendMut, ResMut},
};
use taskmanager_application::PerformanceHistoryProjection;
use taskmanager_core::core::history::HistoryWindow;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Resource)]
pub(crate) enum PerformancePresentation {
    #[default]
    Live,
    Replay,
}
#[derive(Resource)]
pub(crate) struct PerformanceHistoryProjectionResource(pub(crate) PerformanceHistoryProjection);
impl Default for PerformanceHistoryProjectionResource {
    fn default() -> Self {
        Self(HistoryRuntime::default().performance_projection())
    }
}
#[derive(Event)]
pub(crate) struct PerformanceHistoryChanged;
#[derive(Event, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistoryCommand {
    OpenPerformance,
    ClosePerformance,
    Window(HistoryWindow),
    Refresh,
}

pub(crate) fn register(app: &mut App) {
    app.init_non_send::<HistoryRuntime>()
        .init_resource::<PerformancePresentation>()
        .init_resource::<PerformanceHistoryProjectionResource>()
        .add_observer(handle_command);
}
fn handle_command(
    command: On<HistoryCommand>,
    mut runtime: NonSendMut<HistoryRuntime>,
    mut history: ResMut<HistoryProjectionResource>,
    mut performance: ResMut<PerformanceHistoryProjectionResource>,
    mut presentation: ResMut<PerformancePresentation>,
    mut commands: Commands,
) {
    match *command.event() {
        HistoryCommand::OpenPerformance if runtime.available() => {
            *presentation = PerformancePresentation::Replay
        }
        HistoryCommand::OpenPerformance => return,
        HistoryCommand::ClosePerformance => *presentation = PerformancePresentation::Live,
        HistoryCommand::Window(window) => runtime.select_window(window),
        HistoryCommand::Refresh => runtime.refresh(),
    }
    history.0 = runtime.projection();
    performance.0 = runtime.performance_projection();
    commands.trigger(ApplicationHistoryChanged);
    commands.trigger(PerformanceHistoryChanged);
}
