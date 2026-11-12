//! Modality-aware high-contrast keyboard focus indication (CORE-08 / FocusVisible).
//!
//! Delivers true parity with GPUI and Iced: interactive controls and navigation items only paint a
//! high-contrast focus ring when navigating via keyboard. Pointer clicks switch the modality to
//! [`InputModality::Pointer`], suppressing the focus ring immediately to avoid distracting outlines
//! on mouse activation.
//!
//! # Architecture
//!
//! Strictly governed by an explicit discrete state machine ([`InputModalityState`]) with discrete three-state
//! transitions ([`InputModality`]: `Programmatic`, `Keyboard`, `Pointer`). Zero scattered booleans.
//! Actual widgets opt into toolkit Tab navigation; the single [`InputFocus`] resource
//! selects the one control whose outline is painted. No second focus marker exists.
//!
//! The modality vocabulary and the focus-visible rule belong to the shared contract
//! (`taskmanager_ui_contract::InputModality`); this module owns only the Bevy ECS adapter and the
//! observation systems.
//!
//! A page rebuild replaces the focused control's ENTITY, not the control.
//! Every interactive control therefore wears a page-scoped [`FocusIdentity`],
//! resolved from the semantic markers its scene already owns;
//! [`follow_focus_across_rebuild`] rebinds the one [`InputFocus`] to the new
//! entity that carries the same identity.

use bevy::app::{App, PostUpdate, Update};
use bevy::ecs::component::Component;
use bevy::ecs::entity::{Entities, Entity};
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::message::MessageReader;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::query::{Changed, Has, Or, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::SystemParam;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::keyboard::{Key, KeyCode};
use bevy::input_focus::InputFocusSystems;
use bevy::input_focus::tab_navigation::{TabGroup, TabIndex, TabNavigationPlugin};
use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::picking::hover::PickingInteraction;
use bevy::ui::{CalculatedClip, ComputedNode, UiGlobalTransform};
use bevy::ui::{Node, Outline, Val};
use bevy::ui_widgets::ScrollIntoView;
use bevy::ui_widgets::{Button, Checkbox, RadioButton, Scrollbar, Slider, Tab, TextInput};
use bevy::window::PrimaryWindow;
use std::collections::HashMap;

use crate::app::containers::ContainerRowMarker;
use crate::app::{NavTarget, PageContent};
use crate::confirmation::ConfirmationOverlay;
use crate::input::SearchEditor;
use crate::pages::processes::columns_modal::ProcessColumnsPopover;
use crate::pages::processes::properties_modal::ProcessPropertiesOverlay;
use crate::pages::settings::SettingsChoice;
use crate::scene_attach::insert_into;
use crate::spatial_focus::FocusTarget;
use crate::text_selection::ReadoutInput;
use crate::widgets::menu::MenuItemIndex;
use crate::widgets::scene_paint::ScenePaint;
use crate::widgets::switch::SwitchId;
use crate::widgets::table::ProcessSortHeader;
use crate::widgets::table::resize::ColumnWidthNode;
use crate::window::{AppShellRoot, DemoMode, WindowPalette};
use taskmanager_ui_contract::InputModality;

/// Explicit discrete state machine resource tracking the active input modality.
#[derive(Resource, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct InputModalityState {
    modality: InputModality,
}

impl InputModalityState {
    #[must_use]
    pub(crate) fn modality(&self) -> InputModality {
        self.modality
    }

    #[must_use]
    pub(crate) fn shows_focus_ring(&self) -> bool {
        self.modality().shows_focus_ring()
    }

    /// Transition to a new input modality.
    /// Returns true if the state changed, allowing caller to trigger an observer event.
    pub(crate) fn transition(&mut self, next: InputModality) -> bool {
        self.modality.transition(next)
    }
}

/// A real focusable widget, decorated without changing its layout footprint.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusRing;

/// Marker for the actual control whose keyboard outline is currently painted.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusRingVisible;

/// A stable, page-scoped identity for a focusable control.
///
/// The toolkit focus points at an *entity*, but a page rebuild spawns a new
/// entity for the same control — so a focus that lived on the entity would
/// be lost at the first rebuild, while GPUI's focus handle survives it. The
/// identity lets the focus follow the control instead of the entity.
///
/// The identity is a semantic path (`scope/family/name#ordinal`) resolved by
/// [`stamp_focus_identities`] from the product markers the scene already
/// owns. It is NOT the entity id: a rebuild must produce the same string
/// for a different entity. A scene may author one explicitly (the slider's
/// page-scoped owner); the generic resolver never overwrites it.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusIdentity(pub(crate) String);

