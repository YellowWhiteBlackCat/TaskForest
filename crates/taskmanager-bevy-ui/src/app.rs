//! Frontend app shell: page routing, navigation, and the shell-state seam.
//!
//! This module is the composition point the M1+ page agents integrate with.
//! It owns four things, and nothing else in this crate may:
//!
//! 1. **The route model** ([`Page`], [`Route`]): a frontend-owned nine-page
//!    surface. The shared application vocabulary (`AppPage`) has no
//!    Processes/Settings/Alerts *page* shape — `AppAction::OpenAlerts` is
//!    explicitly "the route itself is frontend-owned" — so this enum is the
//!    bevy frontend's own navigation authority. It maps onto the shared
//!    pages where they exist and never redefines a shared page's meaning.
//! 2. **Keyboard routing** ([`route_key_press`]): the frontend-local route
//!    chords (Alt+1..8, bare `P`) resolve here; every other key is forwarded
//!    through the shell's own routers by [`crate::input`], the real-input
//!    seam — same chords, same page semantics as the TUI (Alt+1
//!    Performance, Alt+2 Applications, …, Alt+8 Alerts). The Settings surface
//!    has no shared chord (the TUI binds a frontend-local bare `p`), so it
//!    gets the same treatment here: a documented frontend-local binding on
//!    unmodified `P`.
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

#[cfg(test)]
#[path = "../tests/headless/app_support.rs"]
pub(crate) mod app_support;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::{Has, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, NonSend, NonSendMut, Query, Res, ResMut, Single, SystemParam};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::picking::hover::PickingInteraction;
use bevy::scene::{CommandsSceneExt, Scene, bsn, on, template_value};
use bevy::text::TextColor;
use bevy::ui::Pressed;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, Overflow,
    UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::{
    AppAction, AppPage, ApplicationHistoryProjection, CommandContext, CommandScope,
};

use taskmanager_shell::ShellApp;

use crate::input::{PendingEffects, ShellInteractionApplied};
use crate::pages::history::HistoryProjectionResource;
use crate::palette::{UiPalette, no_wrap_text, space_8, space_12};
use crate::runtime::SharedRuntime;
use crate::widgets::controls::{ControlTone, ControlVisual, control_background};
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
    /// Persistent expansion/collapse state for the Applications tree strip.
    /// It is frontend interaction state, not process data; keeping it beside
    /// the shell track prevents fold-driven scene rebuilds from resetting it.
    pub(crate) process_tree_expansion: crate::pages::process_tree::ProcessTreeExpansion,
}

/// **The page-agent data entry.** Typed read-only view over the folded shell
/// projection, usable as a plain system parameter:
///
/// ```ignore
/// fn my_page_rows(track: ShellTrack, /* … */) {
///     let processes = track.shell().visible_processes();
///     let projection = track.shell().projection();
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
    /// The renderer-neutral shell. Prefer its `projection()` for plain data
    /// reads; use it for the memoized projections (`visible_processes`,
    /// `sorted_services`, …) whose cache lives on the shell.
    pub(crate) fn shell(&self) -> &ShellApp {
        &self.track.shell
    }

    /// The Bevy-local expansion state consumed by the process-tree adapter.
    pub(crate) fn process_tree_expansion(
        &self,
    ) -> &crate::pages::process_tree::ProcessTreeExpansion {
        &self.track.process_tree_expansion
    }
}

