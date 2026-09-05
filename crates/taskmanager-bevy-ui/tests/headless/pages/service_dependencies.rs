//! test-intent: behavior
//!
//! Service dependencies panel behavior over the shell's renderer-neutral lifecycle
//! (ADR-027). The bevy side is a pure consumer: these tests pin the seams:
//! - the open affordance resolves the SELECTED service and submits the request
//!   effect through the same PendingEffects queue the drain drains;
//! - the panel's dependencies fingerprint tracks lifecycle transitions;
//! - closing the panel closes the shell lifecycle;
//! - relation sections build without panic.

use taskmanager_application::PlatformEffect;
use taskmanager_application::ServiceDependenciesLifecycle;
use taskmanager_core::core::services::{
    ServiceDeps, ServiceItem, ServiceRelationKind, ServiceStatus,
};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::RequestId;
use taskmanager_shell::ShellApp;

use super::dependencies_panel::{dependencies_fingerprint, service_dependencies_panel_scene};
use super::tests::{headless_services_app, push_services, route_to_services};

fn service_item(id: &str, name: &str, status: ServiceStatus) -> ServiceItem {
    ServiceItem::from_inventory(
        id,
        name,
        status,
        format!("{name} description"),
        "loaded",
        "active",
        "running",
    )
}

#[test]
fn the_dependencies_open_affordance_targets_selected_service() {
    let (mut app, events) = headless_services_app();
    push_services(
        &events,
        vec![
            service_item("alpha.service", "alpha", ServiceStatus::Active),
            service_item("beta.service", "beta", ServiceStatus::Active),
        ],
    );
    route_to_services(&mut app);
    app.update();

    let target = ServiceId::new("beta.service");
    app.world_mut()
        .resource_mut::<crate::pages::services::ServiceSelection>()
        .target = Some(target.clone());

    app.world_mut()
        .commands()
        .trigger(crate::pages::services::dependencies_panel::ServiceDependenciesRequested);
    app.world_mut().flush();

    let submitted = app
        .world()
        .resource::<crate::input::PendingEffects>()
        .0
        .iter()
        .any(|effect| {
            if let PlatformEffect::ServiceDependencies(req) = effect {
                req.service_id == target
            } else {
                false
            }
        });
    assert!(
        submitted,
        "the dependencies open affordance submits the request effect for the selected service"
    );
}

#[test]
fn the_dependencies_fingerprint_tracks_lifecycle_transitions() {
    let closed = ServiceDependenciesLifecycle::Closed;
    let fp_closed = dependencies_fingerprint(&closed);
    assert_eq!(fp_closed.target, None);
    assert!(!fp_closed.is_loading);

    let target = ServiceId::new("demo.service");
    let mut loading = ServiceDependenciesLifecycle::Closed;
    let req_id = RequestId::new(1).expect("valid request id");
    loading.begin(req_id, target.clone());
    let fp_loading = dependencies_fingerprint(&loading);
    assert_eq!(fp_loading.target, Some(target.clone()));
    assert!(fp_loading.is_loading);

    let mut deps = ServiceDeps::default();
    deps.replace_relation_targets(
        ServiceRelationKind::Requires,
        [ServiceId::new("dep.service")],
    );

    let mut ready = ServiceDependenciesLifecycle::Closed;
    ready.begin(req_id, target.clone());
    let _ = ready.resolve(req_id, target.clone(), deps);
    let fp_ready = dependencies_fingerprint(&ready);
    assert_eq!(fp_ready.target, Some(target.clone()));
    assert!(!fp_ready.is_loading);
    assert!(fp_ready.has_deps);
}

#[test]
fn dependencies_panel_renders_relations_scene() {
    let mut shell = ShellApp::new();
    let target = ServiceId::new("demo.service");
    let mut deps = ServiceDeps::default();
    deps.replace_relation_targets(
        ServiceRelationKind::Requires,
        [ServiceId::new("req.service")],
    );
    deps.replace_relation_targets(
        ServiceRelationKind::Wants,
        [ServiceId::new("wants.service")],
    );

    let req_id = RequestId::new(1).expect("valid request id");
    shell.service_dependencies.begin(req_id, target.clone());
    shell.service_dependencies.resolve(req_id, target, deps);

    let theme = taskmanager_theme::Theme::default();
    let palette = crate::palette::ui_palette(&theme);
    let _scene = service_dependencies_panel_scene(&shell, &palette);
}