/// The control that owned the keyboard focus, remembered across a rebuild.
#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct FocusedControl {
    entity: Option<Entity>,
    identity: Option<String>,
}

type UndecoratedControls<'w, 's> = Query<
    'w,
    's,
    (Entity, Has<Checkbox>, Has<RadioButton>),
    (
        With<Node>,
        Without<FocusRing>,
        Or<(
            With<Button>,
            With<Checkbox>,
            With<RadioButton>,
            With<Slider>,
        )>,
    ),
>;

fn decorate_controls(mut commands: Commands, controls: UndecoratedControls) {
    for (entity, checkbox, radio) in &controls {
        // Text switches have no opaque fill, so an inset ring remains visible
        // and stays inside the scroll viewport. Filled buttons need an outset.
        let offset = if checkbox || radio { -2.0 } else { 0.0 };
        insert_into(
            &mut commands,
            entity,
            (
                FocusRing,
                FocusTarget,
                TabIndex(0),
                // The inner ring's width comes from the shared focus-ring
                // seam (the same 2px the halo layer is measured against) — a
                // literal here would be a second width authority.
                Outline::new(
                    Val::Px(crate::widgets::focus_ring::focus_ring_inner_width()),
                    Val::Px(offset),
                    bevy::color::Color::NONE,
                ),
            ),
        );
    }
}

type UndecoratedModals<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        Without<TabGroup>,
        Or<(
            With<ConfirmationOverlay>,
            With<ProcessPropertiesOverlay>,
            With<ProcessColumnsPopover>,
        )>,
    ),
>;

type FocusableControls<'w, 's> =
    Query<'w, 's, (), Or<(With<Button>, With<Checkbox>, With<RadioButton>)>>;

/// The family of one interactive primitive, so two different controls with
/// the same label can never claim the same identity. The family is the first
/// behavior primitive a control carries, in the order the product composes
/// them (a search field is a `Button` before it is a `TextInput`).
fn control_family(flags: ControlFlags) -> &'static str {
    if flags.button {
        "button"
    } else if flags.checkbox {
        "checkbox"
    } else if flags.radio {
        "radio"
    } else if flags.slider {
        "slider"
    } else if flags.tab {
        "tab"
    } else if flags.text_input {
        "text-input"
    } else if flags.scrollbar {
        "scrollbar"
    } else {
        "control"
    }
}

/// The behavior primitives and semantic markers one control may carry.
///
/// Declared as one query row so the resolver never needs unrestricted world
/// access. Every field is optional: a scene composes exactly one behavior
/// primitive plus, where it owns one, one typed semantic marker.
#[derive(Clone, Copy, Default)]
struct ControlFlags {
    button: bool,
    checkbox: bool,
    radio: bool,
    slider: bool,
    tab: bool,
    text_input: bool,
    scrollbar: bool,
}

/// The semantic name one control's scene-owned marker resolves to.
///
/// Product scenes already stamp a typed marker on the controls that own one
/// (`SettingsChoice`, `NavTarget`, `ProcessRowLink`, …). Where no marker
/// exists — a plain labelled button — the control's own text is the name,
/// and the ordinal in [`stamp_focus_identities`] separates same-name
/// siblings. The marker list is a resolver, never a second identity owner.
fn control_semantic_name(markers: ControlMarkersRow<'_>) -> String {
    if let Some(choice) = markers.settings {
        return settings_choice_identity(&choice.0);
    }
    if let Some(target) = markers.nav {
        return format!("nav:{:?}", target.0);
    }
    if let Some(link) = markers.process_row {
        return format!("process-row:{}", link.0);
    }
    if let Some(row) = markers.service_row {
        return format!("service:{}:{:?}", row.0, row.1);
    }
    if let Some(row) = markers.startup_row {
        return format!("startup:{}:{:?}", row.0, row.1);
    }
    if let Some(row) = markers.container_row {
        return format!("container:{}", row.0);
    }
    if let Some(row) = markers.tree_row {
        return format!("tree:{row:?}");
    }
    if let Some(cpu) = markers.affinity_cpu {
        return format!("affinity:{}", cpu.0);
    }
    if let Some(item) = markers.menu_item {
        return format!("menu:{}", item.0);
    }
    if let Some(header) = markers.sort_header {
        return format!("sort:{}", header.0);
    }
    if let Some(column) = markers.column_width {
        return format!("column:{}", column.0);
    }
    if let Some(switch) = markers.switch {
        return format!("switch:{}", switch.0);
    }
    if let Some(readout) = markers.readout {
        return format!("readout:{}", readout.0);
    }
    if let Some(editor) = markers.search {
        return format!("search:{}", editor.0);
    }
    markers
        .text
        .map(|text| format!("text:{}", text.0.trim()))
        .filter(|name| *name != "text:")
        .unwrap_or_else(|| "control".to_owned())
}