/// One page of the bevy frontend's nine-page surface.
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
    /// Host identity/firmware/CPU/session facts (shared page: `System`).
    System,
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
    /// Rail label through the shared tab vocabulary — the same label fold
    /// every frontend uses (ARCH §8 semantic-parity law; Processes renders
    /// the Applications tab word, exactly like GPUI's nav). Locale keys:
    /// `tab.apps`, `tab.performance`, `tab.services`, `tab.startup`,
    /// `tab.users`, `tab.alerts`, `tab.settings`, `tab.apphistory`.
    #[must_use]
    pub(crate) fn nav_label(self) -> String {
        taskmanager_application::i18n::t(self.label_key()).to_owned()
    }

    /// The shared locale key for this page's tab word.
    pub(crate) const fn label_key(self) -> &'static str {
        match self {
            Page::Processes => "tab.apps",
            Page::Performance => "tab.performance",
            Page::Services => "tab.services",
            Page::System => "tab.system",
            Page::Startup => "tab.startup",
            Page::Sessions => "tab.users",
            Page::Alerts => "tab.alerts",
            Page::Settings => "tab.settings",
            Page::AppHistory => "tab.apphistory",
        }
    }

    /// Content-region title.
    pub(crate) fn title(self) -> String {
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
/// the outgoing page content. The route resource remains the sole page
/// authority; this event is only the change signal.
#[derive(Event)]
pub(crate) struct RouteChanged;

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
/// The identity is consumed by the window capture/visibility adapter and by
/// the headless remount tests.
#[derive(Component)]
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
        AppAction::SelectPage(AppPage::System) => Some(Page::System),
        AppAction::SelectPage(AppPage::Startup) => Some(Page::Startup),
        AppAction::SelectPage(AppPage::Users) => Some(Page::Sessions),
        AppAction::SelectPage(AppPage::AppHistory) => Some(Page::AppHistory),
        AppAction::OpenAlerts => Some(Page::Alerts),
        _ => None,
    }
}

/// Bevy page → shared action: the inverse of the mappable half of
/// [`page_for_action`]. Applied by the input seam when a route chord fires,
/// so the shell's page (and therefore its `CommandScope` derivation) follows
/// the visible page. Alerts moves the shell too (`OpenAlerts` is an
/// acknowledged shared action); Settings owns no shared page shape.
pub(crate) fn action_for_page(page: Page) -> Option<AppAction> {
    match page {
        Page::Processes => Some(AppAction::SelectPage(AppPage::Applications)),
        Page::Performance => Some(AppAction::SelectPage(AppPage::Performance)),
        Page::Services => Some(AppAction::SelectPage(AppPage::Services)),
        Page::System => Some(AppAction::SelectPage(AppPage::System)),
        Page::Startup => Some(AppAction::SelectPage(AppPage::Startup)),
        Page::Sessions => Some(AppAction::SelectPage(AppPage::Users)),
        Page::Alerts => Some(AppAction::OpenAlerts),
        Page::AppHistory => Some(AppAction::SelectPage(AppPage::AppHistory)),
        Page::Settings => None,
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

/// Capture the chord modifiers from `ButtonInput<KeyCode>` so the routing
/// decision stays a pure function with no bevy resource in its signature.
pub(crate) fn modifier_state(keys: &ButtonInput<KeyCode>) -> ModifierState {
    ModifierState {
        control: keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight),
        alt: keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight),
        shift: keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight),
        platform: keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight),
    }
}

// Programmatic transitions (menus, deep links) perform the same pair: move
// the route resource, then trigger [`RouteChanged`]. Triggering the event
// without moving the resource is a protocol violation — the mount system
// reads the resource, not the payload.

/// The single route-change protocol every transition shares: move the
/// resource, then trigger [`RouteChanged`]. Idempotent re-routes do neither.
pub(crate) fn request_route(route: &mut Route, page: Page, commands: &mut Commands) {
    if let Some(next) = route.go(page) {
        route.page = next;
        commands.trigger(RouteChanged);
    }
}

/// Observer: restyle nav items after a route change. The highlight model is
/// [`nav_item_background`]; this observer is its only applier. Icon plates
/// restyle through their [`crate::icons::IconInk`] sibling component — the
/// icon ink and the label ink always move together.
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
    mut inks: Query<&mut crate::icons::IconInk>,
    mut plates: Query<(&crate::icons::IconPlate, &mut bevy::ui::widget::ImageNode)>,
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
            if let Ok(mut icon_ink) = inks.get_mut(*child) {
                icon_ink.0 = ink;
                if let Ok((_, mut node)) = plates.get_mut(*child) {
                    node.color = ink;
                }
            }
        }
    }
}

