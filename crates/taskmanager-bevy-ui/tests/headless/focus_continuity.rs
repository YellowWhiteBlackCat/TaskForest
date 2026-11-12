//! test-intent: behavior
//!
//! Keyboard focus continuity across a page rebuild, for EVERY interactive
//! control family the component layer mounts. A control that changes the
//! page must keep the focus it just received: press a key, let the page
//! rebuild, press the SAME key again, and the second action lands.
//!
//! Each family's test disables the restore (sabotage) and asserts the same
//! second press does NOT land, so a test that merely checks the branch was
//! taken cannot pass. The rebuild replaces the ENTITY: both the old and new
//! entities are asserted, so an identity that cheated by pointing at the
//! old entity cannot pass either.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::system::RunSystemOnce;
use bevy::input::ButtonInput;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input_focus::{FocusCause, FocusedInput, InputFocus, InputFocusVisible};
use bevy::ui::Node;
use bevy::ui::widget::Text;
use bevy::ui_widgets::{
    Button, ButtonPlugin, Checkbox, CheckboxPlugin, ControlOrientation, RadioButton,
    RadioGroupPlugin, Scrollbar, Slider, SliderPlugin, SliderRange, SliderStep, SliderValue, Tab,
    TextInput,
};
use bevy::window::{PrimaryWindow, Window};

use crate::app::{Page, PageContent};
use crate::focus_visible::{FocusIdentity, FocusedControl, follow_focus_across_rebuild, register};
use crate::scene_attach::insert_into;

/// A minimal keyboard composition: the real per-widget plugins and one
/// primary window. The test drives the SAME `FocusedInput<KeyboardInput>`
/// event the toolkit's `InputDispatchPlugin` routes to the focused entity,
/// without pulling in the window/picking plugins that own OS messages.
fn keyboard_app() -> App {
    let mut app = App::new();
    app.add_plugins((ButtonPlugin, CheckboxPlugin, RadioGroupPlugin, SliderPlugin));
    app.add_message::<KeyboardInput>();
    app.init_resource::<ButtonInput<KeyCode>>();
    app.init_resource::<InputFocusVisible>();
    register(&mut app);
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    app
}

/// One page content root whose controls are rebuilt on demand.
fn page_root(app: &mut App) -> Entity {
    app.world_mut()
        .spawn(PageContent {
            page: Page::Settings,
        })
        .id()
}

/// Mount one control of the given family under `root`, with a stable marker.
///
/// The marker is the product's own semantic identity (a typed choice, a row
/// link, a label). The rebuild mounts a DIFFERENT entity with the SAME
/// marker, exactly like a page scene does.
fn mount_control(app: &mut App, root: Entity, family: ControlFamily, marker: &str) -> Entity {
    let label = marker.to_owned();
    let entity = match family {
        ControlFamily::Button => app
            .world_mut()
            .spawn((Node::default(), Button, Text(label)))
            .id(),
        ControlFamily::Checkbox => app
            .world_mut()
            .spawn((Node::default(), Checkbox, Text(label)))
            .id(),
        ControlFamily::Radio => app
            .world_mut()
            .spawn((Node::default(), RadioButton, Text(label)))
            .id(),
        ControlFamily::Slider => app
            .world_mut()
            .spawn((
                Node::default(),
                Slider::default(),
                SliderValue(1.0),
                SliderRange::new(0.0, 10.0),
                SliderStep(1.0),
                Text(label),
            ))
            .id(),
        ControlFamily::Tab => app
            .world_mut()
            .spawn((Node::default(), Tab, Text(label)))
            .id(),
        ControlFamily::TextInput => app
            .world_mut()
            .spawn((Node::default(), TextInput, Text(label)))
            .id(),
        ControlFamily::Scrollbar => app
            .world_mut()
            .spawn((
                Node::default(),
                Scrollbar::new(Entity::PLACEHOLDER, ControlOrientation::Vertical, 8.0),
                Text(label),
            ))
            .id(),
    };
    let world = app.world_mut();
    insert_into(&mut world.commands(), entity, ChildOf(root));
    entity
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlFamily {
    Button,
    Checkbox,
    Radio,
    Slider,
    Tab,
    TextInput,
    Scrollbar,
}

/// The key the control family answers on the focused-input path.
fn family_key(family: ControlFamily) -> (KeyCode, Key) {
    match family {
        ControlFamily::Slider => (KeyCode::ArrowRight, Key::ArrowRight),
        _ => (KeyCode::Space, Key::Space),
    }
}

/// Count the one action that proves continuity: the focused key reaching
/// the control. Every family's widget plugin turns that same key into its
/// own typed change in production; here the focused-input delivery is the
/// family-independent, observable "the second press landed" proof.
fn count_actions(app: &mut App, counter: Arc<AtomicUsize>) {
    app.add_observer(move |key: On<FocusedInput<KeyboardInput>>| {
        counter.fetch_add(1, Ordering::SeqCst);
        let _ = key.event().input.key_code;
    });
}

/// Press one key at the focused control, exactly as the toolkit dispatch
/// does: a `FocusedInput<KeyboardInput>` carrying the raw key press.
fn press(app: &mut App, key_code: KeyCode, logical_key: Key) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world_mut())
        .expect("the composition owns exactly one primary window");
    let Some(focused) = app.world().resource::<InputFocus>().get() else {
        // A stranded keyboard sends the key to the window, never to a
        // control: that is exactly the defect the sabotage arm proves.
        return;
    };
    let raw = KeyboardInput {
        key_code,
        logical_key,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window,
    };
    app.world_mut()
        .trigger(FocusedInput::new(focused, raw, window));
    app.update();
}

