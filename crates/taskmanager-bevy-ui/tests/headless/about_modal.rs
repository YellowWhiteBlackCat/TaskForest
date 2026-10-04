//! test-intent: behavior

use super::*;
use crate::app::{FrontendTrack, Page, Route};
use crate::text_selection::flush_clipboard;
use crate::window::tests::scripted_frontend_app;
use bevy::ecs::change_detection::Mut;
use bevy::ecs::entity::Entity;
use taskmanager_shell::fixture;

#[test]
fn normal_about_control_renders_build_metadata_and_exports_reviewed_values() {
    let mut app = scripted_frontend_app();
    app.world_mut().resource_mut::<Route>().page = Page::System;
    app.update();
    app.world_mut().non_send_mut::<FrontendTrack>().shell = fixture::demo_app();
    let control = app
        .world_mut()
        .query::<(Entity, &AboutControl)>()
        .iter(app.world())
        .find(|(_, control)| matches!(control.0, AboutCommand::Open))
        .map(|(entity, _)| entity)
        .expect("normal System About control");
    app.world_mut().trigger(Activate { entity: control });
    app.update();
    assert!(matches!(
        app.world().resource::<WindowSurfaceState>().0,
        Some(WindowSurface::About)
    ));
    let expected = about_text();
    assert!(expected.contains("TaskForest"));
    assert!(
        expected.contains("Apache-2.0"),
        "application license: {expected}"
    );
    let body = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .find(|text| text.0 == expected);
    assert!(
        body.is_some(),
        "actual modal body must render the reviewed facts"
    );
    app.world_mut().trigger(AboutCommand::Copy);
    app.world_mut().flush();
    let mut written = Vec::new();
    app.world_mut()
        .resource_scope(|world, mut port: Mut<ClipboardPort>| {
            flush_clipboard(
                &mut port,
                &mut world.non_send_mut::<FrontendTrack>().shell,
                |value| {
                    written.push(value.to_owned());
                    Ok(())
                },
            );
        });
    assert_eq!(written, [expected]);
    app.world_mut().trigger(AboutCommand::Diagnostics);
    app.update();
    assert!(
        app.world()
            .resource::<WindowSurfaceState>()
            .diagnostic()
            .is_some()
    );
    app.world_mut().trigger(AboutCommand::Close);
    app.update();
    assert!(
        app.world()
            .resource::<WindowSurfaceState>()
            .diagnostic()
            .is_some(),
        "a stale About close cannot dismiss diagnostics"
    );
}

#[test]
fn independent_system_information_copies_the_frozen_review_and_ignores_stale_about_close() {
    use crate::system_information_modal::SystemInformationCommand;
    use taskmanager_shell::presentation::system_information::copy_all_text;
    let mut app = scripted_frontend_app();
    app.update();
    app.world_mut().non_send_mut::<FrontendTrack>().shell = fixture::demo_app();
    app.world_mut().trigger(AboutCommand::Open);
    app.update();
    app.world_mut().trigger(AboutCommand::SystemInformation);
    app.update();
    let Some(WindowSurface::SystemInformation(facts)) =
        &app.world().resource::<WindowSurfaceState>().0
    else {
        panic!("independent system information");
    };
    let expected = copy_all_text(facts);
    assert!(expected.contains("Linux"));
    fixture::edit_hardware(
        &mut app.world_mut().non_send_mut::<FrontendTrack>().shell,
        |hardware| {
            hardware.as_mut().expect("cached hardware").os_name = Some("new observation".into());
        },
    );
    app.world_mut().trigger(AboutCommand::Close);
    app.update();
    assert!(matches!(
        app.world().resource::<WindowSurfaceState>().0,
        Some(WindowSurface::SystemInformation(_))
    ));
    app.world_mut().trigger(SystemInformationCommand::Copy);
    app.world_mut().flush();
    let mut written = Vec::new();
    app.world_mut()
        .resource_scope(|world, mut port: Mut<ClipboardPort>| {
            flush_clipboard(
                &mut port,
                &mut world.non_send_mut::<FrontendTrack>().shell,
                |value| {
                    written.push(value.to_owned());
                    Ok(())
                },
            );
        });
    assert_eq!(written, [expected]);
    app.world_mut().trigger(SystemInformationCommand::Close);
    app.update();
    assert!(app.world().resource::<WindowSurfaceState>().0.is_none());
}

#[test]
fn native_boot_observes_desktop_appearance_once_and_information_uses_the_response() {
    use crate::app::SharedRuntimeHandle;
    use crate::runtime::SharedRuntime;
    use taskmanager_core::core::appearance::{
        DesktopAppearance, DesktopFamily, PreferredColorScheme,
    };
    use taskmanager_test_support::desktop_appearance::platform;
    let expected = DesktopAppearance {
        family: DesktopFamily::Kde,
        color_scheme: PreferredColorScheme::Dark,
        high_contrast: Some(false),
    };
    let (platform, recorder) = platform(expected);
    let mut app = scripted_frontend_app();
    let runtime = Box::leak(Box::new(SharedRuntime::new(platform)));
    app.insert_resource(SharedRuntimeHandle { shared: runtime });
    app.update();
    app.update();
    assert_eq!(recorder.submissions().expect("requests").len(), 1);
    assert_eq!(
        app.world()
            .resource::<crate::pages::settings::ThemePreferences>()
            .observed_appearance,
        Some(expected)
    );
    app.world_mut()
        .trigger(crate::system_information_modal::SystemInformationCommand::Open);
    app.update();
    let Some(WindowSurface::SystemInformation(facts)) =
        &app.world().resource::<WindowSurfaceState>().0
    else {
        panic!("native information");
    };
    assert!(
        facts
            .iter()
            .flat_map(|group| &group.rows)
            .any(|row| row.value == "KDE Plasma")
    );
    assert!(
        facts
            .iter()
            .flat_map(|group| &group.rows)
            .any(|row| row.label_key == "system_about.color_scheme")
    );
}
