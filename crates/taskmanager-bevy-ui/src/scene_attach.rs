//! Attach a freshly spawned child under its parent, without a despawn race.
//!
//! `EntityCommands::add_one_related` queues a RAW command: when the shell
//! root is despawned between the query and the command's application (a route
//! change replaces the scene), the fallback error handler turns that race into
//! a production panic. The AGENTS charter forbids a panic in production code,
//! so the relation is applied through a SILENCED queue instead: a vanished
//! root simply means the child goes with it.

use bevy::ecs::bundle::Bundle;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::system::Commands;
use bevy::ecs::world::EntityWorldMut;

/// Attach `child` under `parent`.
///
/// A parent that no longer exists is a vanished scene, not an error: the child
/// goes with it. The relation is applied through a SILENCED queue, so the
/// despawn race between the query and the command's application can never
/// reach the fallback error handler (and panic).
pub(crate) fn attach_child(commands: &mut Commands, parent: Entity, child: Entity) {
    commands
        .entity(parent)
        .queue_silenced(move |mut entity: EntityWorldMut| {
            entity.add_related::<ChildOf>(&[child]);
        });
}

/// Insert a bundle into an entity, tolerating an entity that no longer exists.
///
/// `EntityCommands::insert` queues a RAW command as well: a modal that
/// rebuilds its own surface despawns the old tab entity in the same frame the
/// selection is published, and the insert then hits a vanished entity.
pub(crate) fn insert_into(commands: &mut Commands, entity: Entity, bundle: impl Bundle) {
    commands
        .entity(entity)
        .queue_silenced(move |mut entity: EntityWorldMut| {
            entity.insert(bundle);
        });
}