/// The value-independent name of one settings control.
///
/// A settings choice carries the value it would WRITE next. A boolean
/// switch flips that payload on the very rebuild its own activation
/// triggers (`HighContrast(true)` ↔ `HighContrast(false)`), while a radio
/// group's choices are distinct controls that must stay distinct. The
/// identity therefore names the field for a toggle and keeps the value for
/// a discrete choice — never the transient "next" state of a switch.
fn settings_choice_identity(field: &crate::pages::settings::SettingsField) -> String {
    use crate::pages::settings::SettingsField;
    match field {
        SettingsField::HighContrast(_)
        | SettingsField::HistoryPersistence(_)
        | SettingsField::GrayZeroValues(_)
        | SettingsField::PauseTelemetry => format!("settings:{:?}", field_name(field)),
        _ => format!("settings:{field:?}"),
    }
}

fn field_name(field: &crate::pages::settings::SettingsField) -> &'static str {
    use crate::pages::settings::SettingsField;
    match field {
        SettingsField::Theme(_) => "theme",
        SettingsField::SystemMode => "system_mode",
        SettingsField::HighContrast(_) => "high_contrast",
        SettingsField::Language(_) => "language",
        SettingsField::HistoryCapacity(_) => "history_capacity",
        SettingsField::HistoryPersistence(_) => "history_persistence",
        SettingsField::GrayZeroValues(_) => "gray_zero_values",
        SettingsField::UiSize(_) => "ui_size",
        SettingsField::PauseTelemetry => "pause_telemetry",
    }
}

#[derive(SystemParam)]
pub(crate) struct ControlMarkers<'w, 's> {
    settings: Query<'w, 's, &'static SettingsChoice>,
    nav: Query<'w, 's, &'static NavTarget>,
    process_row: Query<'w, 's, &'static crate::pages::processes::ProcessRowLink>,
    service_row: Query<'w, 's, &'static crate::pages::services::ServicesRowMarker>,
    startup_row: Query<'w, 's, &'static crate::pages::startup::StartupRowMarker>,
    container_row: Query<'w, 's, &'static ContainerRowMarker>,
    tree_row: Query<'w, 's, &'static crate::pages::process_tree::ProcessTreeRowMarker>,
    affinity_cpu: Query<'w, 's, &'static crate::pages::processes::affinity::AffinityCpuButton>,
    menu_item: Query<'w, 's, &'static MenuItemIndex>,
    sort_header: Query<'w, 's, &'static ProcessSortHeader>,
    column_width: Query<'w, 's, &'static ColumnWidthNode>,
    switch: Query<'w, 's, &'static SwitchId>,
    readout: Query<'w, 's, &'static ReadoutInput>,
    search: Query<'w, 's, &'static SearchEditor>,
    text: Query<'w, 's, &'static bevy::ui::widget::Text>,
}

impl ControlMarkers<'_, '_> {
    /// Resolve the one row's markers into the values the identity needs.
    fn values(&self, entity: Entity) -> ControlMarkersRow<'_> {
        ControlMarkersRow {
            settings: self.settings.get(entity).ok(),
            nav: self.nav.get(entity).ok(),
            process_row: self.process_row.get(entity).ok(),
            service_row: self.service_row.get(entity).ok(),
            startup_row: self.startup_row.get(entity).ok(),
            container_row: self.container_row.get(entity).ok(),
            tree_row: self.tree_row.get(entity).ok(),
            affinity_cpu: self.affinity_cpu.get(entity).ok(),
            menu_item: self.menu_item.get(entity).ok(),
            sort_header: self.sort_header.get(entity).ok(),
            column_width: self.column_width.get(entity).ok(),
            switch: self.switch.get(entity).ok(),
            readout: self.readout.get(entity).ok(),
            search: self.search.get(entity).ok(),
            text: self.text.get(entity).ok(),
        }
    }
}

