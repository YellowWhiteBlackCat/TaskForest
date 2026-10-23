//! Contract-side gate for the semantic icon registry.

use std::collections::HashSet;

use super::*;

/// The semantic ids the icon owner chain introduced must be part of the
/// contract's single registry: a frontend registry that iterates
/// `IconId::ALL` can never miss one of them.
#[test]
fn the_owner_chain_ids_are_registered_in_the_contract_registry() {
    let added = [
        IconId::Terminal,
        IconId::Display,
        IconId::Docker,
        IconId::Podman,
        IconId::Lock,
    ];
    for id in added {
        assert!(
            IconId::ALL.contains(&id),
            "{id:?} must be listed in IconId::ALL"
        );
    }
}

/// `IconId::ALL` is the registry every frontend registry iterates: a
/// duplicated entry would make one semantic id count twice, and an id that
/// appears nowhere would never reach a frontend at all.
#[test]
fn the_registry_lists_every_declared_id_exactly_once() {
    let mut unique = HashSet::new();
    for id in IconId::ALL {
        assert!(unique.insert(id), "{id:?} is listed twice in IconId::ALL");
    }
    assert_eq!(
        unique.len(),
        IconId::ALL.len(),
        "the registry must list each semantic id once"
    );
}

/// The registry order is the declaration order: a frontend that renders the
/// ids in registry order must see the contract's own order, never a
/// hand-written variant list that drifts.
#[test]
fn the_registry_keeps_the_declaration_order() {
    let declaration = [
        IconId::Cpu,
        IconId::Memory,
        IconId::Disk,
        IconId::Network,
        IconId::Gpu,
        IconId::Process,
        IconId::Service,
        IconId::Startup,
        IconId::User,
        IconId::Health,
        IconId::Alert,
        IconId::Export,
        IconId::Settings,
        IconId::Search,
        IconId::More,
        IconId::NavigateUp,
        IconId::NavigateDown,
        IconId::Focus,
        IconId::Performance,
        IconId::Applications,
        IconId::Services,
        IconId::System,
        IconId::Users,
        IconId::Refresh,
        IconId::EndTask,
        IconId::Properties,
        IconId::Close,
        IconId::Pause,
        IconId::Sidebar,
        IconId::CircleCheck,
        IconId::CircleX,
        IconId::TriangleAlert,
        IconId::History,
        IconId::Terminal,
        IconId::Display,
        IconId::Docker,
        IconId::Podman,
        IconId::Lock,
        IconId::Remote,
    ];
    assert_eq!(IconId::ALL, declaration);
}