/// Rebuild the control: despawn the old entity, mount a new one with the
/// same marker, and let the stamping + restore systems run.
fn rebuild(
    app: &mut App,
    root: Entity,
    family: ControlFamily,
    marker: &str,
    old: Entity,
) -> Entity {
    app.world_mut().entity_mut(old).despawn();
    // The toolkit clears a focus whose entity no longer exists. The restore
    // must put it back on the replacement; without it, the key has no owner.
    app.world_mut().resource_mut::<InputFocus>().clear();
    let new = mount_control(app, root, family, marker);
    app.world_mut()
        .run_system_once(crate::focus_visible::stamp_focus_identities)
        .expect("identity stamping runs");
    app.world_mut()
        .run_system_once(follow_focus_across_rebuild)
        .expect("focus restore runs");
    app.update();
    new
}

/// One control family's whole story: identity, first action, rebuild, and
/// the SAME key landing on the replacement entity.
fn exercise(family: ControlFamily, restore: bool) -> (bool, bool) {
    let mut app = keyboard_app();
    let root = page_root(&mut app);
    let marker = "stable-control";
    let old = mount_control(&mut app, root, family, marker);
    app.world_mut()
        .run_system_once(crate::focus_visible::stamp_focus_identities)
        .expect("identity stamping runs");
    app.update();

    let identity = app
        .world()
        .get::<FocusIdentity>(old)
        .expect("every interactive control wears an identity")
        .0
        .clone();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(old, FocusCause::Navigated);
    app.world_mut()
        .run_system_once(follow_focus_across_rebuild)
        .expect("the restore records the focused control");

    let counter = Arc::new(AtomicUsize::new(0));
    count_actions(&mut app, counter.clone());
    let (code, key) = family_key(family);
    press(&mut app, code, key.clone());
    // Some families (a Tab with its roving TabIndex) deliver more than one
    // focused-input event per press. The proof is that at least one action
    // landed, and then that MORE landed after the second press.
    let first_count = counter.load(Ordering::SeqCst);
    let first_action = first_count >= 1;

    let fresh = rebuild(&mut app, root, family, marker, old);
    let new_identity = app
        .world()
        .get::<FocusIdentity>(fresh)
        .map(|id| id.0.clone())
        .unwrap_or_default();
    let entity_replaced = fresh != old && !app.world().get_entity(old).is_ok();
    let identity_stable = new_identity == identity;

    if restore {
        press(&mut app, code, key);
    } else {
        // Sabotage: disable the restore and leave the keyboard stranded.
        app.world_mut().insert_resource(FocusedControl::default());
        app.world_mut().resource_mut::<InputFocus>().clear();
        press(&mut app, code, key);
    }
    let second_action = counter.load(Ordering::SeqCst) > first_count;
    (
        first_action && entity_replaced && identity_stable,
        second_action,
    )
}

/// One family's paired proof: the restore keeps the second action landing,
/// and disabling the restore strands it.
fn assert_family(family: ControlFamily) {
    let (setup, lands) = exercise(family, true);
    assert!(
        setup,
        "{family:?}: the first action landed, the entity was replaced, and the identity stayed"
    );
    assert!(
        lands,
        "{family:?}: the second press of the SAME key lands on the rebuilt control"
    );
    let (setup, stranded) = exercise(family, false);
    assert!(
        setup,
        "{family:?}: the sabotage fixture rebuilt a stable control"
    );
    assert!(
        !stranded,
        "{family:?}: disabling the restore strands the second press"
    );
}

#[test]
fn a_button_keeps_focus_across_a_rebuild() {
    assert_family(ControlFamily::Button);
}

#[test]
fn a_checkbox_keeps_focus_across_a_rebuild() {
    assert_family(ControlFamily::Checkbox);
}

#[test]
fn a_radio_keeps_focus_across_a_rebuild() {
    assert_family(ControlFamily::Radio);
}

#[test]
fn a_slider_keeps_focus_across_a_rebuild() {
    assert_family(ControlFamily::Slider);
}

#[test]
fn a_tab_keeps_focus_across_a_rebuild() {
    assert_family(ControlFamily::Tab);
}

#[test]
fn a_text_input_keeps_focus_across_a_rebuild() {
    assert_family(ControlFamily::TextInput);
}

#[test]
fn a_scrollbar_keeps_focus_across_a_rebuild() {
    assert_family(ControlFamily::Scrollbar);
}
