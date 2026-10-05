//! Scene replacement finishes before focus decorators query mounted controls.
use bevy::ecs::schedule::SystemSet;
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ScenePaint;
