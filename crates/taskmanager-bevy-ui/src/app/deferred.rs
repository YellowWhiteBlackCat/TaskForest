//! The deferred despawn of an outgoing page content.
//!
//! Bevy's official widget observers (the `Slider` writes its value, the
//! `Tabs` writes its selection) run in the frame their surface is rebuilt in.
//! Despawning the page content in that same frame hands those observers a
//! vanished entity, and their RAW command turns the race into a production
//! panic. The outgoing content is therefore HIDDEN at once and despawned at
//! the end of the frame, after every observer has run against a live child.

use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::Query;
use bevy::ecs::system::{Commands, ResMut};
use bevy::prelude::Visibility;

use super::PageContent;
use crate::scene_attach::insert_into;

/// The page contents whose despawn is deferred to the end of the frame.
#[derive(Resource, Default)]
pub(super) struct DeferredPageDespawn(pub(super) Vec<Entity>);

/// Hide every outgoing page content and defer its despawn to the end of the
/// frame. The hidden subtree leaves no double-mounted surface behind, and the
/// replacement content owns the frame.
pub(super) fn hide_outgoing_contents(
    content: &Query<Entity, With<PageContent>>,
    deferred: &mut DeferredPageDespawn,
    commands: &mut Commands,
) {
    for entity in content.iter() {
        insert_into(commands, entity, Visibility::Hidden);
        deferred.0.push(entity);
    }
}

/// Despawn the page contents hidden by the previous frame.
pub(super) fn flush_page_despawns(
    mut deferred: ResMut<DeferredPageDespawn>,
    mut commands: Commands,
) {
    for entity in deferred.0.drain(..) {
        // `try_despawn` never panics on an entity that is already gone.
        commands.entity(entity).try_despawn();
    }
}
