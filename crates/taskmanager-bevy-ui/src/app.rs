//! Frontend app shell: page routing, navigation, and the shell-state seam.
//!
//! This module is the composition point the M1+ page agents integrate with.
//! It owns four things, and nothing else in this crate may:
//!
//! 1. **The route model** ([`Page`], [`Route`]): a frontend-owned eight-page
//!    surface. The shared application vocabulary (`AppPage`) has no
//!    Processes/Settings/Alerts *page* shape — `AppAction::OpenAlerts` is
//!    explicitly "the route itself is frontend-owned" — so this enum is the
//!    bevy frontend's own navigation authority. It maps onto the shared
//!    pages where they exist and never redefines a shared page's meaning.
//! 2. **Keyboard routing** ([`route_key_press`]): bevy keyboard state is
//!    normalized into the shared `ShellKeyEvent` and routed through the same
//!    conflict-checked command router every frontend uses — same chord, same
//!    page semantics as the TUI (Alt+1 Performance, Alt+2 Applications, …,
//!    Alt+8 Alerts). The Settings surface has no shared chord (the TUI binds
//!    a frontend-local bare `p`), so it gets the same treatment here: a
//!    documented frontend-local binding on unmodified `P`.
//! 3. **The shell-state seam** ([`FrontendTrack`] + [`ShellTrack`]): the
//!    drain folds platform batches into one `ShellApp`, which lives in the
//!    bevy `World` as a non-send resource (it memoizes behind `Rc`/`RefCell`).
//!    [`ShellTrack`] is the typed [`SystemParam`] every page system reads the
//!    projection through — see its docs for the data-entry contract.
//! 4. **Page mounting** ([`PageContext`], [`PageContent`]): the currently
//!    routed page's content scene is spawned under the content slot and
//!    rebuilt on every accepted route change. Page modules expose one
//!    `content(&PageContext) -> impl Scene` function; see `crate::pages`.
//!
//! Static structure composes with `bsn!`; dynamic state (route changes, drain
//! folds) reaches the tree only through observers — never polling, never
//! imperative bulk spawn.

use bevy::app::{App, Plugin, Update};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::{Has, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, NonSend, Query, Res, ResMut, Single, SystemParam};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::picking::hover::PickingInteraction;
use bevy::scene::{CommandsSceneExt, Scene, bsn, on};
use bevy::text::{TextColor, TextFont};
use bevy::ui::Pressed;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, UiRect, Val,
    percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::{
    AppAction, AppPage, ApplicationHistoryProjection, CommandContext, CommandScope,
};
use taskmanager_shell::{ShellApp, SystemProjectionStore};

use crate::pages::history::HistoryProjectionResource;
use crate::palette::{UiPalette, space_2, space_8};
use crate::runtime::SharedRuntime;
use crate::widgets::controls::{ControlTone, ControlVisual, control_background};
use crate::widgets::layout::{COMPACT_NAV_WIDTH_PX, WIDE_NAV_WIDTH_PX};
use crate::window::{Role, TextRole, WindowPalette};

/// `'static` borrow of the process-wide runtime, held in the bevy `World` so
/// window rebuilds reuse the cached handle (charter boundary 5).
#[derive(Resource)]
pub(crate) struct SharedRuntimeHandle {
    pub(crate) shared: &'static SharedRuntime,
}

/// The frontend's shell track: one [`ShellApp`] per Bevy `App` plus the
/// one-shot initial refresh flag. Frontend state lives in the `World`; the
/// platform runtime never does.
///
/// `ShellApp` memoizes projections behind a `RefCell`/`Rc`, so it is neither
/// `Send` nor `Sync` — the track registers as a **non-send resource**
/// (bevy's main-thread-only container, the same contract GPUI's per-window
/// entities rely on) instead of a plain resource.
pub(crate) struct FrontendTrack {
    /// The renderer-neutral shell: sole fold target of the drain seam and
    /// the only projection authority any page may read.
    pub(crate) shell: ShellApp,
    pub(crate) initial_refresh_submitted: bool,
}

/// **The page-agent data entry.** Typed read-only view over the folded shell
/// projection, usable as a plain system parameter:
///
/// ```ignore
/// fn my_page_rows(track: ShellTrack, /* … */) {
///     let processes = track.shell().visible_processes();
///     let projection: &SystemProjectionStore = track.projection();
/// }
/// ```
///
/// A `SystemParam` (not a bare resource) so the borrow sits in the schedule:
/// the drain mutates the track in `PreUpdate`, page systems borrow it
/// read-only in `Update`, and bevy enforces the exclusion — a page can never
/// race a fold. `NonSend` is deliberate: the shell is main-thread state, and
/// this frontend runs every UI system on the app thread anyway.
#[derive(SystemParam)]
pub(crate) struct ShellTrack<'w> {
    track: NonSend<'w, FrontendTrack>,
}