/// Bevy 0.19 button activation for both wide and compact route items. Route
/// changes still follow the same resource-plus-event protocol as keyboard and
/// programmatic transitions.
///
/// Pointer navigation applies the same page action to the shell the keyboard
/// chord does (see [`crate::input`]): `CommandScope` derivation in the shell's
/// `dispatch_key` must follow the visible page, whichever way the page was
/// reached. The resulting platform effect joins [`PendingEffects`] so the drain
/// — the only holder of the client lock — submits it, and the shell mutation
/// publishes [`ShellInteractionApplied`] exactly like a key press.
fn nav_button_activated(
    activate: On<Activate>,
    targets: Query<&NavTarget>,
    mut track: NonSendMut<FrontendTrack>,
    mut route: ResMut<Route>,
    mut pending: ResMut<PendingEffects>,
    mut commands: Commands,
) {
    let Ok(target) = targets.get(activate.event().entity) else {
        return;
    };
    // The keyboard adapter signals a re-render for every accepted route chord
    // (effect or not) and stays silent for pages with no shared action — the
    // pointer path mirrors that exactly.
    if let Some(action) = action_for_page(target.0) {
        if let Some(effect) = track.shell.apply_action(action) {
            pending.0.push(effect);
        }
        commands.trigger(ShellInteractionApplied);
    }
    request_route(&mut route, target.0, &mut commands);
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
/// outlives the spawn call. The history projection is separate because
/// application-history has a connector-owned lifecycle rather than belonging
/// to the live process projection.
pub(crate) struct PageContext<'a> {
    /// The shell: projection store + memoized row projections. Read-only.
    pub(crate) shell: &'a ShellApp,
    /// Persistent Bevy-local process-tree expansion state.
    pub(crate) process_tree_expansion: &'a crate::pages::process_tree::ProcessTreeExpansion,
    /// Resolved theme tokens for this window (see [`crate::palette`]).
    pub(crate) palette: &'a UiPalette,
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
        Page::System => Box::new(crate::pages::system::content(context)),
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
        process_tree_expansion: track.process_tree_expansion(),
        palette: &palette.inner,
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
            // The input adapter drives every inventory modal on every key;
            // all four exist whether or not their pages ever mounted.
            .init_resource::<crate::menu_modal::MenuModal<
                crate::pages::processes::menu::ProcessMenuCtx,
            >>()
            .init_resource::<crate::menu_modal::MenuModal<
                crate::pages::services::menu::ServiceMenuCtx,
            >>()
            .init_resource::<crate::menu_modal::MenuModal<
                crate::pages::startup::menu::StartupMenuCtx,
            >>()
            .init_resource::<crate::menu_modal::MenuModal<
                crate::pages::sessions::menu::SessionMenuCtx,
            >>()
            .add_observer(highlight_nav_items)
            .add_observer(despawn_page_content)
            .add_plugins(crate::input::InputPlugin)
            .add_systems(
                Update,
                (
                    crate::input::keyboard_dispatch_system,
                    crate::pages::processes::input::scroll_intent_system,
                    mount_page_system,
                )
                    .chain(),
            );
    }
}

// ---- product navigation strip ---------------------------------------------
//
// GPUI parity shape: one horizontal strip — the shared page tabs followed by
// trailing route affordances. The tab set is the shared `AppPage::ALL`
// vocabulary in the shared order (the same order the Alt+1..7 router walks);
// Alerts and Settings have no shared tab slot, so they ride the strip's
// trailing icon buttons — the same slot GPUI's gear occupies. Icons resolve
// through the semantic registry (`IconId`), never through text codepoints.

