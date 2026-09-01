//! test-intent: behavior
//!
//! Headless routing tests for the frontend-owned page model.
//!
//! Two layers:
//! - pure: the chord→page decision must stay chord-for-chord aligned with
//!   the shared command router (the TUI/GPUI page-switching semantics), the
//!   frontend-local Settings binding must be unmodified-only, and shared
//!   pages with no bevy surface must resolve to nothing — a mutation in the
//!   key normalization, the action mapping, or the route transition fails
//!   these without a world;
//! - wired: on a `MinimalPlugins` app with the real window plugin, a scripted
//!   keypress must move the route, remount exactly one page content under
//!   the content slot, and restyle the nav rail through the observer — no
//!   compositor involved.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::With;
use bevy::input::keyboard::KeyCode;
use bevy::ui_widgets::Activate;
use taskmanager_application::{
    HostTelemetryRequest, PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle,
    SystemFacets,
};
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    EventEnvelope, EventPort, EventPortError, RequestEnvelope, RequestPort, SubmissionError,
};

use taskmanager_theme::Theme;

use super::{
    ContentSlot, ModifierState, NavTarget, Page, PageContent, Route, RouteChanged,
    nav_item_background, page_for_action, route_key_press,
};
use crate::input_contract::SemanticAddress;
use crate::pages::process_tree::ProcessTreeSurface;
use crate::palette::ui_palette;
use crate::runtime::{RuntimeCache, SharedRuntime};
use crate::window::FrontendWindowPlugin;
use crate::window::SummaryLine;
use crate::window::tests::HeadlessFrontendPlugins;

// ---- scripted platform client (same shape as the window test) ----

struct FixedCapabilities(CapabilitySnapshot);

impl CapabilityCatalog for FixedCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        self.0.clone()
    }
}

struct QuietEvents;

impl EventPort for QuietEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}

struct QuietRequests;

impl RequestPort for QuietRequests {
    type Request = HostTelemetryRequest;

    fn try_submit(&self, _request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        Ok(())
    }
}

fn descriptor(id: CapabilityId, status: CapabilityStatus) -> CapabilityDescriptor {
    CapabilityDescriptor {
        id,
        status,
        providers: Vec::new(),
        observed_at_ms: 1,
        last_success_at_ms: None,
    }
}

fn scripted_runtime() -> &'static SharedRuntime {
    let snapshot = CapabilitySnapshot::from_descriptors([descriptor(
        CapabilityId::TELEMETRY_HOST,
        CapabilityStatus::Available,
    )]);
    let client = PlatformClient::new(PlatformHandle::new(
        std::sync::Arc::new(FixedCapabilities(snapshot)),
        std::sync::Arc::new(QuietEvents),
        PlatformFacets::default()
            .with_system(SystemFacets::default().with_host(std::sync::Arc::new(QuietRequests))),
    ));
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    cache
        .get_or_init(move || Ok(client))
        .expect("the scripted runtime always starts")
}

/// `MinimalPlugins` + the real window plugin + the real route/nav/mount
/// systems, with the font store `AssetPlugin` does not auto-create.
fn headless_shell_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(HeadlessFrontendPlugins);
    app.add_plugins(FrontendWindowPlugin {
        runtime: scripted_runtime(),
        palette: ui_palette(&Theme::dark()),
    });
    app.init_resource::<bevy::asset::Assets<bevy::text::Font>>();
    app
}

use bevy::ecs::entity::Entity;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};

/// Inject key presses the way a real window does: as `KeyboardInput`
/// messages, which the input plugin's `PreUpdate` system folds into
/// `ButtonInput` before the `Update` routing system reads them. Pressing
/// `ButtonInput` directly is wiped by that same `PreUpdate` fold.
fn press(app: &mut App, keys: &[KeyCode]) {
    for &key in keys {
        let input = KeyboardInput {
            key_code: key,
            logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        };
        app.world_mut().write_message(input);
    }
}

fn alt(page_digit: KeyCode) -> Vec<KeyCode> {
    vec![KeyCode::AltLeft, page_digit]
}

// ---- pure routing semantics ----