impl ShellTrack<'_> {
    /// The renderer-neutral shell. Prefer [`Self::projection`] for plain
    /// data reads; use this for the memoized projections (`visible_processes`,
    /// `sorted_services`, …) whose cache lives on the shell.
    pub(crate) fn shell(&self) -> &ShellApp {
        &self.track.shell
    }

    /// Immutable view of the folded projection store: processes, services,
    /// startup entries, sessions, telemetry, alerts, source statuses, typed
    /// revisions. Facts enter only through the drain seam — a page can read
    /// but never write this store. Consumed by page systems (and the
    /// headless seam probe) from `Update`.
    #[allow(dead_code)]
    pub(crate) fn projection(&self) -> &SystemProjectionStore {
        self.track.shell.projection()
    }
}

/// One page of the bevy frontend's eight-page surface.
///
/// Order is single-sourced in [`Page::ALL`] (nav order and the test walk);
/// keyboard chords are NOT positional — they follow the shared command
/// router's semantics per page (see [`route_key_press`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Page {
    /// The M1 process table (shared page: `Applications`).
    #[default]
    Processes,
    /// Device/system curves (shared page: `Performance`).
    Performance,
    /// Service inventory (shared page: `Services`).
    Services,
    /// Startup entries + boot evidence (shared page: `Startup`).
    Startup,
    /// Login sessions (shared page: `Users`).
    Sessions,
    /// Alert center (frontend-owned route behind shared `OpenAlerts`).
    Alerts,
    /// Settings surface (frontend-owned; no shared chord, local `P` binding).
    Settings,
    /// Durable per-application history, reached through shared `Alt+7`.
    AppHistory,
}

impl Page {
    /// Single source of nav order; iteration and tests walk this, never a
    /// second copy of the variant list.
    pub(crate) const ALL: &'static [Page] = &[
        Page::Processes,
        Page::Performance,
        Page::Services,
        Page::Startup,
        Page::Sessions,
        Page::Alerts,
        Page::Settings,
        Page::AppHistory,
    ];

    /// Short rail label (nav item).
    pub(crate) const fn nav_label(self) -> &'static str {
        match self {
            Page::Processes => "Processes",
            Page::Performance => "Performance",
            Page::Services => "Services",
            Page::Startup => "Startup",
            Page::Sessions => "Sessions",
            Page::Alerts => "Alerts",
            Page::Settings => "Settings",
            Page::AppHistory => "App History",
        }
    }

    /// Content-region title.
    pub(crate) const fn title(self) -> &'static str {
        self.nav_label()
    }
}

/// The current route. One resource; transitions go through [`Route::go`], so
/// the page field never mutates without the [`RouteChanged`] trigger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Resource)]
pub(crate) struct Route {
    pub(crate) page: Page,
}

impl Route {
    /// Pure transition decision: the next page when it differs from the
    /// current one, `None` for an idempotent re-route (no churn, no event).
    pub(crate) fn go(self, next: Page) -> Option<Page> {
        (self.page != next).then_some(next)
    }
}

/// Fired (via `Commands::trigger`) after an accepted route change. The nav
/// highlight observer restyles the rail from it; the mount observer despawns
/// the outgoing page content. Route changes are the ONLY trigger — no polling.
#[derive(Event)]
pub(crate) struct RouteChanged(
    /// Grammar-complete today; observers key off the trigger itself, page
    /// analytics read the payload when it lands.
    #[allow(dead_code)]
    pub(crate) Page,
);

/// Nav-item identity marker. The `Default` seed only exists for the bsn!
/// template mechanism (template-then-patch); every spawned instance carries
/// its explicit page.
#[derive(Component, Clone, Default)]
pub(crate) struct NavTarget(pub(crate) Page);

/// Marker on the icon and label leaves of one route item. The route observer
/// updates their ink together with the parent fill; the text-role observer
/// still owns their font metrics.
#[derive(Component, Clone, Default)]
pub(crate) struct NavItemLabel;

/// Marker on the one node that hosts the routed page's content scene.
#[derive(Component, Clone, Default)]
pub(crate) struct ContentSlot;

