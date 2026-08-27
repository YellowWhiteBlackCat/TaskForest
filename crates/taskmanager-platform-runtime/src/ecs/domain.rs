//! Data-driven diagnostics for the runtime's typed capability domains.

use bevy_app::prelude::{App, Update};
use bevy_ecs::prelude::{Res, ResMut, Resource};
use bevy_ecs::schedule::IntoScheduleConfigs;

use super::{DueWork, EcsScheduleSet, RuntimeEcsPlugin};
use crate::config::RuntimeDomain;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct DomainDiagnostics {
    passes: u64,
    planned_items: [u64; RuntimeDomain::COUNT],
}

impl DomainDiagnostics {
    pub(super) fn planned_items(self, domain: RuntimeDomain) -> u64 {
        self.planned_items[domain.index()]
    }
}

pub(super) struct DomainDiagnosticsPlugin;

impl RuntimeEcsPlugin for DomainDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .insert_resource(DomainDiagnostics::default());
        app.add_systems(Update, record_domain_plan.in_set(EcsScheduleSet::Domain));
    }
}

fn record_domain_plan(mut diagnostics: ResMut<DomainDiagnostics>, due_work: Res<DueWork>) {
    diagnostics.passes = diagnostics.passes.saturating_add(1);
    for item in &due_work.items {
        let planned = &mut diagnostics.planned_items[item.domain.index()];
        *planned = planned.saturating_add(1);
    }
}