#[test]
fn shared_page_chords_route_chord_for_chord_like_the_tui() {
    // The shared router owns Alt+1..8; the bevy mapping must resolve the
    // same pages those chords open in the TUI/GPUI frontends.
    let alt = ModifierState {
        alt: true,
        ..ModifierState::default()
    };
    assert_eq!(
        route_key_press(KeyCode::Digit1, alt),
        Some(Page::Performance)
    );
    assert_eq!(route_key_press(KeyCode::Digit2, alt), Some(Page::Processes));
    assert_eq!(route_key_press(KeyCode::Digit3, alt), Some(Page::Services));
    assert_eq!(route_key_press(KeyCode::Digit5, alt), Some(Page::Startup));
    assert_eq!(route_key_press(KeyCode::Digit6, alt), Some(Page::Sessions));
    assert_eq!(route_key_press(KeyCode::Digit8, alt), Some(Page::Alerts));
}

#[test]
fn every_shared_page_chord_routes_and_system_is_formal() {
    // Alt+4 (System) is now the mounted host-facts route, completing the
    // shared page set; Alt+7 remains the application-history route.
    let alt = ModifierState {
        alt: true,
        ..ModifierState::default()
    };
    assert_eq!(route_key_press(KeyCode::Digit4, alt), Some(Page::System));
    assert_eq!(
        route_key_press(KeyCode::Digit7, alt),
        Some(Page::AppHistory)
    );
}

#[test]
fn app_history_chord_mounts_the_real_history_page_scene() {
    let mut app = headless_shell_app();
    app.update();
    app.update();
    press(&mut app, &alt(KeyCode::Digit7));
    app.update();
    app.update();
    assert_eq!(app.world().resource::<Route>().page, Page::AppHistory);
    assert_eq!(
        app.world_mut()
            .query_filtered::<&crate::pages::history::HistoryPageRoot, ()>()
            .iter(app.world())
            .count(),
        1,
        "Alt+7 mounts the history page rather than a placeholder"
    );
}

#[test]
fn settings_binding_is_frontend_local_and_unmodified_only() {
    // TUI parity: settings has no shared chord, so the frontend binds an
    // unmodified key. Bare `P` routes; modified `P` must NOT (Ctrl+P etc.
    // stay free for the shared vocabulary).
    let plain = ModifierState::default();
    assert_eq!(route_key_press(KeyCode::KeyP, plain), Some(Page::Settings));
    let ctrl = ModifierState {
        control: true,
        ..ModifierState::default()
    };
    assert_eq!(route_key_press(KeyCode::KeyP, ctrl), None);
    let alt = ModifierState {
        alt: true,
        ..ModifierState::default()
    };
    assert_eq!(route_key_press(KeyCode::KeyP, alt), None);
}

#[test]
fn bare_digits_do_not_route() {
    // The shared table requires Alt; a bare digit press must be a no-op so
    // future in-page digit bindings stay free.
    assert_eq!(
        route_key_press(KeyCode::Digit2, ModifierState::default()),
        None,
    );
}

#[test]
fn route_transitions_are_idempotent_and_explicit() {
    let route = Route {
        page: Page::Processes,
    };
    assert_eq!(route.go(Page::Processes), None, "re-route is a no-op");
    assert_eq!(route.go(Page::Alerts), Some(Page::Alerts));
}

#[test]
fn unshared_actions_never_invent_pages() {
    use taskmanager_application::{AppAction, AppPage, RefreshRequest};

    // The shared page set maps one-to-one; every shared SelectPage action
    // must route, and non-page actions must never invent a route.
    for shared in [
        AppPage::Applications,
        AppPage::Performance,
        AppPage::Services,
        AppPage::System,
        AppPage::Startup,
        AppPage::Users,
        AppPage::AppHistory,
    ] {
        assert_eq!(
            page_for_action(AppAction::SelectPage(shared)).map(|_| ()),
            Some(()),
            "the shared page {shared:?} must route"
        );
    }
    assert_eq!(
        page_for_action(AppAction::Refresh(RefreshRequest::All)),
        None,
    );
}

// ---- nav highlight model ----

#[test]
fn nav_highlight_model_uses_two_distinct_theme_surfaces() {
    let palette = ui_palette(&Theme::dark());
    let active = nav_item_background(true, &palette);
    let idle = nav_item_background(false, &palette);
    assert_eq!(active, palette.accent);
    assert_eq!(idle, palette.nav_active_bg);
    assert_ne!(
        active.to_srgba(),
        idle.to_srgba(),
        "an active rail item must be visually distinct from an idle one"
    );
}

// ---- wired routing on the real plugin composition ----

