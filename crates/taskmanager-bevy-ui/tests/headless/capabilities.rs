//! test-intent: behavior
//!
//! CORE-08 capability gate for the Bevy shape: the declaration is total and
//! reason-bearing, and every delivered-surface cell is pinned against the
//! real mounted page trees.
//!
//! Two layers:
//! - declaration: the fold has no findings and each cell's support kind is
//!   pinned, so a support change is a conscious edit here;
//! - delivered surface: the census mounts every route on the real
//!   `FrontendWindowPlugin` composition, walks the mounted page subtree, and
//!   counts both the authored widget surfaces (`TooltipSurface`,
//!   `SliderSurface`, `ScrollbarSurface`, `DropdownMenuSurface`) and the
//!   official `bevy_ui_widgets` mechanisms the Divergent cells name.
//!   Mounting an authored scene, or an official slider/scrollbar/popover,
//!   fails here until the declaration is updated in the same change.

use std::sync::Arc;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::With;
use bevy::ecs::world::World;
use bevy::image::Image;
use bevy::scene::{Scene, ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui_widgets::popover::Popover;
use bevy::ui_widgets::{
    Checkbox, MenuPopup, RadioButton, RadioGroup, ScrollArea, Scrollbar, Slider,
};
use taskmanager_application::{
    HostTelemetryRequest, PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle,
    SystemFacets,
};
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    EventEnvelope, EventPort, EventPortError, RequestEnvelope, RequestPort, SubmissionError,
};
use taskmanager_theme::Theme;
use taskmanager_ui_contract::{
    CapabilitySupport, ComponentCapability, FrontendShape, capability_findings, capability_report,
};

use crate::app::{Page, PageContent, Route, RouteChanged};
use crate::palette::ui_palette;
use crate::runtime::{RuntimeCache, SharedRuntime};
use crate::widgets::controls::{
    MIN_SCROLLBAR_THUMB_PX, ScrollbarOrientation, ScrollbarSurface, SliderState, SliderSurface,
    TooltipSpec, TooltipSurface, compute_scrollbar_geometry, scrollbar_scene, slider_scene,
    tooltip_scene,
};
use crate::widgets::menu::{
    DropdownMenuState, DropdownMenuSurface, MenuItem, MenuSpec, dropdown_menu_scene,
};
use crate::window::FrontendWindowPlugin;
use crate::window::tests::HeadlessFrontendPlugins;

// ---- fixtures -----------------------------------------------------------

/// Bare scene app for the authored widget scenes (pure nodes, no page
/// resources): the asset stores are the only infrastructure they resolve.
fn headless_scene_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app.init_resource::<Assets<Image>>();
    app
}

// ---- scripted platform client (the headless window composition) ----

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
    let snapshot = CapabilitySnapshot::from_descriptors([
        descriptor(CapabilityId::TELEMETRY_HOST, CapabilityStatus::Available),
        descriptor(CapabilityId::TELEMETRY_CPU, CapabilityStatus::Available),
        descriptor(
            CapabilityId::HARDWARE_INVENTORY,
            CapabilityStatus::Available,
        ),
    ]);
    let client = PlatformClient::new(PlatformHandle::new(
        Arc::new(FixedCapabilities(snapshot)),
        Arc::new(QuietEvents),
        PlatformFacets::default()
            .with_system(SystemFacets::default().with_host(Arc::new(QuietRequests))),
    ));
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    cache
        .get_or_init(move || Ok(client))
        .expect("the scripted runtime always starts")
}

/// The real `FrontendWindowPlugin` composition with no window: the same
/// resources and observers every mounted page reads in production.
fn headless_window_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(HeadlessFrontendPlugins);
    app.add_plugins(FrontendWindowPlugin {
        runtime: scripted_runtime(),
        palette: ui_palette(&Theme::dark()),
    });
    app.init_resource::<Assets<Font>>();
    app
}