/// Marker + identity of the currently mounted page content. Exactly one
/// instance exists while any page is mounted; route changes despawn it.
/// The identity is asserted by the headless remount tests until page
/// analytics read it in-product.
#[derive(Component)]
#[allow(dead_code)]
pub(crate) struct PageContent {
    pub(crate) page: Page,
}

/// Navigation highlight model: the fill a rail item renders with. Pure
/// function of (active, palette) so the model is testable without a world;
/// the observer below is its only applier.
pub(crate) fn nav_item_background(active: bool, palette: &UiPalette) -> bevy::color::Color {
    if active {
        palette.accent
    } else {
        palette.nav_active_bg
    }
}

/// Modifier state captured from `ButtonInput<KeyCode>`, normalized so the
/// routing decision is a pure function with no bevy resource in its signature.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ModifierState {
    pub(crate) control: bool,
    pub(crate) alt: bool,
    pub(crate) shift: bool,
    pub(crate) platform: bool,
}

/// Shared action → Bevy page. The shared System route remains intentionally
/// absent until this frontend has a typed System page; AppHistory is now a
/// formal route and consumes the read-only history projection.
pub(crate) fn page_for_action(action: AppAction) -> Option<Page> {
    match action {
        AppAction::SelectPage(AppPage::Applications) => Some(Page::Processes),
        AppAction::SelectPage(AppPage::Performance) => Some(Page::Performance),
        AppAction::SelectPage(AppPage::Services) => Some(Page::Services),
        AppAction::SelectPage(AppPage::Startup) => Some(Page::Startup),
        AppAction::SelectPage(AppPage::Users) => Some(Page::Sessions),
        AppAction::SelectPage(AppPage::AppHistory) => Some(Page::AppHistory),
        AppAction::OpenAlerts => Some(Page::Alerts),
        _ => None,
    }
}

/// Decide the routed page for one key press. Pure; the input system below is
/// only the bevy adapter around it.
///
/// Alignment with the TUI's page-switching semantics is chord-for-chord: the
/// shared router owns Alt+1..8, so Alt+2 opens Processes here exactly as it
/// opens Applications there. The Settings surface keeps the TUI's
/// frontend-local-binding convention (bare `p` there, unmodified `P` here)
/// because the shared vocabulary deliberately has no settings chord.
pub(crate) fn route_key_press(key: KeyCode, modifiers: ModifierState) -> Option<Page> {
    let plain = !modifiers.control && !modifiers.alt && !modifiers.platform;
    if plain && matches!(key, KeyCode::KeyP) {
        return Some(Page::Settings);
    }
    let action = crate::input_contract::normalize_key(
        key,
        crate::input_contract::InputModifiers {
            control: modifiers.control,
            alt: modifiers.alt,
            shift: modifiers.shift,
            platform: modifiers.platform,
        },
        CommandContext {
            scope: CommandScope::Global,
            ..CommandContext::default()
        },
    )?;
    page_for_action(action)
}

/// `Update` keyboard adapter: route the first just-pressed key that maps to a
/// page; on an accepted transition update the resource and trigger
/// [`RouteChanged`].
fn keyboard_route_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut route: ResMut<Route>,
    mut commands: Commands,
) {
    let modifiers = ModifierState {
        control: keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight),
        alt: keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight),
        shift: keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight),
        platform: keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight),
    };
    let next = keys
        .get_just_pressed()
        .find_map(|&key| route_key_press(key, modifiers));
    if let Some(next) = next
        && let Some(page) = route.go(next)
    {
        route.page = page;
        commands.trigger(RouteChanged(page));
    }
}

// Programmatic transitions (menus, deep links) perform the same pair: move
// the route resource, then trigger [`RouteChanged`]. Triggering the event
// without moving the resource is a protocol violation — the mount system
// reads the resource, not the payload.

/// Observer: restyle nav items after a route change. The highlight model is
/// [`nav_item_background`]; this observer is its only applier.
#[allow(clippy::type_complexity)]
fn highlight_nav_items(
    _changed: On<RouteChanged>,
    route: Res<Route>,
    palette: Res<WindowPalette>,
    mut items: Query<(
        &NavTarget,
        &Children,
        &mut ControlVisual,
        Option<&PickingInteraction>,
        Has<Pressed>,
        &mut BackgroundColor,
    )>,
    mut labels: Query<&mut TextColor, With<NavItemLabel>>,
) {
    for (target, children, mut visual, interaction, pressed, mut fill) in &mut items {
        let active = target.0 == route.page;
        *visual = ControlVisual(ControlTone::Nav, active);
        fill.0 = control_background(
            &visual,
            interaction.copied().unwrap_or_default(),
            pressed,
            &palette.inner,
        );
        let ink = if active {
            palette.inner.nav_active_ink
        } else {
            palette.inner.dim_color
        };
        for child in children.iter() {
            if let Ok(mut color) = labels.get_mut(*child) {
                color.0 = ink;
            }
        }
    }
}