/// The marker values resolved for one entity.
struct ControlMarkersRow<'w> {
    settings: Option<&'w SettingsChoice>,
    nav: Option<&'w NavTarget>,
    process_row: Option<&'w crate::pages::processes::ProcessRowLink>,
    service_row: Option<&'w crate::pages::services::ServicesRowMarker>,
    startup_row: Option<&'w crate::pages::startup::StartupRowMarker>,
    container_row: Option<&'w ContainerRowMarker>,
    tree_row: Option<&'w crate::pages::process_tree::ProcessTreeRowMarker>,
    affinity_cpu: Option<&'w crate::pages::processes::affinity::AffinityCpuButton>,
    menu_item: Option<&'w MenuItemIndex>,
    sort_header: Option<&'w ProcessSortHeader>,
    column_width: Option<&'w ColumnWidthNode>,
    switch: Option<&'w SwitchId>,
    readout: Option<&'w ReadoutInput>,
    search: Option<&'w SearchEditor>,
    text: Option<&'w bevy::ui::widget::Text>,
}

/// The page scope of one control, derived from the mounted page content.
///
/// A control outside any page content (a modal, a nav tab, a window edge)
/// belongs to the shell, so its identity is stable for the shell's own
/// lifetime and cannot collide with a page-local control.
fn page_scope(entity: Entity, parents: &Query<&ChildOf>, pages: &Query<&PageContent>) -> String {
    let mut current = entity;
    for _ in 0..64 {
        if let Ok(page) = pages.get(current) {
            return format!("page:{:?}", page.page);
        }
        let Ok(parent) = parents.get(current) else {
            break;
        };
        current = parent.parent();
    }
    "shell".to_owned()
}

type IdentityCandidates<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Has<Button>,
        Has<Checkbox>,
        Has<RadioButton>,
        Has<Slider>,
        Has<Tab>,
        Has<TextInput>,
        Has<Scrollbar>,
    ),
    Or<(
        With<Button>,
        With<Checkbox>,
        With<RadioButton>,
        With<Slider>,
        With<Tab>,
        With<TextInput>,
        With<Scrollbar>,
    )>,
>;

/// Stamp every mounted interactive control with its stable, page-scoped
/// identity.
///
/// The system runs every frame over the small set of mounted controls, in
/// entity order — a page rebuild spawns its controls in the same order, so
/// the ordinal is stable. A scene-authored `FocusIdentity` (the slider's
/// page-scoped owner) is kept, never overwritten.
pub(crate) fn stamp_focus_identities(
    mut commands: Commands,
    candidates: IdentityCandidates,
    identities: Query<&FocusIdentity>,
    parents: Query<&ChildOf>,
    pages: Query<&PageContent>,
    markers: ControlMarkers,
) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (entity, button, checkbox, radio, slider, tab, text_input, scrollbar) in candidates.iter() {
        if identities.get(entity).is_ok() {
            continue;
        }
        let flags = ControlFlags {
            button,
            checkbox,
            radio,
            slider,
            tab,
            text_input,
            scrollbar,
        };
        let key = format!(
            "{}/{}/{}",
            page_scope(entity, &parents, &pages),
            control_family(flags),
            control_semantic_name(markers.values(entity))
        );
        let ordinal = counts.entry(key.clone()).or_insert(0);
        let identity = format!("{key}#{ordinal}");
        *ordinal += 1;
        insert_into(&mut commands, entity, FocusIdentity(identity));
    }
}

fn decorate_groups(
    mut commands: Commands,
    roots: Query<Entity, (With<AppShellRoot>, Without<TabGroup>)>,
    modals: UndecoratedModals,
    children: Query<&Children>,
    controls: FocusableControls,
) {
    for entity in &roots {
        insert_into(&mut commands, entity, TabGroup::new(0));
    }
    for entity in &modals {
        insert_into(&mut commands, entity, TabGroup::modal());
        if let Some(first) = children
            .iter_descendants(entity)
            .find(|child| controls.contains(*child))
        {
            insert_into(&mut commands, first, AutoFocus);
        }
    }
}