/// The delivered-surface census of one mounted page subtree: the authored
/// widget families plus the official mechanisms the declaration names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SurfaceCensus {
    authored_tooltip: usize,
    authored_slider: usize,
    authored_scrollbar: usize,
    authored_dropdown: usize,
    official_slider: usize,
    official_scrollbar: usize,
    official_popover: usize,
    official_menu_popup: usize,
    official_checkbox: usize,
    official_radio_group: usize,
    official_radio_button: usize,
    scroll_areas: usize,
}

impl SurfaceCensus {
    fn add(&mut self, other: Self) {
        self.authored_tooltip += other.authored_tooltip;
        self.authored_slider += other.authored_slider;
        self.authored_scrollbar += other.authored_scrollbar;
        self.authored_dropdown += other.authored_dropdown;
        self.official_slider += other.official_slider;
        self.official_scrollbar += other.official_scrollbar;
        self.official_popover += other.official_popover;
        self.official_menu_popup += other.official_menu_popup;
        self.official_checkbox += other.official_checkbox;
        self.official_radio_group += other.official_radio_group;
        self.official_radio_button += other.official_radio_button;
        self.scroll_areas += other.scroll_areas;
    }
}

/// Whether `entity` is `root` or one of its descendants (the mounted page
/// subtree; the app shell above the content slot is deliberately excluded).
fn is_within(world: &World, entity: Entity, root: Entity) -> bool {
    let mut current = Some(entity);
    while let Some(candidate) = current {
        if candidate == root {
            return true;
        }
        current = world.get::<ChildOf>(candidate).map(ChildOf::parent);
    }
    false
}

fn count_in_subtree<T: Component>(world: &mut World, root: Entity) -> usize {
    let candidates: Vec<Entity> = {
        let mut state = world.query::<(Entity, &T)>();
        state.iter(world).map(|(entity, _)| entity).collect()
    };
    candidates
        .into_iter()
        .filter(|entity| is_within(world, *entity, root))
        .count()
}

fn census_subtree(world: &mut World, root: Entity) -> SurfaceCensus {
    SurfaceCensus {
        authored_tooltip: count_in_subtree::<TooltipSurface>(world, root),
        authored_slider: count_in_subtree::<SliderSurface>(world, root),
        authored_scrollbar: count_in_subtree::<ScrollbarSurface>(world, root),
        authored_dropdown: count_in_subtree::<DropdownMenuSurface>(world, root),
        official_slider: count_in_subtree::<Slider>(world, root),
        official_scrollbar: count_in_subtree::<Scrollbar>(world, root),
        official_popover: count_in_subtree::<Popover>(world, root),
        official_menu_popup: count_in_subtree::<MenuPopup>(world, root),
        official_checkbox: count_in_subtree::<Checkbox>(world, root),
        official_radio_group: count_in_subtree::<RadioGroup>(world, root),
        official_radio_button: count_in_subtree::<RadioButton>(world, root),
        scroll_areas: count_in_subtree::<ScrollArea>(world, root),
    }
}

/// Route to `page` through the real observer chain and census the mounted
/// page subtree.
fn routed_census(app: &mut App, page: Page) -> SurfaceCensus {
    app.world_mut().resource_mut::<Route>().page = page;
    app.world_mut().commands().trigger(RouteChanged);
    app.update();
    app.update();
    let root = {
        let world = app.world_mut();
        world
            .query_filtered::<Entity, With<PageContent>>()
            .single(world)
            .expect("exactly one page content mounts per route")
    };
    assert_eq!(
        app.world()
            .get::<PageContent>(root)
            .map(|content| content.page),
        Some(page),
        "the routed page is the mounted one"
    );
    census_subtree(app.world_mut(), root)
}

// ---- declaration --------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Native,
    Ported,
    Divergent,
    Unsupported,
}

fn kind(support: CapabilitySupport) -> Kind {
    match support {
        CapabilitySupport::Reference => panic!("the Bevy shape never owns the reference role"),
        CapabilitySupport::Native { .. } => Kind::Native,
        CapabilitySupport::Ported => Kind::Ported,
        CapabilitySupport::Divergent { .. } => Kind::Divergent,
        CapabilitySupport::Unsupported { .. } => Kind::Unsupported,
    }
}