/// The strip's tab set: exactly the shared pages this frontend implements,
/// in `AppPage::ALL` order. Tests pin this list against the shared constant.
pub(crate) const NAV_TABS: &[Page] = &[
    Page::Performance,
    Page::Processes,
    Page::Services,
    Page::System,
    Page::Startup,
    Page::Sessions,
    Page::AppHistory,
];

/// The semantic icon identity for a route. Total over `Page` so trailing
/// affordances (Alerts, Settings) resolve through the same table.
pub(crate) fn tab_icon(page: Page) -> taskmanager_ui_contract::IconId {
    use taskmanager_ui_contract::IconId;
    match page {
        Page::Performance => IconId::Performance,
        Page::Processes => IconId::Applications,
        Page::Services => IconId::Services,
        Page::System => IconId::System,
        Page::Startup => IconId::Startup,
        Page::Sessions => IconId::Users,
        Page::AppHistory => IconId::History,
        Page::Alerts => IconId::Alert,
        Page::Settings => IconId::Settings,
    }
}

/// One strip tab: the semantic icon plus the shared tab word, centered in an
/// evenly divided cell. The label is the only shrinking child (elastic
/// shrink with NoWrap + clip, GPUI's truncation contract) so a narrow window
/// ellipses labels instead of pushing the trailing buttons out of the strip.
fn nav_tab_scene(page: Page, active: bool, palette: &UiPalette) -> impl Scene + use<> {
    let label = page.nav_label().to_owned();
    let height = palette.control_height_px * 1.4;
    let radius = palette.control_radius_px;
    let fill = nav_item_background(active, palette);
    let ink = if active {
        palette.nav_active_ink
    } else {
        palette.dim_color
    };
    bsn! {
        Node {
            flex_grow: 1.0,
            min_width: px(0.0),
            height: px(height),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::horizontal(Val::Px(space_12())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor(fill)
        NavTarget(page)
        ControlVisual(ControlTone::Nav, active)
        Button
        on(nav_button_activated)
        Children [
            ( { crate::icons::icon_scene(tab_icon(page), 18.0, ink) } NavItemLabel ),
            (
                Node {
                    min_width: px(0.0),
                    flex_shrink: 1.0,
                    overflow: Overflow::clip_x(),
                }
                NavItemLabel
                Children [
                    ( Text(label) TextRole(Role::Body) NavItemLabel TextColor(ink) template_value(no_wrap_text()) ),
                ]
            ),
        ]
    }
}

/// Trailing strip affordance for the two frontend-owned routes: a compact
/// square icon button (Alerts bell, Settings gear) — GPUI's gear slot.
fn nav_trailing_scene(page: Page, active: bool, palette: &UiPalette) -> impl Scene + use<> {
    let height = palette.control_height_px * 1.4;
    let fill = nav_item_background(active, palette);
    let ink = if active {
        palette.nav_active_ink
    } else {
        palette.dim_color
    };
    bsn! {
        Node {
            width: px(height),
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
            ( { crate::icons::icon_scene(tab_icon(page), 18.0, ink) } NavItemLabel ),
        ]
    }
}

/// The navigation strip: shared tabs + trailing Alerts/Settings affordances,
/// on one nav-surface band. The single product chrome every page renders.
pub(crate) fn nav_strip_scene(route: Page, palette: &UiPalette) -> impl Scene + use<> {
    let tabs: Vec<Box<dyn Scene>> = NAV_TABS
        .iter()
        .map(|&page| Box::new(nav_tab_scene(page, page == route, palette)) as Box<dyn Scene>)
        .collect();
    let trailing: Vec<Box<dyn Scene>> = [Page::Alerts, Page::Settings]
        .iter()
        .map(|&page| Box::new(nav_trailing_scene(page, page == route, palette)) as Box<dyn Scene>)
        .collect();
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_8())),
        }
        BackgroundColor({ palette.nav_bg })
        Children [
            { tabs },
            ( Node { flex_grow: 1.0 } ),
            { trailing },
        ]
    }
}

#[cfg(test)]
#[path = "../tests/headless/app.rs"]
mod tests;