#[test]
fn keypress_moves_the_route_and_remounts_one_page_under_the_slot() {
    let mut app = headless_shell_app();
    // Startup + first Update: the initial page mounts.
    app.update();
    app.update();

    let route = app.world().resource::<Route>();
    assert_eq!(
        route.page,
        Page::Processes,
        "the default route is Processes"
    );

    let content = app
        .world_mut()
        .query_filtered::<(bevy::ecs::entity::Entity, &PageContent), ()>()
        .iter(app.world())
        .collect::<Vec<_>>();
    assert_eq!(content.len(), 1, "exactly one page content is mounted");
    let (content_entity, mounted) = content[0];
    assert_eq!(mounted.page, Page::Processes);

    let slot = app
        .world_mut()
        .query_filtered::<bevy::ecs::entity::Entity, With<ContentSlot>>()
        .single(app.world())
        .expect("the shell spawns exactly one content slot");
    let child_of = app
        .world()
        .get::<ChildOf>(content_entity)
        .expect("page content is parented into the content slot");
    assert_eq!(child_of.0, slot, "the mounted page lives under the slot");

    // Alt+1 → Performance (the shared router's chord), pressed via real
    // ButtonInput state through the real keyboard system.
    press(&mut app, &alt(KeyCode::Digit1));
    app.update();

    assert_eq!(
        app.world().resource::<Route>().page,
        Page::Performance,
        "Alt+1 moves the route through the shared router"
    );
    let remounted = app
        .world_mut()
        .query_filtered::<&PageContent, ()>()
        .iter(app.world())
        .collect::<Vec<_>>();
    assert_eq!(
        remounted.len(),
        1,
        "the old page content was despawned, not accumulated"
    );
    assert_eq!(remounted[0].page, Page::Performance);

    // The same key held must not churn: only just_pressed routes.
    app.update();
    assert_eq!(app.world().resource::<Route>().page, Page::Performance);
    let still = app
        .world_mut()
        .query_filtered::<&PageContent, ()>()
        .iter(app.world())
        .count();
    assert_eq!(still, 1, "a held key remounts nothing");
}

#[test]
fn route_change_restyles_the_nav_rail_through_the_observer() {
    let mut app = headless_shell_app();
    app.update();
    app.update();

    let rail = app
        .world_mut()
        .query_filtered::<(&NavTarget, &bevy::ui::BackgroundColor), ()>()
        .iter(app.world())
        .map(|(target, fill)| (target.0, fill.0))
        .collect::<Vec<_>>();
    assert_eq!(rail.len(), Page::ALL.len(), "one rail item per page");
    let palette = ui_palette(&Theme::dark());
    for (page, fill) in rail {
        let expected = nav_item_background(page == Page::Processes, &palette);
        assert_eq!(
            fill.to_srgba(),
            expected.to_srgba(),
            "initial fill for {page:?}"
        );
    }

    press(&mut app, &[KeyCode::KeyP]);
    app.update();
    assert_eq!(app.world().resource::<Route>().page, Page::Settings);

    let rail = app
        .world_mut()
        .query_filtered::<(&NavTarget, &bevy::ui::BackgroundColor), ()>()
        .iter(app.world())
        .map(|(target, fill)| (target.0, fill.0))
        .collect::<Vec<_>>();
    for (page, fill) in rail {
        let expected = nav_item_background(page == Page::Settings, &palette);
        assert_eq!(
            fill.to_srgba(),
            expected.to_srgba(),
            "highlight for {page:?}"
        );
    }
}

#[test]
fn nav_button_activation_moves_the_real_route_and_remounts_content() {
    let mut app = headless_shell_app();
    app.update();
    app.update();

    let settings = {
        let world = app.world_mut();
        let mut nav = world.query::<(Entity, &NavTarget)>();
        nav.iter(world)
            .find(|(_, target)| target.0 == Page::Settings)
            .map(|(entity, _)| entity)
            .expect("the settings nav item is a button scene")
    };
    app.world_mut()
        .commands()
        .trigger(Activate { entity: settings });
    app.update();
    app.update();

    assert_eq!(
        app.world().resource::<Route>().page,
        Page::Settings,
        "nav activation moves the same route resource as keyboard navigation"
    );
    let mounted = app
        .world_mut()
        .query::<&PageContent>()
        .iter(app.world())
        .map(|content| content.page)
        .collect::<Vec<_>>();
    assert_eq!(mounted, vec![Page::Settings]);
}