/// The declaration is total, duplicate-free, reason-bearing, and pinned cell
/// by cell — the delivered-surface kinds the declaration audit measured.
#[test]
fn capability_declaration_is_complete_and_pinned() {
    use ComponentCapability::{
        Checkbox, ColumnDragResize, ContextMenu, DropdownMenu, FocusVisible, ModalOverlay,
        Scrollbar, SearchInput, SegmentedControl, Select, Slider, Switch, Table, TextInput,
        TextSelection, Toast, Tooltip, Tree, VirtualList,
    };

    let declaration = crate::capabilities::capability_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Bevy);
    assert!(capability_findings(&declaration).is_empty());
    assert_eq!(
        capability_report(&declaration).len(),
        ComponentCapability::ALL.len()
    );

    let expected: [(ComponentCapability, Kind); 19] = [
        (ModalOverlay, Kind::Ported),
        (ContextMenu, Kind::Ported),
        // Authored scene, zero mounts: no control-anchored popover is offered.
        (DropdownMenu, Kind::Unsupported),
        // No hover/focus explanation surface; hints render as inline captions.
        (Tooltip, Kind::Unsupported),
        (Toast, Kind::Divergent),
        (TextInput, Kind::Divergent),
        (SearchInput, Kind::Ported),
        // No selection surface at all; the editor buffer never leaves the
        // process (no system-clipboard write).
        (TextSelection, Kind::Unsupported),
        // Boolean rows ride the official Checkbox.
        (Switch, Kind::Divergent),
        // Bounded settings are discrete radio choices.
        (Slider, Kind::Divergent),
        (Checkbox, Kind::Ported),
        (Select, Kind::Ported),
        (SegmentedControl, Kind::Ported),
        (Table, Kind::Ported),
        (ColumnDragResize, Kind::Divergent),
        (VirtualList, Kind::Ported),
        (Tree, Kind::Ported),
        // Scrolling rides the official ScrollArea; no rail is mounted.
        (Scrollbar, Kind::Divergent),
        (FocusVisible, Kind::Divergent),
    ];

    for (capability, expected_kind) in expected {
        let support = declaration
            .entries
            .iter()
            .find(|entry| entry.capability == capability)
            .map(|entry| entry.support)
            .expect("every contract capability is declared");
        assert_eq!(
            kind(support),
            expected_kind,
            "capability `{}` support kind",
            capability.id()
        );
        if let Some(explanation) = support.explanation() {
            assert!(
                !explanation.is_empty(),
                "capability `{}` must carry its driver",
                capability.id()
            );
        }
    }
}

/// The authored widget scenes behind the `Unsupported`/`Divergent` cells
/// still assemble, carry their surface marker, and spawn: "authored but not
/// mounted" is the declared state, not a missing implementation.
#[test]
fn authored_widget_scenes_assemble_and_carry_their_surface_marker() {
    let palette = ui_palette(&Theme::dark());
    let mut app = headless_scene_app();

    let tooltip_spec = TooltipSpec::new("Search").with_key_hint("Ctrl+F");
    assert_eq!(
        census_authored(&mut app, tooltip_scene(&tooltip_spec, &palette)).authored_tooltip,
        1,
        "the tooltip bubble carries TooltipSurface"
    );

    let slider_state = SliderState::new(0.0, 100.0, 10.0, 50.0);
    assert_eq!(
        census_authored(&mut app, slider_scene(&slider_state, &palette)).authored_slider,
        1,
        "the slider carries SliderSurface"
    );

    let menu_spec = MenuSpec {
        title: "Actions".to_owned(),
        items: vec![MenuItem {
            label: "Run".to_owned(),
            enabled: true,
        }],
    };
    let mut dropdown_state = DropdownMenuState::default();
    assert_eq!(
        census_authored(
            &mut app,
            dropdown_menu_scene("Action".to_owned(), &menu_spec, &dropdown_state, &palette),
        )
        .authored_dropdown,
        1,
        "the closed dropdown carries DropdownMenuSurface"
    );
    dropdown_state.open();
    assert_eq!(
        census_authored(
            &mut app,
            dropdown_menu_scene("Action".to_owned(), &menu_spec, &dropdown_state, &palette),
        )
        .authored_dropdown,
        1,
        "the open dropdown carries DropdownMenuSurface"
    );

    let geo = compute_scrollbar_geometry(100.0, 300.0, 50.0, 200.0, MIN_SCROLLBAR_THUMB_PX);
    assert_eq!(
        census_authored(
            &mut app,
            scrollbar_scene(&geo, ScrollbarOrientation::Vertical, &palette),
        )
        .authored_scrollbar,
        1,
        "the vertical rail carries ScrollbarSurface"
    );
    assert_eq!(
        census_authored(
            &mut app,
            scrollbar_scene(&geo, ScrollbarOrientation::Horizontal, &palette),
        )
        .authored_scrollbar,
        1,
        "the horizontal rail carries ScrollbarSurface"
    );
}