/// Bevy 0.19 button activation for both wide and compact route items. Route
/// changes still follow the same resource-plus-event protocol as keyboard and
/// programmatic transitions.
fn nav_button_activated(
    activate: On<Activate>,
    targets: Query<&NavTarget>,
    mut route: ResMut<Route>,
    mut commands: Commands,
) {
    let Ok(target) = targets.get(activate.event().entity) else {
        return;
    };
    let Some(page) = route.go(target.0) else {
        return;
    };
    route.page = page;
    commands.trigger(RouteChanged(page));
}

/// Observer: despawn the outgoing page content and request a remount. The
/// respawn itself happens in [`mount_page_system`], which needs the
/// non-send shell track this observer deliberately avoids.
fn despawn_page_content(
    _changed: On<RouteChanged>,
    content: Query<Entity, With<PageContent>>,
    mut mount: ResMut<PageMount>,
    mut commands: Commands,
) {
    for entity in &content {
        commands.entity(entity).despawn();
    }
    mount.requested = true;
}

/// Mount bookkeeping: `requested` is set by the route observer, cleared by
/// the mount system once the routed page's scene is spawned.
#[derive(Default, Resource)]
pub(crate) struct PageMount {
    requested: bool,
    mounted: Option<Page>,
}

/// **Page-agent scene input.** Everything a page's `content` function may
/// read. Borrowed — scenes capture values by cloning, so nothing here
/// outlives the spawn call.
/// Fields are read by the page bodies; the history projection is a separate
/// immutable resource because application-history has a connector-owned
/// lifecycle rather than belonging to the live process projection.
#[allow(dead_code)]
pub(crate) struct PageContext<'a> {
    /// The shell: projection store + memoized row projections. Read-only.
    pub(crate) shell: &'a ShellApp,
    /// Resolved theme tokens for this window (see [`crate::palette`]).
    pub(crate) palette: &'a UiPalette,
    /// Body type metrics (size/weight; the style observers stamp the font
    /// handle, so pages never touch font assets).
    pub(crate) body: TextFont,
    /// Page-title type metrics.
    pub(crate) heading: TextFont,
    /// Read-only application-history projection from the app-host connector.
    pub(crate) history: &'a ApplicationHistoryProjection,
}

/// Dispatch the routed page to its module scene. The single place a page
/// module is registered.
pub(crate) fn page_scene(page: Page, context: &PageContext<'_>) -> Box<dyn Scene> {
    match page {
        Page::Processes => Box::new(crate::pages::processes::content(context)),
        Page::Performance => Box::new(crate::pages::performance::scene::content(context)),
        Page::Services => Box::new(crate::pages::services::content(context)),
        Page::Startup => Box::new(crate::pages::startup::content(context)),
        Page::Sessions => Box::new(crate::pages::sessions::content(context)),
        Page::Alerts => Box::new(crate::pages::alerts::content(context)),
        Page::Settings => Box::new(crate::pages::settings::content(context)),
        Page::AppHistory => Box::new(crate::pages::history::scene::content(
            context.history,
            context.palette,
        )),
    }
}

/// Mount (or remount) the routed page's content under the content slot.
/// Chained after the keyboard adapter: an accepted key press triggers the
/// despawn observer at the deferred sync point, and this system rebuilds the
/// content before the frame renders. The first frame mounts the initial page.
fn mount_page_system(
    track: ShellTrack,
    palette: Res<WindowPalette>,
    history: Res<HistoryProjectionResource>,
    route: Res<Route>,
    mut mount: ResMut<PageMount>,
    slot: Single<Entity, With<ContentSlot>>,
    mut commands: Commands,
) {
    if mount.mounted == Some(route.page) && !mount.requested {
        return;
    }
    let context = PageContext {
        shell: track.shell(),
        palette: &palette.inner,
        body: palette.inner.body.clone(),
        heading: palette.inner.heading.clone(),
        history: &history.0,
    };
    let entity = commands
        .spawn_scene(page_scene(route.page, &context))
        .insert(PageContent { page: route.page })
        .id();
    // Relate the fresh content to the slot: `add_one_related::<ChildOf>`
    // inserts ChildOf(slot) ON the given entity (the child side).
    commands.entity(*slot).add_one_related::<ChildOf>(entity);
    mount.mounted = Some(route.page);
    mount.requested = false;
}

