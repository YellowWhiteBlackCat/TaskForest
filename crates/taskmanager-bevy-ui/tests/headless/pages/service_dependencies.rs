//! test-intent: behavior
//!
//! Service dependencies panel behavior over the shell's renderer-neutral lifecycle
//! (ADR-027). The bevy side is a pure consumer: these tests pin the seams:
//! - the open affordance resolves the SELECTED service and submits the request
//!   effect through the same PendingEffects queue the drain drains;
//! - the panel's dependencies fingerprint tracks lifecycle transitions;
//! - closing the panel closes the shell lifecycle;
//! - relation sections build without panic.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::widget::Text;
use taskmanager_application::PlatformEffect;
use taskmanager_application::ServiceDependenciesLifecycle;
use taskmanager_application::i18n::t;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::services::{
    ServiceDeps, ServiceItem, ServiceRelationKind, ServiceStatus,
};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::RequestId;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::control_error_detail;
use taskmanager_test_support::pin_english;
use taskmanager_theme::tokens::UiSize;

use super::dependencies_panel::{
    dependencies_fingerprint, service_dependencies_panel_scene_expanded,
};
use super::tests::{headless_services_app, push_services, route_to_services};
use crate::palette::UiPalette;
use bevy::scene::Scene;
use taskmanager_theme::Theme;

/// The panel scene with a default accordion state. The production painter
/// carries the reader's expansion state; the scene-level fixtures ask for the
/// default one.
fn service_dependencies_panel_scene(shell: &ShellApp, palette: &UiPalette) -> Box<dyn Scene> {
    service_dependencies_panel_scene_expanded(
        shell,
        palette,
        &super::deps_tree::ServiceDepsExpansion::default(),
    )
}

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

    let theme = Theme::default();
    let palette = crate::palette::ui_palette(&theme, UiSize::Standard);
    let scene = service_dependencies_panel_scene(&shell, &palette);

    // Spawn the production scene and read the painted text nodes: the panel
    // must really draw the resolved DAG's identity, its per-kind headers, and
    // the relation targets — not merely build without panic.
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    let world = app.world_mut();
    let root = world
        .spawn_scene(scene)
        .expect("the dependencies panel scene resolves without app resources")
        .id();
    let texts: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    assert!(
        world.despawn(root),
        "the dependencies scene despawns cleanly"
    );

    for expected in [
        format!("{} — demo.service", t("svc.dependencies")),
        format!("{} (1):", t("svc.requires")),
        "req.service".to_owned(),
        format!("{} (1):", t("svc.wants")),
        "wants.service".to_owned(),
    ] {
        assert!(
            texts.iter().any(|text| text == &expected),
            "the dependencies panel must paint {expected:?}: {texts:?}"
        );
    }
    // The two untargeted kinds stay honest dashes in the same panel; they
    // never borrow another kind's targets.
    for empty in [t("svc.wanted_by"), t("svc.after")] {
        let dashed = format!("{empty}: —");
        assert!(
            texts.iter().any(|text| text == &dashed),
            "an untargeted kind must paint its honest dash ({dashed:?}): {texts:?}"
        );
    }
    assert_eq!(
        texts.iter().filter(|text| *text == "req.service").count(),
        1
    );
}

#[test]
fn dependency_failure_renders_the_shared_localized_reason_in_the_production_scene() {
    pin_english();
    let failure = FailureKind::PermissionDenied;
    let target = ServiceId::new("restricted.service");
    let request = RequestId::new(1).expect("valid request id");
    let mut shell = ShellApp::new();
    shell.service_dependencies.begin(request, target.clone());
    assert!(shell.service_dependencies.fail(request, target, failure));

    let palette = crate::palette::ui_palette(&Theme::dark(), UiSize::Standard);
    let scene = service_dependencies_panel_scene(&shell, &palette);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app.world_mut()
        .spawn_scene(scene)
        .expect("the failed dependency panel resolves");
    app.update();

    let painted = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert!(painted.contains(&format!(
        "{}: {}",
        t("source.unavailable_title"),
        control_error_detail(failure)
    )));
    assert!(
        painted
            .iter()
            .all(|text| !text.contains("PermissionDenied")),
        "renderer output must not expose the Rust enum debug spelling"
    );
}