/// Follow the keyboard focus across a page rebuild.
///
/// A rebuild replaces the focused control's *entity*, not the control: the
/// focus is restored to the control that carries the remembered identity, so
/// a keyboard-driven change (a slider step) does not strand the keyboard.
/// A focus the user moved elsewhere is never overridden — only a focus
/// whose entity no longer exists is restored.
pub(crate) fn follow_focus_across_rebuild(
    mut focus: ResMut<InputFocus>,
    mut remembered: ResMut<FocusedControl>,
    controls: Query<(Entity, &FocusIdentity)>,
    entities: &Entities,
) {
    if let Some(entity) = focus.get() {
        if entities.contains(entity) {
            remembered.entity = Some(entity);
            remembered.identity = controls.get(entity).ok().map(|(_, id)| id.0.clone());
            return;
        }
        // A stale entity is a rebuild, not focus the user moved: leave the
        // remembered control intact and look for its replacement below.
    } else if let Some(entity) = remembered.entity
        && entities.contains(entity)
    {
        // The user (or the toolkit) cleared the focus on purpose: a live
        // remembered control must not be forced back onto the keyboard.
        return;
    }
    let Some(identity) = remembered.identity.clone() else {
        return;
    };
    let Some((entity, _)) = controls.iter().find(|(_, id)| id.0 == identity) else {
        return;
    };
    focus.set(entity, bevy::input_focus::FocusCause::Navigated);
    remembered.entity = Some(entity);
}

/// Demo-only capture arm for the `settings-switch-focus` scenario: name the
/// High-contrast switch directly, because the live window's tab order is not
/// guaranteed to reach a page-level switch before the capture deadline (the
/// GPUI and Iced capture paths likewise focus their switch programmatically).
fn arm_settings_switch_focus(
    demo: Option<Res<DemoMode>>,
    mut modality: ResMut<InputModalityState>,
    mut focus: ResMut<InputFocus>,
    targets: Query<(Entity, &SettingsChoice), With<FocusRing>>,
) {
    if demo.is_none() || crate::capture::capture_scenario_target() != Some("settings-switch-focus")
    {
        return;
    }
    let Some((entity, _)) = targets.iter().find(|(_, choice)| {
        matches!(
            choice.0,
            crate::pages::settings::SettingsField::HighContrast(_)
        )
    }) else {
        return;
    };
    if focus.get() == Some(entity) && modality.shows_focus_ring() {
        return;
    }
    modality.transition(InputModality::Keyboard);
    focus.set(entity, bevy::input_focus::FocusCause::Navigated);
}

/// Toolkit focus is the authority; moving focus updates the ring even when
/// the input modality stays Keyboard across successive Tab presses.
fn paint_focus_ring(
    focus: Res<InputFocus>,
    modality: Res<InputModalityState>,
    palette: Option<Res<WindowPalette>>,
    mut controls: Query<(Entity, &mut Outline, Option<&FocusRingVisible>), With<FocusRing>>,
    mut commands: Commands,
) {
    let color = palette
        .as_ref()
        .map_or(bevy::color::Color::WHITE, |p| p.inner.focus_ring);
    for (entity, mut outline, visible) in &mut controls {
        let show = focus.get() == Some(entity) && modality.shows_focus_ring();
        outline.color = if show {
            color
        } else {
            bevy::color::Color::NONE
        };
        if show && visible.is_none() {
            insert_into(&mut commands, entity, FocusRingVisible);
        } else if !show && visible.is_some() {
            commands.entity(entity).remove::<FocusRingVisible>();
        }
    }
}

fn reveal_keyboard_focus(
    focus: Res<InputFocus>,
    modality: Res<InputModalityState>,
    mut commands: Commands,
) {
    if modality.shows_focus_ring()
        && let Some(entity) = focus.get()
    {
        commands.trigger(ScrollIntoView { entity });
    }
}

type FocusControlQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static ComputedNode,
        &'static UiGlobalTransform,
        Option<&'static CalculatedClip>,
        Has<Checkbox>,
        Has<FocusRing>,
        Has<FocusRingVisible>,
    ),