/// The app-shell plugin: route resources, nav highlight, page mounting, and
/// the observers every window composition shares. The launcher supplies the
/// keyboard-input resource host through `DefaultPlugins`; headless tests
/// supply it through `HeadlessFrontendPlugins`.
pub(crate) struct AppShellPlugin;

impl Plugin for AppShellPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Route>()
            .init_resource::<PageMount>()
            .add_observer(highlight_nav_items)
            .add_observer(despawn_page_content)
            .add_systems(Update, (keyboard_route_system, mount_page_system).chain());
    }
}

/// Build one nav rail item scene (bsn! idiom: structure declarative, identity
/// via the `NavTarget` template value, ink via the text style observers).
fn nav_item_scene(page: Page, active: bool, palette: &UiPalette) -> impl Scene + use<> {
    let icon = nav_icon(page);
    let label = page.nav_label().to_owned();
    let height = palette.control_height_px * 1.25;
    let radius = palette.control_radius_px;
    let fill = nav_item_background(active, palette);
    let ink = if active {
        palette.nav_active_ink
    } else {
        palette.dim_color
    };
    bsn! {
        Node {
            width: percent(100),
            height: px(height),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::horizontal(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor(fill)
        NavTarget(page)
        ControlVisual(ControlTone::Nav, active)
        Button
        on(nav_button_activated)
        Children [
            ( Text(icon) TextRole(Role::Body) NavItemLabel TextColor(ink) ),
            ( Text(label) TextRole(Role::Caption) NavItemLabel TextColor(ink) ),
        ]
    }
}

pub(crate) fn nav_icon(page: Page) -> &'static str {
    match page {
        Page::Processes => "▦",
        Page::Performance => "◔",
        Page::Services => "≡",
        Page::Startup => "↗",
        Page::Sessions => "♙",
        Page::Alerts => "!",
        Page::Settings => "⚙",
        Page::AppHistory => "◫",
    }
}

fn compact_nav_item_scene(page: Page, active: bool, palette: &UiPalette) -> impl Scene + use<> {
    let height = palette.control_height_px * 1.25;
    let fill = nav_item_background(active, palette);
    let ink = if active {
        palette.nav_active_ink
    } else {
        palette.dim_color
    };
    bsn! {
        Node {
            width: percent(100),
            height: px(height),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor(fill)
        NavTarget(page)
        ControlVisual(ControlTone::Nav, active)
        Button
        on(nav_button_activated)
        Children [
            ( Text(nav_icon(page)) TextRole(Role::Body) NavItemLabel TextColor(ink) ),
        ]
    }
}

fn compact_nav_items(route: Page, palette: &UiPalette) -> Vec<impl Scene + use<>> {
    Page::ALL
        .iter()
        .map(|&page| compact_nav_item_scene(page, page == route, palette))
        .collect()
}

/// All rail items, one per [`Page::ALL`] entry. Returned as a scene list so
/// the rail's bsn! tree embeds it with one expression item.
fn nav_items(route: Page, palette: &UiPalette) -> Vec<impl Scene + use<>> {
    Page::ALL
        .iter()
        .map(|&page| nav_item_scene(page, page == route, palette))
        .collect()
}

/// The navigation rail scene: one item per page, newest-route-aware fill.
pub(crate) fn nav_rail_scene(route: Page, palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node {
            width: px(WIDE_NAV_WIDTH_PX),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_8())),
        }
        BackgroundColor({ palette.nav_bg })
        Children [
            { nav_items(route, palette) },
        ]
    }
}

/// Compact Performance navigation: the same route authority and observer
/// markers as the wide rail, reduced to icon-only controls so the main graph
/// keeps the usable width promised by the responsive contract.
pub(crate) fn compact_nav_rail_scene(route: Page, palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node {
            width: px(COMPACT_NAV_WIDTH_PX),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_8())),
        }
        BackgroundColor({ palette.nav_bg })
        Children [
            { compact_nav_items(route, palette) },
        ]
    }
}

#[cfg(test)]
#[path = "../tests/headless/app.rs"]
mod tests;