/// One bare authoring scene's census (the scene root is the subtree).
fn census_authored(app: &mut App, scene: impl Scene) -> SurfaceCensus {
    let world = app.world_mut();
    let root = world
        .spawn_scene(scene)
        .expect("the authored scene resolves with no app resources")
        .id();
    let census = census_subtree(world, root);
    assert!(world.despawn(root), "the authored scene despawns cleanly");
    census
}

/// Every declared delivered-surface cell matches the mounted page trees: the
/// "not mounted" reasons are true today, and the mechanisms those reasons
/// name are really there.
#[test]
fn declared_cells_match_the_mounted_page_surface() {
    let mut app = headless_window_app();
    app.update();
    let mut totals = SurfaceCensus::default();
    let mut per_page: Vec<(Page, SurfaceCensus)> = Vec::new();
    for &page in Page::ALL {
        let census = routed_census(&mut app, page);
        totals.add(census);
        per_page.push((page, census));
    }
    let of = |page: Page| {
        per_page
            .iter()
            .find(|(routed, _)| *routed == page)
            .map(|(_, census)| *census)
            .expect("every route has a census")
    };

    // Authored surfaces: no page mounts one (the declaration's reasons say so).
    assert_eq!(
        totals.authored_tooltip, 0,
        "no page may mount a tooltip bubble while Tooltip is Unsupported"
    );
    assert_eq!(
        totals.authored_slider, 0,
        "no page may mount a slider while Slider is Divergent to radio choices"
    );
    assert_eq!(
        totals.authored_scrollbar, 0,
        "no page may mount a rail while Scrollbar is Divergent to ScrollArea"
    );
    assert_eq!(
        totals.authored_dropdown, 0,
        "no page may mount a dropdown while DropdownMenu is Unsupported"
    );

    // Official mechanisms that would replace a declared cell: absent too.
    assert_eq!(
        totals.official_slider, 0,
        "an official Slider mount must update the Slider declaration first"
    );
    assert_eq!(
        totals.official_scrollbar, 0,
        "an official Scrollbar mount must update the Scrollbar declaration first"
    );
    assert_eq!(
        totals.official_popover + totals.official_menu_popup,
        0,
        "an official Popover/MenuPopup mount must update the DropdownMenu declaration first"
    );

    // The mechanisms the Divergent reasons name are really mounted.
    let settings = of(Page::Settings);
    assert!(
        settings.official_checkbox >= 1,
        "boolean settings rows ride the official Checkbox (Switch cell): {settings:?}"
    );
    assert!(
        settings.official_radio_group >= 1 && settings.official_radio_button >= 1,
        "bounded settings ride discrete official radio choices (Slider cell): {settings:?}"
    );
    assert!(
        of(Page::Performance).scroll_areas >= 1,
        "the Performance scene scrolls through the official ScrollArea (Scrollbar cell)"
    );
}