#[test]
fn nav_button_activation_applies_the_page_action_to_the_shell() {
    // Pointer navigation must follow the same rule as the keyboard chord: the
    // shell page tracks the visible page, so `CommandScope` derivation in the
    // shell's own routers stays correct no matter how the page was reached
    // (BEVY_UI_FRONTEND.md input seam).
    let mut app = headless_shell_app();
    app.update();
    app.update();

    let services = {
        let world = app.world_mut();
        let mut nav = world.query::<(Entity, &NavTarget)>();
        nav.iter(world)
            .find(|(_, target)| target.0 == Page::Services)
            .map(|(entity, _)| entity)
            .expect("the services nav item is a button scene")
    };
    app.world_mut()
        .commands()
        .trigger(Activate { entity: services });
    app.update();
    app.update();

    assert_eq!(app.world().resource::<Route>().page, Page::Services);
    let shell = &app.world().non_send::<crate::app::FrontendTrack>().shell;
    assert_eq!(
        shell.page(),
        taskmanager_application::AppPage::Services,
        "the pointer route wrote the shell page, not only the bevy route"
    );
    assert!(
        app.world()
            .resource::<crate::input::PendingEffects>()
            .0
            .is_empty(),
        "a page switch emits no platform effect"
    );
}

#[test]
fn programmatic_transition_remounts_through_the_observer_chain() {
    // The programmatic seam (menus, deep links): move the route resource and
    // trigger RouteChanged — the same pair the keyboard adapter performs.
    let mut app = headless_shell_app();
    app.update();
    app.update();
    app.world_mut().resource_mut::<Route>().page = Page::Services;
    app.world_mut().commands().trigger(RouteChanged);
    app.update();
    app.update();
    let mounted = app
        .world_mut()
        .query_filtered::<&PageContent, ()>()
        .iter(app.world())
        .collect::<Vec<_>>();
    assert_eq!(mounted.len(), 1);
    assert_eq!(
        mounted[0].page,
        Page::Services,
        "the observer chain remounts the newly routed page"
    );
}

/// Test-only marker: the seam-probe system below records what it could
/// read through the ShellTrack param.
#[derive(Default, bevy::ecs::resource::Resource)]
struct SeamProbe(Option<String>);

fn seam_probe_system(
    track: crate::app::ShellTrack,
    mut probe: bevy::ecs::system::ResMut<SeamProbe>,
) {
    let projection = track.shell().projection();
    let status = projection
        .capability_status(&CapabilityId::TELEMETRY_HOST)
        .map(|status| format!("{status:?}"))
        .unwrap_or_else(|| "missing".to_owned());
    let visible = track.shell().visible_processes().len();
    probe.0 = Some(format!("{status}/{visible}"));
}

#[test]
fn shell_track_param_reads_the_folded_projection() {
    // The page-agent data path end to end: drain folds the scripted
    // capability inventory in PreUpdate, and a page-shaped system reads it
    // back through the ShellTrack SystemParam in Update.
    let mut app = headless_shell_app();
    app.init_resource::<SeamProbe>();
    app.add_systems(bevy::app::Update, seam_probe_system);
    app.update();
    let probe = app.world().resource::<SeamProbe>();
    let seen = probe.0.clone().expect("the seam probe ran");
    assert!(
        seen.starts_with("Available/"),
        "the folded capability inventory is visible through ShellTrack: {seen}"
    );
    assert!(
        seen.ends_with("/0"),
        "no processes folded yet, honestly zero"
    );
}

#[test]
fn shell_spawns_the_full_page_surfaces() {
    let mut app = headless_shell_app();
    app.update();
    app.update();
    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<&SummaryLine, ()>()
            .iter(world)
            .count(),
        1,
        "one summary line"
    );
    assert_eq!(
        world.query_filtered::<&NavTarget, ()>().iter(world).count(),
        Page::ALL.len(),
        "one nav item per page"
    );
    assert_eq!(
        world
            .query_filtered::<&ContentSlot, ()>()
            .iter(world)
            .count(),
        1,
        "exactly one content slot"
    );
    assert_eq!(
        world
            .query_filtered::<&ProcessTreeSurface, ()>()
            .iter(world)
            .count(),
        1,
        "Applications route mounts the Bevy tree surface"
    );
    assert_eq!(
        world
            .query_filtered::<&SemanticAddress, ()>()
            .iter(world)
            .count(),
        0,
        "empty process projection does not invent semantic rows"
    );
}