>;

/// Capture uses normal Tab messages and waits for the actual painted control.
#[derive(SystemParam)]
pub(crate) struct FocusCapture<'w, 's> {
    focus: Res<'w, InputFocus>,
    controls: FocusControlQuery<'w, 's>,
    windows: Query<'w, 's, Entity, With<PrimaryWindow>>,
    keyboard: MessageWriter<'w, KeyboardInput>,
}
pub(crate) fn capture_ready(access: &mut FocusCapture, switch: bool) -> bool {
    if let Some(entity) = access.focus.get()
        && let Ok((node, transform, clip, checkbox, ring, ring_visible)) =
            access.controls.get(entity)
        && (!switch || checkbox)
        && ring
    {
        let half = node.outlined_node_size() * 0.5;
        let visible = clip.is_none_or(|clip| {
            clip.contains_point(transform.translation - half)
                && clip.contains_point(transform.translation + half)
        });
        return ring_visible && node.size().x > 0.0 && node.size().y > 0.0 && visible;
    }
    if crate::capture::capture_scenario_target() == Some("settings-switch-focus") {
        return false;
    }
    let Some(window) = access.windows.iter().next() else {
        return false;
    };
    access.keyboard.write(KeyboardInput {
        key_code: KeyCode::Tab,
        logical_key: Key::Tab,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window,
    });
    false
}
/// System observing keyboard activity: any keyboard press transitions the modality to [`InputModality::Keyboard`].
pub(crate) fn focus_visible_keyboard_system(
    mut modality: ResMut<InputModalityState>,
    mut key_events: MessageReader<KeyboardInput>,
) {
    for event in key_events.read() {
        if event.state == ButtonState::Pressed {
            modality.transition(InputModality::Keyboard);
            break;
        }
    }
}

/// System observing pointer activity: pointer presses transition the modality to [`InputModality::Pointer`].
pub(crate) fn focus_visible_pointer_system(
    mut modality: ResMut<InputModalityState>,
    query: Query<&PickingInteraction, Changed<PickingInteraction>>,
) {
    for interaction in &query {
        if *interaction == PickingInteraction::Pressed {
            modality.transition(InputModality::Pointer);
            break;
        }
    }
}

/// Register focus-visible modality resources, observer, and input monitoring systems.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<InputModalityState>();
    app.init_resource::<InputFocus>();
    app.init_resource::<FocusedControl>();
    if !app.is_plugin_added::<TabNavigationPlugin>() {
        app.add_plugins(TabNavigationPlugin);
    }
    // Identity stamping and the restore land before the toolkit routes this
    // frame's keys: a rebuild's new controls are named in time for the
    // restore to find them.
    app.add_systems(
        bevy::app::PreUpdate,
        (stamp_focus_identities, follow_focus_across_rebuild)
            .chain()
            .before(InputFocusSystems::Dispatch),
    );
    app.add_systems(
        PostUpdate,
        arm_settings_switch_focus.chain().after(ScenePaint),
    );
    app.add_systems(
        PostUpdate,
        (decorate_groups, decorate_controls, paint_focus_ring)
            .chain()
            .after(arm_settings_switch_focus),
    );
    // A page mounts its controls during Update; stamping in PostUpdate
    // names them in the same frame, so the first key after a rebuild has
    // an identity to follow. Registered separately so the decorator chain
    // keeps its original schedule.
    // A page mounts its controls during Update; stamping in PostUpdate
    // names them in the same frame, so the first key after a rebuild has
    // an identity to follow. Registered separately so the decorator chain
    // keeps its original schedule.
    app.add_systems(PostUpdate, stamp_focus_identities.after(ScenePaint));
    // A page mounts its controls during Update; stamping in PostUpdate
    // names them in the same frame, so the first key after a rebuild has
    // an identity to follow. Registered separately so the decorator chain
    // keeps its original schedule.
    app.add_systems(
        PostUpdate,
        reveal_keyboard_focus.after(bevy::ui::UiSystems::PostLayout),
    );
    app.add_systems(
        Update,
        (focus_visible_keyboard_system, focus_visible_pointer_system),
    );
}

#[cfg(test)]
#[path = "../tests/headless/focus_visible.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/headless/focus_continuity.rs"]
mod continuity_tests;
